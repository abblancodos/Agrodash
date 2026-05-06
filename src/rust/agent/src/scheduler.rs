// agent/src/scheduler.rs
#![allow(clippy::panic)] // sqlx::query! genera panics internos que son falsos positivos

use agrodash_shared::{
    AgentState, EdgeConfig, EdgeKind, NodeAction, NodeConfig, NodeState, Signal,
};
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::{HashMap, VecDeque};

use crate::nodes::{self, NodeInstance};

// ── Graph ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct PipelineGraph {
    nodes: HashMap<String, Box<dyn NodeInstance>>,
    topo: Vec<String>,
    /// from_id → Vec<(to_id, to_port, edge_kind)>
    out_edges: HashMap<String, Vec<EdgeTarget>>,
    /// to_id   → Vec<(from_id, to_port, edge_kind)>
    in_edges: HashMap<String, Vec<EdgeSource>>,
}

#[derive(Clone)]
struct EdgeTarget {
    to: String,
    to_port: Option<String>,
    kind: EdgeKind,
}

#[derive(Clone)]
struct EdgeSource {
    from: String,
    to_port: Option<String>,
    kind: EdgeKind,
}

impl PipelineGraph {
    pub async fn build(
        nodes_cfg: &[NodeConfig],
        edges_cfg: &[EdgeConfig],
        shared_connections: &Option<agrodash_shared::SharedConnections>,
        pool: &PgPool,
    ) -> Result<Self> {
        let mut nodes = HashMap::new();
        for node_cfg in nodes_cfg {
            let inst = nodes::build(node_cfg, shared_connections, pool)
                .await
                .with_context(|| format!("Error construyendo nodo '{}'", node_cfg.id))?;
            nodes.insert(node_cfg.id.clone(), inst);
        }

        let mut out_edges: HashMap<String, Vec<EdgeTarget>> = HashMap::new();
        let mut in_edges: HashMap<String, Vec<EdgeSource>> = HashMap::new();

        for e in edges_cfg {
            out_edges
                .entry(e.from.clone())
                .or_default()
                .push(EdgeTarget {
                    to: e.to.clone(),
                    to_port: e.to_port.clone(),
                    kind: e.kind.clone(),
                });
            in_edges.entry(e.to.clone()).or_default().push(EdgeSource {
                from: e.from.clone(),
                to_port: e.to_port.clone(),
                kind: e.kind.clone(),
            });
        }

        // Para el sort topológico usamos solo el grafo de aristas (sin puerto)
        let simple_out: HashMap<String, Vec<String>> = out_edges
            .iter()
            .map(|(k, vs)| (k.clone(), vs.iter().map(|v| v.to.clone()).collect()))
            .collect();
        let simple_in: HashMap<String, Vec<String>> = in_edges
            .iter()
            .map(|(k, vs)| (k.clone(), vs.iter().map(|v| v.from.clone()).collect()))
            .collect();

        let topo = topological_sort(&nodes, &simple_out, &simple_in)
            .context("El grafo tiene ciclos — no es un DAG válido")?;

        Ok(Self {
            nodes,
            topo,
            out_edges,
            in_edges,
        })
    }

    pub fn load_state(&mut self, state: &AgentState) {
        for (node_id, node_state) in &state.node_states {
            if let Some(inst) = self.nodes.get_mut(node_id) {
                inst.load_state(node_state);
            }
        }
    }

    // ── Watchdog control ─────────────────────────────────────────────────────

    /// Resetea el watchdog que protege `actuator_id` (o todos si None).
    pub fn reset_watchdog(&mut self, actuator_id: Option<&str>) {
        for node in self.nodes.values_mut() {
            let ns = node.save_state();
            if ns.node_type != "watchdog" {
                continue;
            }
            let node_act = ns.data.get("actuator_id").and_then(|v| v.as_str());
            let matches = match actuator_id {
                None => true,
                Some(id) => node_act == Some(id),
            };
            if matches {
                // Downcast via trait object — usamos un método en el trait NodeInstance
                // que por defecto no hace nada, y WatchdogNode lo implementa.
                node.watchdog_reset();
            }
        }
    }

    /// Confirma la acción pendiente del watchdog que protege `actuator_id`.
    pub fn confirm_watchdog(&mut self, actuator_id: Option<&str>) {
        for node in self.nodes.values_mut() {
            let ns = node.save_state();
            if ns.node_type != "watchdog" {
                continue;
            }
            let node_act = ns.data.get("actuator_id").and_then(|v| v.as_str());
            let matches = match actuator_id {
                None => true,
                Some(id) => node_act == Some(id),
            };
            if matches {
                node.watchdog_confirm();
            }
        }
    }

