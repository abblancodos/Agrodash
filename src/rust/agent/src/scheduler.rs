// agent/src/scheduler.rs
//
// Ejecuta los nodos de un pipeline en orden topológico.
// Soporta fan-in (múltiples entradas a un nodo) y
// fan-out (una salida a múltiples nodos).

use std::collections::{HashMap, VecDeque};
use anyhow::{Context, Result};
use agrodash_shared::{
    NodeConfig, EdgeConfig, Signal, NodeAction,
    NodeState, SignalSnapshot, AgentState, SharedConnections,
};
use sqlx::PgPool;

use crate::nodes::{self, NodeInstance};

// ── CycleResult — señales + extractos para process_readings ──────────────────

pub struct CycleResult {
    pub signals:   HashMap<String, Signal>,
    /// Primer vector raw encontrado (fuente)
    pub raw:       Option<Vec<f64>>,
    /// Primer vector filtrado encontrado (kalman/ewma/etc.)
    pub filtered:  Option<Vec<f64>>,
    /// Diagonal de covarianza del kalman si existe
    pub p_diag:    Option<Vec<f64>>,
    /// Decisión numérica del decisor (distancia, llr, etc.)
    pub decision:  Option<f64>,
    /// Acción del actuador
    pub actuator:  String,
}

// ── Graph ─────────────────────────────────────────────────────────────────────

pub struct PipelineGraph {
    nodes:     HashMap<String, Box<dyn NodeInstance>>,
    topo:      Vec<String>,
    out_edges: HashMap<String, Vec<String>>,
    in_edges:  HashMap<String, Vec<String>>,
    /// Tipos de nodos para poder extraer señales específicas
    node_types: HashMap<String, String>,
}

impl PipelineGraph {
    /// Construir el grafo a partir del config.
    pub async fn build(
        nodes_cfg:            &[NodeConfig],
        edges_cfg:            &[EdgeConfig],
        shared_connections:   &Option<SharedConnections>,
        pipeline_connections: &Option<SharedConnections>,
        pool:                 &PgPool,
    ) -> Result<Self> {
        let mut nodes = HashMap::new();
        let mut node_types = HashMap::new();

        for node_cfg in nodes_cfg {
            // Guardar tipo para extracción de señales
            let type_str = format!("{:?}", node_cfg.kind)
                .split('(').next().unwrap_or("unknown")
                .to_lowercase();
            node_types.insert(node_cfg.id.clone(), type_str);

            let inst = nodes::build(
                node_cfg,
                shared_connections,
                pipeline_connections,
                pool,
            ).await.with_context(|| format!("Error construyendo nodo '{}'", node_cfg.id))?;
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

        Ok(Self { nodes, topo, out_edges, in_edges, node_types })
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
    pub async fn run_cycle(
        &mut self,
        pool:         &PgPool,
        dt:           f64,
        override_act: &Option<NodeAction>,
    ) -> Result<CycleResult> {
        let mut signals: HashMap<String, Signal> = HashMap::new();

        for node_id in &self.topo.clone() {
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

        // Extraer datos para process_readings
        let result = self.extract_readings(&signals);
        Ok(result)
    }

    /// Extraer raw, filtered, p_diag, decision y actuator de las señales del ciclo.
    fn extract_readings(&self, signals: &HashMap<String, Signal>) -> CycleResult {
        let mut raw:      Option<Vec<f64>> = None;
        let mut filtered: Option<Vec<f64>> = None;
        let mut p_diag:   Option<Vec<f64>> = None;
        let mut decision: Option<f64>      = None;
        let mut actuator  = "hold".to_string();

        for (node_id, signal) in signals {
            let node_type = self.node_types.get(node_id).map(|s| s.as_str()).unwrap_or("");

            match signal {
                Signal::Vector(v) => {
                    match node_type {
                        t if t.contains("postgres_sensor") => {
                            if raw.is_none() { raw = Some(v.clone()); }
                        }
                        t if t.contains("kalman") || t.contains("moving_avg")
                            || t.contains("ewma") || t.contains("lowpass") => {
                            if filtered.is_none() { filtered = Some(v.clone()); }
                        }
                        _ => {
                            if filtered.is_none() { filtered = Some(v.clone()); }
                        }
                    }

                    // p_diag desde el estado guardado del Kalman
                    if node_type.contains("kalman") {
                        if let Some(node) = self.nodes.get(node_id) {
                            let state = node.save_state();
                            if let Some(p) = state.data.get("p")
                                .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok())
                            {
                                p_diag = Some(p);
                            }
                        }
                    }

                    // Decisión numérica desde mahalanobis/hysteresis/sprt
                    if node_type.contains("mahalanobis") {
                        if let Some(node) = self.nodes.get(node_id) {
                            let state = node.save_state();
                            decision = state.data.get("last_d").and_then(|v| v.as_f64());
                        }
                    }
                    if node_type.contains("sprt") && decision.is_none() {
                        if let Some(node) = self.nodes.get(node_id) {
                            let state = node.save_state();
                            decision = state.data.get("llr").and_then(|v| v.as_f64());
                        }
                    }
                }
                Signal::Action(a) => {
                    if self.nodes.get(node_id).map(|n| n.is_actuator()).unwrap_or(false) {
                        actuator = match a {
                            NodeAction::On   => "on".to_string(),
                            NodeAction::Off  => "off".to_string(),
                            NodeAction::Hold => "hold".to_string(),
                        };
                    }
                }
            }
        }

        CycleResult { signals, raw, filtered, p_diag, decision, actuator }
    }

    /// Serializar estado actual de todos los nodos.
    pub fn save_state(&self, pipeline_id: &str, cycle: u64) -> AgentState {
        let node_states: HashMap<String, NodeState> = self.nodes.iter()
            .map(|(id, node)| (id.clone(), node.save_state()))
            .collect();

        AgentState {
            pipeline_id: pipeline_id.to_string(),
            node_states,
            last_signals: HashMap::new(),
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