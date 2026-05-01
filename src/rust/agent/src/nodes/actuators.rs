// agent/src/nodes/actuators.rs

use async_trait::async_trait;
use agrodash_shared::{
    NodeState, Signal, NodeAction,
    MqttActuatorConfig, HttpActuatorConfig, MqttConnection, HttpConnection,
};
use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use super::NodeInstance;

// ── MQTT ──────────────────────────────────────────────────────────────────────

pub struct MqttActuatorNode {
    id:          String,
    cfg:         MqttActuatorConfig,
    conn_cfg:    MqttConnection,
    client:      rumqttc::AsyncClient,
    qos:         rumqttc::QoS,
    last_action: Option<NodeAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl MqttActuatorNode {
    pub async fn new(id: String, cfg: MqttActuatorConfig, conn: MqttConnection) -> Result<Self> {
        let (client, qos) = Self::connect(&conn).await?;
        Ok(Self {
            id, cfg, conn_cfg: conn, client, qos,
            last_action: None, last_at: None, total_on: 0.0, on_since: None,
        })
    }

    async fn connect(conn: &MqttConnection) -> Result<(rumqttc::AsyncClient, rumqttc::QoS)> {
        use rumqttc::{MqttOptions, AsyncClient, QoS};

        let url = conn.broker_url.trim_start_matches("mqtt://");
        let (host, port) = url.split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let mut opts = MqttOptions::new(conn.client_id.clone(), host, port);
        opts.set_keep_alive(std::time::Duration::from_secs(
            conn.keepalive_secs.unwrap_or(30)
        ));
        if let (Some(u), Some(p)) = (conn.username.clone(), conn.password.clone()) {
            opts.set_credentials(u, p);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 10);

        // Eventloop con reconexión real: si el poll falla repetidamente,
        // el eventloop de rumqttc ya maneja reconexión interna. Solo logueamos.
        tokio::spawn(async move {
            let mut consecutive_errors = 0u32;
            loop {
                match eventloop.poll().await {
                    Ok(_) => { consecutive_errors = 0; }
                    Err(e) => {
                        consecutive_errors += 1;
                        tracing::warn!(
                            "MQTT eventloop error (intento {}): {e}",
                            consecutive_errors
                        );
                        // Backoff exponencial hasta 60s
                        let delay = std::cmp::min(5 * consecutive_errors, 60);
                        tokio::time::sleep(std::time::Duration::from_secs(delay as u64)).await;
                    }
                }
            }
        });

        let qos = match conn.qos.unwrap_or(1) {
            0 => QoS::AtMostOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };

        Ok((client, qos))
    }

    async fn publish(&self, payload: &str) -> Result<()> {
        self.client.publish(
            &self.cfg.topic,
            self.qos,
            self.cfg.retain.unwrap_or(false),
            payload.as_bytes().to_vec(),
        ).await?;
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
                tracing::info!("MQTT ON → topic:{}", self.cfg.topic);
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() {
                    self.total_on += t.elapsed().as_secs_f64();
                }
                self.publish(&self.cfg.payload_off.clone()).await?;
                self.last_action = Some(NodeAction::Off);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!("MQTT OFF → topic:{}", self.cfg.topic);
            }
            NodeAction::Hold | _ => {}
        }
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool { true }

    fn save_state(&self) -> NodeState {
        NodeState {
            node_id:   self.id.clone(),
            node_type: "mqtt_actuator".into(),
            data: serde_json::json!({
                "last_action": self.last_action,
                "last_at":     self.last_at,
                "total_on":    self.total_on,
            }),
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
        self.last_action = state.data.get("last_action")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_at = state.data.get("last_at")
            .and_then(|v| v.as_str().map(String::from));
        self.total_on = state.data.get("total_on")
            .and_then(|v| v.as_f64()).unwrap_or(0.0);
    }
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

pub struct HttpActuatorNode {
    id:          String,
    cfg:         HttpActuatorConfig,
    conn:        HttpConnection,       // resuelto correctamente en el factory
    client:      reqwest::Client,
    last_action: Option<NodeAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl HttpActuatorNode {
    pub fn new(id: String, cfg: HttpActuatorConfig, conn: HttpConnection) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(conn.timeout_secs.unwrap_or(8)));

        // Headers extra definidos en la conexión
        if let Some(headers) = &conn.extra_headers {
            let mut hmap = reqwest::header::HeaderMap::new();
            for (k, v) in headers {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(v),
                ) {
                    hmap.insert(name, val);
                }
            }
            builder = builder.default_headers(hmap);
        }

        let client = builder.build().unwrap_or_default();

        Self {
            id, cfg, conn, client,
            last_action: None, last_at: None, total_on: 0.0, on_since: None,
        }
    }

    async fn call(&self, path: &str, body: Option<&serde_json::Value>) -> Result<()> {
        let url = format!(
            "{}{}",
            self.conn.base_url.trim_end_matches('/'),
            path
        );
        let method = self.cfg.method.as_deref().unwrap_or("POST");
        let mut req = match method.to_uppercase().as_str() {
            "GET"   => self.client.get(&url),
            "PUT"   => self.client.put(&url),
            "PATCH" => self.client.patch(&url),
            _       => self.client.post(&url),
        };

        if let Some(token) = &self.conn.bearer_token {
            req = req.bearer_auth(token);
        }
        if let Some(b) = body {
            req = req.json(b);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP actuador respondió {}", resp.status());
        }
        Ok(())
    }
}

#[async_trait]
impl NodeInstance for HttpActuatorNode {
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
                self.call(&self.cfg.path_on.clone(), self.cfg.body_on.as_ref()).await?;
                self.on_since    = Some(std::time::Instant::now());
                self.last_action = Some(NodeAction::On);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!("HTTP ON → {}{}", self.conn.base_url, self.cfg.path_on);
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() {
                    self.total_on += t.elapsed().as_secs_f64();
                }
                self.call(&self.cfg.path_off.clone(), self.cfg.body_off.as_ref()).await?;
                self.last_action = Some(NodeAction::Off);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!("HTTP OFF → {}{}", self.conn.base_url, self.cfg.path_off);
            }
            NodeAction::Hold => {}
            _ => {}
        }
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool { true }

    fn save_state(&self) -> NodeState {
        NodeState {
            node_id:   self.id.clone(),
            node_type: "http_actuator".into(),
            data: serde_json::json!({
                "last_action": self.last_action,
                "last_at":     self.last_at,
                "total_on":    self.total_on,
            }),
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
        self.last_action = state.data.get("last_action")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_at = state.data.get("last_at")
            .and_then(|v| v.as_str().map(String::from));
        self.total_on = state.data.get("total_on")
            .and_then(|v| v.as_f64()).unwrap_or(0.0);
    }
}