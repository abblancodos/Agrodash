// agent/src/scheduler.rs

use std::collections::{HashMap, VecDeque};
use anyhow::{Context, Result};
use agrodash_shared::{
    NodeConfig, EdgeConfig, Signal, NodeAction,
    NodeState, AgentState,
};
use sqlx::PgPool;

use crate::nodes::{self, NodeInstance};

// ── Graph ─────────────────────────────────────────────────────────────────────

pub struct PipelineGraph {
    nodes:     HashMap<String, Box<dyn NodeInstance>>,
    topo:      Vec<String>,
    out_edges: HashMap<String, Vec<String>>,
    in_edges:  HashMap<String, Vec<String>>,
}

impl PipelineGraph {
    pub async fn build(
        nodes_cfg:          &[NodeConfig],
        edges_cfg:          &[EdgeConfig],
        shared_connections: &Option<agrodash_shared::SharedConnections>,
        pool:               &PgPool,
    ) -> Result<Self> {
        let mut nodes = HashMap::new();
        for node_cfg in nodes_cfg {
            let inst = nodes::build(node_cfg, shared_connections, pool).await
                .with_context(|| format!("Error construyendo nodo '{}'", node_cfg.id))?;
            nodes.insert(node_cfg.id.clone(), inst);
        }

        let mut out_edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_edges:  HashMap<String, Vec<String>> = HashMap::new();
        for e in edges_cfg {
            out_edges.entry(e.from.clone()).or_default().push(e.to.clone());
            in_edges.entry(e.to.clone()).or_default().push(e.from.clone());
        }

        let topo = topological_sort(&nodes, &out_edges, &in_edges)
            .context("El grafo tiene ciclos — no es un DAG válido")?;

        Ok(Self { nodes, topo, out_edges, in_edges })
    }

    pub fn load_state(&mut self, state: &AgentState) {
        for (node_id, node_state) in &state.node_states {
            if let Some(inst) = self.nodes.get_mut(node_id) {
                inst.load_state(node_state);
            }
        }
    }

    /// Ciclo normal — override por actuator_id (HashMap vacío = modo automático).
    pub async fn run_cycle(
        &mut self,
        pool:         &PgPool,
        dt:           f64,
        overrides:    &HashMap<String, NodeAction>,
    ) -> Result<HashMap<String, Signal>> {
        let mut signals: HashMap<String, Signal> = HashMap::new();

        for node_id in &self.topo.clone() {
            let inputs = self.collect_inputs(node_id, &signals);

            let node = self.nodes.get_mut(node_id)
                .context(format!("Nodo '{node_id}' no encontrado"))?;

            let output = if node.is_actuator() {
                if let Some(action) = overrides.get(node_id) {
                    node.execute_override(action.clone()).await?
                } else {
                    node.execute(inputs, dt, pool).await?
                }
            } else {
                node.execute(inputs, dt, pool).await?
            };

            if let Some(sig) = output {
                signals.insert(node_id.clone(), sig);
            }
        }

        Ok(signals)
    }

    /// Dry-run: ejecuta todos los nodos excepto actuadores.
    /// Útil para self-test: verifica que sensores y filtros funcionan
    /// sin disparar válvulas ni dispositivos físicos.
    pub async fn run_cycle_dry(
        &mut self,
        pool: &PgPool,
        dt:   f64,
    ) -> Result<HashMap<String, Signal>> {
        let mut signals: HashMap<String, Signal> = HashMap::new();

        for node_id in &self.topo.clone() {
            let inputs = self.collect_inputs(node_id, &signals);

            let node = self.nodes.get_mut(node_id)
                .context(format!("Nodo '{node_id}' no encontrado"))?;

            // Saltear actuadores en dry-run
            if node.is_actuator() { continue; }

            let output = node.execute(inputs, dt, pool).await?;
            if let Some(sig) = output {
                signals.insert(node_id.clone(), sig);
            }
        }

        Ok(signals)
    }

    pub fn save_state(&self, pipeline_id: &str, cycle: u64, label: &str, override_active: bool) -> AgentState {
        let node_states: HashMap<String, NodeState> = self.nodes.iter()
            .map(|(id, node)| (id.clone(), node.save_state()))
            .collect();

        AgentState {
            pipeline_id:     pipeline_id.to_string(),
            node_states,
            last_signals:    HashMap::new(),
            cycle,
            is_ready:        self.is_ready(),
            override_active,
            label:           label.to_string(),
        }
    }

    /// Recoger scope_values — para cada Logger del grafo, retorna {tag: señal}
    /// usando las señales del ciclo actual. El agente llama esto después de run_cycle.
    pub fn collect_scope_values(
        &self,
        signals: &HashMap<String, Signal>,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        for (node_id, node) in &self.nodes {
            let state = node.save_state();
            if state.node_type != "logger" { continue; }

            let tag = state.data.get("tag")
                .and_then(|v| v.as_str())
                .unwrap_or(node_id)
                .to_string();

            // La señal del Logger es la que él mismo emitió (= la de su input)
            if let Some(sig) = signals.get(node_id) {
                let val = match sig {
                    Signal::Vector(v) => serde_json::json!(v),
                    Signal::Action(a) => serde_json::json!(format!("{a:?}").to_lowercase()),
                };
                map.insert(tag, val);
            }
        }

        serde_json::Value::Object(map)
    }

    /// IDs de todos los nodos actuador del grafo.
    pub fn actuator_ids(&self) -> Vec<String> {
        self.nodes.iter()
            .filter(|(_, n)| n.is_actuator())
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn is_ready(&self) -> bool {
        self.nodes.values().all(|n| n.is_ready())
    }

    /// Retorna el NodeState del primer nodo del tipo dado.
    /// Usado por el loop principal para extraer raw/filtered/p_diag.
    pub fn node_state_by_type(&self, node_type: &str) -> Option<NodeState> {
        self.nodes.values()
            .map(|n| n.save_state())
            .find(|ns| ns.node_type == node_type)
    }

    // ── Interno ───────────────────────────────────────────────────────────────

    fn collect_inputs(&self, node_id: &str, signals: &HashMap<String, Signal>) -> Vec<Signal> {
        self.in_edges
            .get(node_id)
            .map(|froms| froms.iter()
                .filter_map(|from| signals.get(from).cloned())
                .collect())
            .unwrap_or_default()
    }
}

// ── Topological sort (Kahn's algorithm) ──────────────────────────────────────

fn topological_sort(
    nodes:     &HashMap<String, Box<dyn NodeInstance>>,
    out_edges: &HashMap<String, Vec<String>>,
    in_edges:  &HashMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    let mut in_degree: HashMap<String, usize> = nodes.keys()
        .map(|id| (id.clone(), in_edges.get(id).map(|v| v.len()).unwrap_or(0)))
        .collect();

    let mut queue: VecDeque<String> = in_degree.iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();

    while let Some(id) = queue.pop_front() {
        order.push(id.clone());
        if let Some(successors) = out_edges.get(&id) {
            for succ in successors {
                let deg = in_degree.get_mut(succ).unwrap();
                *deg -= 1;
                if *deg == 0 { queue.push_back(succ.clone()); }
            }
        }
    }

    if order.len() != nodes.len() {
        anyhow::bail!("El grafo tiene ciclos");
    }

    Ok(order)
}