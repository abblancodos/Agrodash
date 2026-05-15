// agent/src/nodes/mod.rs

use agrodash_shared::{
    ConnectionRef, HttpConnection, MqttConnection, NodeAction, NodeConfig, NodeKind, NodeState,
    SharedConnections, Signal,
};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;

pub mod actuators;
pub mod decisions;
pub mod filters;
pub mod source;
pub mod subscriber;
pub mod utils;
pub mod watchdog;

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait NodeInstance: Send + Sync {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        dt: f64,
        pool: &PgPool,
    ) -> Result<Option<Signal>>;

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        let _action = action; // default: nodo no es actuador, ignora la acción
        Ok(None)
    }

    fn is_actuator(&self) -> bool {
        false
    }
    fn is_ready(&self) -> bool {
        true
    }

    /// Métricas internas para series temporales (scope_values).
    fn metrics(&self) -> Vec<(String, f64)> {
        vec![]
    }

    // Watchdog control — implementación vacía por defecto.
    // WatchdogNode overridea estos métodos.
    fn watchdog_reset(&mut self) {}
    fn watchdog_confirm(&mut self) {}
    fn watchdog_set_override(&mut self, _action: NodeAction) {}
    #[allow(dead_code)]
    fn has_watchdog_override(&self) -> bool {
        false
    }

    fn save_state(&self) -> NodeState;
    fn load_state(&mut self, state: &NodeState);
}

// ── Factory ───────────────────────────────────────────────────────────────────

pub async fn build(
    cfg: &NodeConfig,
    shared_connections: &Option<SharedConnections>,
    _pool: &PgPool,
) -> Result<Box<dyn NodeInstance>> {
    match &cfg.kind {
        NodeKind::PostgresSensor(c) => Ok(Box::new(source::PostgresSensorNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),

        NodeKind::Kalman(c) => Ok(Box::new(filters::KalmanNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),
        NodeKind::MovingAvg(c) => Ok(Box::new(filters::MovingAvgNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),
        NodeKind::Ewma(c) => Ok(Box::new(filters::EwmaNode::new(cfg.id.clone(), c.clone()))),
        NodeKind::Lowpass(c) => Ok(Box::new(filters::LowpassNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),
        NodeKind::Passthrough => Ok(Box::new(filters::PassthroughNode::new(cfg.id.clone()))),

        NodeKind::Concat => Ok(Box::new(utils::ConcatNode::new(cfg.id.clone()))),
        NodeKind::WeightedMean { weights } => Ok(Box::new(utils::WeightedMeanNode::new(
            cfg.id.clone(),
            weights.clone(),
        ))),

        NodeKind::Logger { tag } => Ok(Box::new(utils::LoggerNode::new(
            cfg.id.clone(),
            tag.clone(),
        ))),
        NodeKind::Select { indices } => Ok(Box::new(utils::SelectNode::new(
            cfg.id.clone(),
            indices.clone(),
        ))),
        NodeKind::LinearScale { a, b } => Ok(Box::new(utils::LinearScaleNode::new(
            cfg.id.clone(),
            a.clone(),
            b.clone(),
        ))),

        NodeKind::Mahalanobis(c) => Ok(Box::new(decisions::MahalanobisNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),
        NodeKind::Hysteresis(c) => Ok(Box::new(decisions::HysteresisNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),
        NodeKind::Sprt(c) => Ok(Box::new(decisions::SprtNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),

        NodeKind::MqttActuator(c) => {
            let conn = resolve_mqtt_connection(&c.connection, shared_connections)?;
            Ok(Box::new(
                actuators::MqttActuatorNode::new(cfg.id.clone(), c.clone(), conn).await?,
            ))
        }

        NodeKind::HttpActuator(c) => {
            let conn = resolve_http_connection(&c.connection, shared_connections)?;
            Ok(Box::new(actuators::HttpActuatorNode::new(
                cfg.id.clone(),
                c.clone(),
                conn,
            )))
        }

        NodeKind::MqttSubscriber(c) => {
            let conn = resolve_mqtt_connection(&c.connection, shared_connections)?;
            Ok(Box::new(
                subscriber::MqttSubscriberNode::new(cfg.id.clone(), c.clone(), conn).await?,
            ))
        }

        NodeKind::Watchdog(c) => Ok(Box::new(watchdog::WatchdogNode::new(
            cfg.id.clone(),
            c.clone(),
        ))),
    }
}

// ── Resolvers de conexiones ───────────────────────────────────────────────────

pub fn resolve_mqtt_connection(
    conn_ref: &ConnectionRef,
    shared_connections: &Option<SharedConnections>,
) -> Result<MqttConnection> {
    match conn_ref {
        ConnectionRef::Named(name) if name == "shared" => shared_connections
            .as_ref()
            .and_then(|sc| sc.mqtt.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("Referencia 'shared' sin shared_connections.mqtt definido")
            }),
        ConnectionRef::Inline(inline) => inline
            .mqtt
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Conexión inline sin campo mqtt")),
        ConnectionRef::Named(other)
            if other.starts_with("mqtt://") || other.starts_with("mqtts://") =>
        {
            // URL inline como string — construir MqttConnection directamente
            let url = other
                .trim_start_matches("mqtt://")
                .trim_start_matches("mqtts://");
            let (host, port) = url
                .split_once(':')
                .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
                .unwrap_or((url.to_string(), 1883));
            Ok(MqttConnection {
                broker_url: other.clone(),
                client_id: format!("agrodash-act-{}", uuid::Uuid::new_v4()),
                username: None,
                password: None,
                keepalive_secs: Some(30),
                qos: Some(1),
            })
        }
        ConnectionRef::Named(other) => {
            anyhow::bail!("Referencia de conexión MQTT desconocida: '{}'", other)
        }
    }
}

pub fn resolve_http_connection(
    conn_ref: &ConnectionRef,
    shared_connections: &Option<SharedConnections>,
) -> Result<HttpConnection> {
    match conn_ref {
        ConnectionRef::Named(name) if name == "shared" => shared_connections
            .as_ref()
            .and_then(|sc| sc.http.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("Referencia 'shared' sin shared_connections.http definido")
            }),
        ConnectionRef::Inline(inline) => inline
            .http
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Conexión inline sin campo http")),
        ConnectionRef::Named(other) => {
            anyhow::bail!("Referencia de conexión HTTP desconocida: '{}'", other)
        }
    }
}

// ── Helpers de señal ──────────────────────────────────────────────────────────

pub fn expect_vector(signal: &Signal, node_id: &str) -> Result<Vec<f64>> {
    match signal {
        Signal::Vector(v) => Ok(v.clone()),
        Signal::Action(_) => anyhow::bail!(
            "Nodo '{node_id}': se esperaba Signal::Vector pero se recibió Signal::Action"
        ),
    }
}

#[allow(dead_code)]
pub fn expect_action(signal: &Signal, node_id: &str) -> Result<NodeAction> {
    match signal {
        Signal::Action(a) => Ok(a.clone()),
        Signal::Vector(_) => anyhow::bail!(
            "Nodo '{node_id}': se esperaba Signal::Action pero se recibió Signal::Vector"
        ),
    }
}