    /// Aplica un override a través del watchdog que protege `actuator_id`.
    /// El watchdog lo acepta solo si no está bloqueado.
    pub fn set_watchdog_override(&mut self, actuator_id: &str, action: NodeAction) {
        for node in self.nodes.values_mut() {
            let ns = node.save_state();
            if ns.node_type != "watchdog" {
                continue;
            }
            let node_act = ns.data.get("actuator_id").and_then(|v| v.as_str());
            if node_act == Some(actuator_id) {
                node.watchdog_set_override(action.clone());
                return;
            }
        }
    }

    /// True si hay al menos un Watchdog en el grafo.
    pub fn has_watchdog(&self) -> bool {
        self.nodes
            .values()
            .any(|n| n.save_state().node_type == "watchdog")
    }

    // ── Ciclo normal ──────────────────────────────────────────────────────────

    /// Override directo de actuador (sin watchdog). Usado cuando no hay Watchdog
    /// en el pipeline — comportamiento idéntico al anterior.
    pub async fn run_cycle(
        &mut self,
        pool: &PgPool,
        dt: f64,
        overrides: &HashMap<String, NodeAction>,
    ) -> Result<HashMap<String, Signal>> {
        let mut signals: HashMap<String, Signal> = HashMap::new();

        for node_id in &self.topo.clone() {
            let inputs = self.collect_inputs(node_id, &signals);

            let node = self
                .nodes
                .get_mut(node_id)
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

    /// Dry-run: ejecuta todos los nodos excepto actuadores y watchdogs.
    pub async fn run_cycle_dry(
        &mut self,
        pool: &PgPool,
        dt: f64,
    ) -> Result<HashMap<String, Signal>> {
        let mut signals: HashMap<String, Signal> = HashMap::new();

        for node_id in &self.topo.clone() {
            let inputs = self.collect_inputs(node_id, &signals);
            let node = self
                .nodes
                .get_mut(node_id)
                .context(format!("Nodo '{node_id}' no encontrado"))?;

            let ns = node.save_state();
            if node.is_actuator() || ns.node_type == "watchdog" {
                continue;
            }

            let output = node.execute(inputs, dt, pool).await?;
            if let Some(sig) = output {
                signals.insert(node_id.clone(), sig);
            }
        }

        Ok(signals)
    }

    pub fn save_state(
        &self,
        pipeline_id: &str,
        cycle: u64,
        label: &str,
        override_active: bool,
    ) -> AgentState {
        let node_states: HashMap<String, NodeState> = self
            .nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.save_state()))
            .collect();

        AgentState {
            pipeline_id: pipeline_id.to_string(),
            node_states,
            last_signals: HashMap::new(),
            cycle,
            is_ready: self.is_ready(),
            override_active,
            label: label.to_string(),
        }
    }

    /// Recoger scope_values — combina dos fuentes:
    ///
    /// 1. Para cada Logger: {tag: señal_que_le_llega}
    /// 2. Para cada nodo con metrics(): {tag_upstream:key: valor}
    pub fn collect_scope_values(&self, signals: &HashMap<String, Signal>) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        let mut upstream_to_tag: HashMap<String, String> = HashMap::new();

        for (node_id, node) in &self.nodes {
            let state = node.save_state();
            if state.node_type != "logger" {
                continue;
            }

            let tag = state
                .data
                .get("tag")
                .and_then(|v| v.as_str())
                .unwrap_or(node_id)
                .to_string();

            if let Some(sig) = signals.get(node_id) {
                let val = match sig {
                    Signal::Vector(v) => serde_json::json!(v),
                    Signal::Action(a) => serde_json::json!(format!("{a:?}").to_lowercase()),
                };
                map.insert(tag.clone(), val);
            }

            if let Some(sources) = self.in_edges.get(node_id) {
                for src in sources {
                    upstream_to_tag.insert(src.from.clone(), tag.clone());
                }
            }
        }

        for (node_id, node) in &self.nodes {
            let metrics = node.metrics();
            if metrics.is_empty() {
                continue;
            }

            let prefix = upstream_to_tag
                .get(node_id)
                .cloned()
                .unwrap_or_else(|| node_id.clone());

            for (key, val) in metrics {
                map.insert(format!("{prefix}:{key}"), serde_json::json!(val));
            }
        }

