// agent/src/nodes/mod.rs
//
// Trait NodeInstance y factory para construir nodos desde NodeConfig.

use anyhow::Result;
use async_trait::async_trait;
use agrodash_shared::{
    NodeConfig, NodeKind, NodeState, Signal, NodeAction,
    SharedConnections, ConnectionRef, MqttConnection,
};
use sqlx::PgPool;

pub mod source;
pub mod filters;
pub mod decisions;
pub mod actuators;
pub mod utils;

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait NodeInstance: Send + Sync {
    /// Ejecutar el nodo con las señales de entrada.
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        dt:     f64,
        pool:   &PgPool,
    ) -> Result<Option<Signal>>;

    /// Ejecutar override en actuadores.
    async fn execute_override(
        &mut self,
        action: NodeAction,
    ) -> Result<Option<Signal>> {
        // Por defecto no hace nada — solo los actuadores lo implementan
        let _ = action;
        Ok(None)
    }

    /// True si el nodo es un actuador.
    fn is_actuator(&self) -> bool { false }

    /// True si el nodo ya completó su warmup.
    fn is_ready(&self) -> bool { true }

    /// Serializar estado interno.
    fn save_state(&self) -> NodeState;

    /// Restaurar estado desde DB.
    fn load_state(&mut self, state: &NodeState);
}

// ── Factory ───────────────────────────────────────────────────────────────────

pub async fn build(
    cfg:                 &NodeConfig,
    shared_connections:  &Option<SharedConnections>,
    pool:                &PgPool,
) -> Result<Box<dyn NodeInstance>> {
    match &cfg.kind {
        // Fuentes
        NodeKind::PostgresSensor(c) => {
            Ok(Box::new(source::PostgresSensorNode::new(cfg.id.clone(), c.clone())))
        }

        // Filtros
        NodeKind::Kalman(c) => {
            Ok(Box::new(filters::KalmanNode::new(cfg.id.clone(), c.clone())))
        }
        NodeKind::MovingAvg(c) => {
            Ok(Box::new(filters::MovingAvgNode::new(cfg.id.clone(), c.clone())))
        }
        NodeKind::Ewma(c) => {
            Ok(Box::new(filters::EwmaNode::new(cfg.id.clone(), c.clone())))
        }
        NodeKind::Lowpass(c) => {
            Ok(Box::new(filters::LowpassNode::new(cfg.id.clone(), c.clone())))
        }
        NodeKind::Passthrough => {
            Ok(Box::new(filters::PassthroughNode::new(cfg.id.clone())))
        }

        // Combinadores
        NodeKind::Concat => {
            Ok(Box::new(utils::ConcatNode::new(cfg.id.clone())))
        }
        NodeKind::WeightedMean { weights } => {
            Ok(Box::new(utils::WeightedMeanNode::new(cfg.id.clone(), weights.clone())))
        }

        // Utilidades
        NodeKind::Logger { tag } => {
            Ok(Box::new(utils::LoggerNode::new(cfg.id.clone(), tag.clone())))
        }
        NodeKind::Select { indices } => {
            Ok(Box::new(utils::SelectNode::new(cfg.id.clone(), indices.clone())))
        }
        NodeKind::LinearScale { a, b } => {
            Ok(Box::new(utils::LinearScaleNode::new(cfg.id.clone(), a.clone(), b.clone())))
        }

        // Decisores
        NodeKind::Mahalanobis(c) => {
            Ok(Box::new(decisions::MahalanobisNode::new(cfg.id.clone(), c.clone())))
        }
        NodeKind::Hysteresis(c) => {
            Ok(Box::new(decisions::HysteresisNode::new(cfg.id.clone(), c.clone())))
        }
        NodeKind::Sprt(c) => {
            Ok(Box::new(decisions::SprtNode::new(cfg.id.clone(), c.clone())))
        }

        // Actuadores
        NodeKind::MqttActuator(c) => {
            let conn = resolve_mqtt_connection(&c.connection, shared_connections)?;
            Ok(Box::new(actuators::MqttActuatorNode::new(
                cfg.id.clone(), c.clone(), conn,
            ).await?))
        }
        NodeKind::HttpActuator(c) => {
            Ok(Box::new(actuators::HttpActuatorNode::new(cfg.id.clone(), c.clone())))
        }
    }
}

// ── Resolver conexiones ───────────────────────────────────────────────────────

fn resolve_mqtt_connection(
    conn_ref:           &ConnectionRef,
    shared_connections: &Option<SharedConnections>,
) -> Result<MqttConnection> {
    match conn_ref {
        ConnectionRef::Named(name) if name == "shared" => {
            shared_connections
                .as_ref()
                .and_then(|sc| sc.mqtt.clone())
                .ok_or_else(|| anyhow::anyhow!(
                    "Se referencia 'shared' pero no hay shared_connections.mqtt definido"
                ))
        }
        ConnectionRef::Inline(inline) => {
            inline.mqtt.clone().ok_or_else(|| anyhow::anyhow!(
                "Conexión inline sin campo mqtt"
            ))
        }
        ConnectionRef::Named(other) => {
            anyhow::bail!("Referencia de conexión desconocida: '{}'", other)
        }
    }
}

// ── Helper para extraer vector de señal ──────────────────────────────────────

pub fn expect_vector(signal: &Signal, node_id: &str) -> Result<Vec<f64>> {
    match signal {
        Signal::Vector(v) => Ok(v.clone()),
        Signal::Action(_) => anyhow::bail!(
            "Nodo '{}': se esperaba Signal::Vector pero se recibió Signal::Action", node_id
        ),
    }
}

pub fn expect_action(signal: &Signal, node_id: &str) -> Result<NodeAction> {
    match signal {
        Signal::Action(a) => Ok(a.clone()),
        Signal::Vector(_) => anyhow::bail!(
            "Nodo '{}': se esperaba Signal::Action pero se recibió Signal::Vector", node_id
        ),
    }
}
