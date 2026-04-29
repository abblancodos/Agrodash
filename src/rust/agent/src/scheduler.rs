// agent/src/scheduler.rs
//
// Ejecuta los nodos de un pipeline en orden topológico.
// Soporta fan-in (múltiples entradas a un nodo) y
// fan-out (una salida a múltiples nodos).

use std::collections::{HashMap, VecDeque};
use anyhow::{Context, Result};
use agrodash_shared::{
    NodeConfig, EdgeConfig, Signal, NodeAction,
    NodeState, SignalSnapshot, AgentState,
};
use sqlx::PgPool;

use crate::nodes::{self, NodeInstance};

// ── Graph ─────────────────────────────────────────────────────────────────────

pub struct PipelineGraph {
    /// Instancias de nodos — keyed por node_id
    nodes:    HashMap<String, Box<dyn NodeInstance>>,
    /// Orden topológico de ejecución
    topo:     Vec<String>,
    /// Edges: de → [a, ...]
    out_edges: HashMap<String, Vec<String>>,
    /// Edges: a → [de, ...]
    in_edges:  HashMap<String, Vec<String>>,
}

impl PipelineGraph {
    /// Construir el grafo a partir del config.
    pub async fn build(
        nodes_cfg:  &[NodeConfig],
        edges_cfg:  &[EdgeConfig],
        shared_connections: &Option<agrodash_shared::SharedConnections>,
        pool:       &PgPool,
    ) -> Result<Self> {
        // Construir instancias de nodos
        let mut nodes = HashMap::new();
        for node_cfg in nodes_cfg {
            let inst = nodes::build(node_cfg, shared_connections, pool).await
                .with_context(|| format!("Error construyendo nodo '{}'", node_cfg.id))?;
            nodes.insert(node_cfg.id.clone(), inst);
        }

        // Construir mapa de edges
        let mut out_edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_edges:  HashMap<String, Vec<String>> = HashMap::new();
        for e in edges_cfg {
            out_edges.entry(e.from.clone()).or_default().push(e.to.clone());
            in_edges.entry(e.to.clone()).or_default().push(e.from.clone());
        }

        // Orden topológico (Kahn's algorithm)
        let topo = topological_sort(&nodes, &out_edges, &in_edges)
            .context("El grafo tiene ciclos — no es un DAG válido")?;

        Ok(Self { nodes, topo, out_edges, in_edges })
    }

    /// Restaurar estado desde DB (warmstart).
    pub fn load_state(&mut self, state: &AgentState) {
        for (node_id, node_state) in &state.node_states {
            if let Some(inst) = self.nodes.get_mut(node_id) {
                inst.load_state(node_state);
            }
        }
    }

    /// Ejecutar un ciclo completo del pipeline.
    /// Retorna el mapa de señales producidas por cada nodo.
    pub async fn run_cycle(
        &mut self,
        pool:        &PgPool,
        dt:          f64,
        override_act: &Option<NodeAction>,
    ) -> Result<HashMap<String, Signal>> {
        let mut signals: HashMap<String, Signal> = HashMap::new();

        for node_id in &self.topo.clone() {
            // Recolectar señales de entrada
            let inputs: Vec<Signal> = self.in_edges
                .get(node_id)
                .map(|froms| {
                    froms.iter()
                        .filter_map(|from| signals.get(from).cloned())
                        .collect()
                })
                .unwrap_or_default();

            let node = self.nodes.get_mut(node_id)
                .context(format!("Nodo '{}' no encontrado", node_id))?;

            // Inyectar override en actuadores
            let output = if node.is_actuator() {
                if let Some(action) = override_act {
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

    /// Serializar estado actual de todos los nodos.
    pub fn save_state(&self, pipeline_id: &str, cycle: u64) -> AgentState {
        let node_states: HashMap<String, NodeState> = self.nodes.iter()
            .map(|(id, node)| (id.clone(), node.save_state()))
            .collect();

        let last_signals: HashMap<String, SignalSnapshot> = HashMap::new();

        AgentState {
            pipeline_id: pipeline_id.to_string(),
            node_states,
            last_signals,
            cycle,
        }
    }

    /// True si todos los nodos con warmup están listos.
    pub fn is_ready(&self) -> bool {
        self.nodes.values().all(|n| n.is_ready())
    }
}

// ── Topological sort (Kahn's algorithm) ──────────────────────────────────────

fn topological_sort(
    nodes:     &HashMap<String, Box<dyn NodeInstance>>,
    out_edges: &HashMap<String, Vec<String>>,
    in_edges:  &HashMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    // In-degree por nodo
    let mut in_degree: HashMap<String, usize> = nodes.keys()
        .map(|id| (id.clone(), in_edges.get(id).map(|v| v.len()).unwrap_or(0)))
        .collect();

    // Queue: nodos sin dependencias
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
                if *deg == 0 {
                    queue.push_back(succ.clone());
                }
            }
        }
    }

    if order.len() != nodes.len() {
        anyhow::bail!("El grafo tiene ciclos");
    }

    Ok(order)
}