        serde_json::Value::Object(map)
    }

    pub fn actuator_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.is_actuator())
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn is_ready(&self) -> bool {
        self.nodes.values().all(|n| n.is_ready())
    }

    pub fn node_state_by_type(&self, node_type: &str) -> Option<NodeState> {
        self.nodes
            .values()
            .map(|n| n.save_state())
            .find(|ns| ns.node_type == node_type)
    }

    pub fn extract_raw_filtered(
        &self,
        signals: &HashMap<String, Signal>,
    ) -> (Option<Vec<f64>>, Option<Vec<f64>>) {
        const SOURCE_TYPES: &[&str] = &["postgres_sensor"];
        const FILTER_TYPES: &[&str] = &["kalman", "ewma", "moving_avg", "lowpass", "passthrough"];

        let source_ids: Vec<&str> = self
            .topo
            .iter()
            .filter(|id| self.in_edges.get(*id).map(|v| v.is_empty()).unwrap_or(true))
            .map(|s| s.as_str())
            .collect();

        let raw = source_ids
            .iter()
            .find_map(|id| {
                signals.get(*id).and_then(|s| match s {
                    Signal::Vector(v) => Some(v.clone()),
                    _ => None,
                })
            })
            .or_else(|| {
                self.topo.iter().find_map(|id| {
                    let ns = self.nodes.get(id)?.save_state();
                    if SOURCE_TYPES.contains(&ns.node_type.as_str()) {
                        signals.get(id).and_then(|s| match s {
                            Signal::Vector(v) => Some(v.clone()),
                            _ => None,
                        })
                    } else {
                        None
                    }
                })
            });

        let filtered = self.topo.iter().find_map(|id| {
            let ns = self.nodes.get(id)?.save_state();
            if !FILTER_TYPES.contains(&ns.node_type.as_str()) {
                return None;
            }
            signals.get(id).and_then(|s| match s {
                Signal::Vector(v) => Some(v.clone()),
                _ => None,
            })
        });

        (raw, filtered)
    }

    pub fn kalman_p_diag(&self) -> Option<Vec<f64>> {
        self.node_state_by_type("kalman")
            .and_then(|ns| ns.data.get("p").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
    }

    // ── collect_inputs con soporte de puertos ─────────────────────────────────
    //
    // El Watchdog distingue sus inputs por `to_port`:
    //   "decision"  → Signal::Action del decisor
    //   "feedback"  → Signal::Action del MqttSubscriber
    //   "signal"    → Signal::Vector del filtro
    //
    // Para nodos sin to_port, el orden de llegada es el orden de los edges en config.
    // El Watchdog es tolerante al orden siempre que los puertos estén definidos.

    fn collect_inputs(&self, node_id: &str, signals: &HashMap<String, Signal>) -> Vec<Signal> {
        let sources = match self.in_edges.get(node_id) {
            Some(s) => s,
            None => return vec![],
        };

        // Si hay puertos definidos, ordenamos: decision → feedback → signal → sin-puerto
        // para que el Watchdog siempre reciba los inputs en el orden correcto.
        let mut with_port: Vec<(&EdgeSource, Signal)> = vec![];
        let mut without_port: Vec<Signal> = vec![];

        for src in sources {
            let sig = match signals.get(&src.from) {
                Some(s) => s.clone(),
                None => continue,
            };
            if src.to_port.is_some() {
                with_port.push((src, sig));
            } else {
                without_port.push(sig);
            }
        }

        if with_port.is_empty() {
            return without_port;
        }

        // Orden canónico de puertos para el Watchdog
        const PORT_ORDER: &[&str] = &["decision", "feedback", "signal"];
        with_port.sort_by_key(|(src, _)| {
            let port = src.to_port.as_deref().unwrap_or("");
            PORT_ORDER
                .iter()
                .position(|&p| p == port)
                .unwrap_or(PORT_ORDER.len())
        });

        let mut result: Vec<Signal> = with_port.into_iter().map(|(_, s)| s).collect();
        result.extend(without_port);
        result
    }
}

// ── Topological sort (Kahn's algorithm) ──────────────────────────────────────

fn topological_sort(
    nodes: &HashMap<String, Box<dyn NodeInstance>>,
    out_edges: &HashMap<String, Vec<String>>,
    in_edges: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    let mut in_degree: HashMap<String, usize> = nodes
        .keys()
        .map(|id| (id.clone(), in_edges.get(id).map(|v| v.len()).unwrap_or(0)))
        .collect();

    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();

    while let Some(id) = queue.pop_front() {
        order.push(id.clone());
        if let Some(successors) = out_edges.get(&id) {
            for succ in successors {
                let deg = in_degree
                    .get_mut(succ)
                    .expect("nodo sucesor no encontrado en in_degree — grafo inconsistente");
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

// ── Métodos adicionales accedidos desde main.rs ───────────────────────────────

impl PipelineGraph {
    /// Iterador sobre todos los nodos — usado por log_watchdog_alerts.
    pub fn nodes(&self) -> impl Iterator<Item = (&String, &Box<dyn NodeInstance>)> {
        self.nodes.iter()
    }

    /// True si algún watchdog tiene un override manual activo.
    /// Usado para calcular override_active en save_state.
    pub fn has_watchdog_override(&self) -> bool {
        self.nodes.values().any(|n| {
            let ns = n.save_state();
            if ns.node_type != "watchdog" {
                return false;
            }
            // override_action en el watchdog se refleja en status != ok
            // pero lo más directo es mirar si pending o override está seteado.
            // Por ahora usamos el heurístico: si hay override_action en data
            // (el watchdog lo serializa cuando set_override fue llamado).
            ns.data.get("override_action").is_some()
        })
    }
}
