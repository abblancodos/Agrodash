// agent/src/nodes/actuators.rs

use async_trait::async_trait;
use agrodash_shared::{
    NodeState, Signal, NodeAction,
    MqttActuatorConfig, HttpActuatorConfig, MqttConnection,
};
use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use super::NodeInstance;

// ── MQTT ──────────────────────────────────────────────────────────────────────

pub struct MqttActuatorNode {
    id:          String,
    cfg:         MqttActuatorConfig,
    client:      rumqttc::AsyncClient,
    qos:         rumqttc::QoS,
    last_action: Option<NodeAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl MqttActuatorNode {
    pub async fn new(id: String, cfg: MqttActuatorConfig, conn: MqttConnection) -> Result<Self> {
        use rumqttc::{MqttOptions, AsyncClient, QoS};
        let url = conn.broker_url.trim_start_matches("mqtt://");
        let (host, port) = url.split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let mut opts = MqttOptions::new(conn.client_id, host, port);
        opts.set_keep_alive(std::time::Duration::from_secs(conn.keepalive_secs.unwrap_or(30)));
        if let (Some(u), Some(p)) = (conn.username, conn.password) { opts.set_credentials(u, p); }

        let (client, mut eventloop) = AsyncClient::new(opts, 10);
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_) => {}
                    Err(e) => { tracing::warn!("MQTT eventloop: {e}"); tokio::time::sleep(std::time::Duration::from_secs(5)).await; }
                }
            }
        });

        let qos = match conn.qos.unwrap_or(1) { 0 => QoS::AtMostOnce, 2 => QoS::ExactlyOnce, _ => QoS::AtLeastOnce };
        Ok(Self { id, cfg, client, qos, last_action: None, last_at: None, total_on: 0.0, on_since: None })
    }

    async fn publish(&self, payload: &str) -> Result<()> {
        self.client.publish(&self.cfg.topic, self.qos, self.cfg.retain.unwrap_or(false), payload.as_bytes().to_vec()).await?;
        Ok(())
    }
}

#[async_trait]
impl NodeInstance for MqttActuatorNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let action = match inputs.first() {
            Some(Signal::Action(a)) => a.clone(),
            _ => return Ok(None),
        };
        self.execute_override(action).await
    }

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        match &action {
            NodeAction::On if self.last_action != Some(NodeAction::On) => {
                self.publish(&self.cfg.payload_on.clone()).await?;
                self.on_since    = Some(std::time::Instant::now());
                self.last_action = Some(NodeAction::On);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!("MQTT ON: {}", self.cfg.topic);
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() { self.total_on += t.elapsed().as_secs_f64(); }
                self.publish(&self.cfg.payload_off.clone()).await?;
                self.last_action = Some(NodeAction::Off);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!("MQTT OFF: {}", self.cfg.topic);
            }
            NodeAction::Hold | _ => {}
        }
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool { true }

    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "mqtt_actuator".into(),
            data: serde_json::json!({ "last_action": self.last_action, "last_at": self.last_at, "total_on": self.total_on }),
            is_ready: true }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.last_action = state.data.get("last_action").and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_at     = state.data.get("last_at").and_then(|v| v.as_str().map(String::from));
        self.total_on    = state.data.get("total_on").and_then(|v| v.as_f64()).unwrap_or(0.0);
    }
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

pub struct HttpActuatorNode {
    id:          String,
    cfg:         HttpActuatorConfig,
    client:      reqwest::Client,
    base_url:    String,
    last_action: Option<NodeAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl HttpActuatorNode {
    pub fn new(id: String, cfg: HttpActuatorConfig) -> Self {
        // base_url comes from the connection ref — resolved at runtime
        Self { id, cfg, client: reqwest::Client::new(), base_url: String::new(),
               last_action: None, last_at: None, total_on: 0.0, on_since: None }
    }
}

#[async_trait]
impl NodeInstance for HttpActuatorNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let action = match inputs.first() { Some(Signal::Action(a)) => a.clone(), _ => return Ok(None) };
        self.execute_override(action).await
    }

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        let (path, body) = match &action {
            NodeAction::On  => (self.cfg.path_on.clone(),  self.cfg.body_on.clone()),
            NodeAction::Off => (self.cfg.path_off.clone(), self.cfg.body_off.clone()),
            NodeAction::Hold => return Ok(Some(Signal::Action(action))),
        };
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut req = self.client.post(&url);
        if let Some(b) = body { req = req.json(&b); }
        req.send().await?;
        self.last_action = Some(action.clone());
        self.last_at     = Some(Utc::now().to_rfc3339());
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool { true }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "http_actuator".into(),
            data: serde_json::json!({ "last_action": self.last_action, "last_at": self.last_at }),
            is_ready: true }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.last_action = state.data.get("last_action").and_then(|v| serde_json::from_value(v.clone()).ok());
    }
}
