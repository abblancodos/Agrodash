// agent/src/nodes/actuators.rs

use super::NodeInstance;
use agrodash_shared::{
    HttpActuatorConfig, HttpConnection, MqttActuatorConfig, MqttConnection, NodeAction, NodeState,
    Signal,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::time::Duration;

// ── MQTT ──────────────────────────────────────────────────────────────────────

pub struct MqttActuatorNode {
    id: String,
    cfg: MqttActuatorConfig,
    conn_cfg: MqttConnection,
    client: Option<rumqttc::AsyncClient>,
    qos: rumqttc::QoS,
    last_action: Option<NodeAction>,
    last_at: Option<String>,
    total_on: f64,
    on_since: Option<std::time::Instant>,
}

impl MqttActuatorNode {
    pub async fn new(id: String, cfg: MqttActuatorConfig, conn: MqttConnection) -> Result<Self> {
        use rumqttc::QoS;
        let qos = match conn.qos.unwrap_or(1) {
            0 => QoS::AtMostOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };
        let mut node = Self {
            id,
            cfg,
            conn_cfg: conn,
            client: None,
            qos,
            last_action: None,
            last_at: None,
            total_on: 0.0,
            on_since: None,
        };
        node.connect().await;
        Ok(node)
    }

    /// Crea/recrea el cliente MQTT. Se llama en init y tras errores graves.
    async fn connect(&mut self) {
        use rumqttc::{AsyncClient, MqttOptions};

        let url = self.conn_cfg.broker_url.trim_start_matches("mqtt://");
        let (host, port) = url
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let mut opts = MqttOptions::new(self.conn_cfg.client_id.clone(), host, port);
        opts.set_keep_alive(Duration::from_secs(
            self.conn_cfg.keepalive_secs.unwrap_or(30),
        ));
        if let (Some(u), Some(p)) = (
            self.conn_cfg.username.clone(),
            self.conn_cfg.password.clone(),
        ) {
            opts.set_credentials(u, p);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 10);
        let node_id = self.id.clone();
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(2);
            loop {
                match eventloop.poll().await {
                    Ok(_) => {
                        backoff = Duration::from_secs(2);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[MQTT {node_id}] eventloop: {e} — reintento en {}s",
                            backoff.as_secs()
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });

        self.client = Some(client);
        tracing::info!(
            "[MQTT {}] conectado a {}",
            self.id,
            self.conn_cfg.broker_url
        );
    }

    async fn publish(&mut self, payload: &str) -> Result<()> {
        if self.client.is_none() {
            self.connect().await;
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sin cliente MQTT"))?;
        client
            .publish(
                &self.cfg.topic,
                self.qos,
                self.cfg.retain.unwrap_or(false),
                payload.as_bytes().to_vec(),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl NodeInstance for MqttActuatorNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
    ) -> Result<Option<Signal>> {
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
                self.on_since = Some(std::time::Instant::now());
                self.last_action = Some(NodeAction::On);
                self.last_at = Some(Utc::now().to_rfc3339());
                tracing::info!(
                    "[MQTT {}] ▶ ON → topic='{}' payload='{}'",
                    self.id,
                    self.cfg.topic,
                    self.cfg.payload_on
                );
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() {
                    self.total_on += t.elapsed().as_secs_f64();
                }
                self.publish(&self.cfg.payload_off.clone()).await?;
                self.last_action = Some(NodeAction::Off);
                self.last_at = Some(Utc::now().to_rfc3339());
                tracing::info!(
                    "[MQTT {}] ■ OFF → topic='{}' payload='{}'",
                    self.id,
                    self.cfg.topic,
                    self.cfg.payload_off
                );
            }
            NodeAction::Hold | _ => {}
        }
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool {
        true
    }

    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
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
        // Restauramos last_action pero lo reseteamos a None para forzar
        // re-publicación en el primer ciclo tras un reinicio.
        // Así el relay físico siempre queda sincronizado con el estado del agente.
        let _prev = state
            .data
            .get("last_action")
            .and_then(|v| serde_json::from_value::<NodeAction>(v.clone()).ok());
        self.last_action = None; // fuerza sync en primer ciclo
        self.last_at = state
            .data
            .get("last_at")
            .and_then(|v| v.as_str().map(String::from));
        self.total_on = state
            .data
            .get("total_on")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
    }
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

pub struct HttpActuatorNode {
    id: String,
    cfg: HttpActuatorConfig,
    conn: HttpConnection,
    client: reqwest::Client,
    last_action: Option<NodeAction>,
    last_at: Option<String>,
    total_on: f64,
    on_since: Option<std::time::Instant>,
}

impl HttpActuatorNode {
    /// FIX: `conn` viene resuelto en `nodes/mod.rs` via `resolve_http_connection()`.
    pub fn new(id: String, cfg: HttpActuatorConfig, conn: HttpConnection) -> Self {
        let timeout = Duration::from_secs(conn.timeout_secs.unwrap_or(10));
        let mut builder = reqwest::Client::builder().timeout(timeout);

        if let Some(token) = &conn.bearer_token {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
            builder = builder.default_headers(headers);
        }

        let client = builder.build().unwrap_or_default();
        Self {
            id,
            cfg,
            conn,
            client,
            last_action: None,
            last_at: None,
            total_on: 0.0,
            on_since: None,
        }
    }

    async fn call(&self, path: &str, body: Option<&serde_json::Value>, method: &str) -> Result<()> {
        let url = format!("{}{}", self.conn.base_url.trim_end_matches('/'), path);

        let mut req = match method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "PUT" => self.client.put(&url),
            "PATCH" => self.client.patch(&url),
            _ => self.client.post(&url),
        };

        if let Some(extra) = &self.conn.extra_headers {
            for (k, v) in extra {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(v),
                ) {
                    req = req.header(name, val);
                }
            }
        }

        if let Some(b) = body {
            req = req.json(b);
        }

        let res = req.send().await?;
        if res.status().is_server_error() {
            anyhow::bail!("HTTP actuator {}: status {}", self.id, res.status());
        }
        Ok(())
    }
}

#[async_trait]
impl NodeInstance for HttpActuatorNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
    ) -> Result<Option<Signal>> {
        let action = match inputs.first() {
            Some(Signal::Action(a)) => a.clone(),
            _ => return Ok(None),
        };
        self.execute_override(action).await
    }

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        let method = self.cfg.method.clone().unwrap_or_else(|| "POST".into());
        match &action {
            NodeAction::On if self.last_action != Some(NodeAction::On) => {
                self.call(
                    &self.cfg.path_on.clone(),
                    self.cfg.body_on.as_ref(),
                    &method,
                )
                .await?;
                self.on_since = Some(std::time::Instant::now());
                self.last_action = Some(NodeAction::On);
                self.last_at = Some(Utc::now().to_rfc3339());
                tracing::info!(
                    "[HTTP {}] ON → {}{}",
                    self.id,
                    self.conn.base_url,
                    self.cfg.path_on
                );
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() {
                    self.total_on += t.elapsed().as_secs_f64();
                }
                self.call(
                    &self.cfg.path_off.clone(),
                    self.cfg.body_off.as_ref(),
                    &method,
                )
                .await?;
                self.last_action = Some(NodeAction::Off);
                self.last_at = Some(Utc::now().to_rfc3339());
                tracing::info!(
                    "[HTTP {}] OFF → {}{}",
                    self.id,
                    self.conn.base_url,
                    self.cfg.path_off
                );
            }
            NodeAction::Hold | _ => {}
        }
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool {
        true
    }

    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
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
        // Restauramos last_action pero lo reseteamos a None para forzar
        // re-publicación en el primer ciclo tras un reinicio.
        // Así el relay físico siempre queda sincronizado con el estado del agente.
        let _prev = state
            .data
            .get("last_action")
            .and_then(|v| serde_json::from_value::<NodeAction>(v.clone()).ok());
        self.last_action = None; // fuerza sync en primer ciclo
        self.last_at = state
            .data
            .get("last_at")
            .and_then(|v| v.as_str().map(String::from));
        self.total_on = state
            .data
            .get("total_on")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
    }
}

// ── Ping helpers para self-test ────────────────────────────────────────────────

/// Verifica conectividad MQTT sin publicar nada en topics reales.
/// Retorna la latencia del ConnAck.
pub async fn mqtt_ping(conn: &MqttConnection) -> Result<Duration> {
    use rumqttc::{AsyncClient, Event, MqttOptions, Packet};
    use tokio::time::timeout;

    let url = conn.broker_url.trim_start_matches("mqtt://");
    let (host, port) = url
        .split_once(':')
        .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
        .unwrap_or((url.to_string(), 1883));

    let client_id = format!("agrodash-ping-{}", uuid::Uuid::new_v4());
    let mut opts = MqttOptions::new(client_id, host, port);
    opts.set_keep_alive(Duration::from_secs(5));
    if let (Some(u), Some(p)) = (conn.username.clone(), conn.password.clone()) {
        opts.set_credentials(u, p);
    }

    let (_client, mut eventloop) = AsyncClient::new(opts, 4);
    let t0 = std::time::Instant::now();

    let result = timeout(Duration::from_secs(5), async {
        loop {
            match eventloop.poll().await? {
                Event::Incoming(Packet::ConnAck(_)) => return Ok::<(), anyhow::Error>(()),
                _ => {}
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(t0.elapsed()),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("MQTT ping timeout (>5s)"),
    }
}

/// Verifica conectividad HTTP haciendo GET/HEAD al base_url.
pub async fn http_ping(conn: &HttpConnection) -> Result<Duration> {
    let timeout_dur = Duration::from_secs(conn.timeout_secs.unwrap_or(5));
    let client = reqwest::Client::builder().timeout(timeout_dur).build()?;
    let t0 = std::time::Instant::now();
    let res = client.get(&conn.base_url).send().await?;
    if res.status().is_server_error() {
        anyhow::bail!("HTTP ping: status {}", res.status());
    }
    Ok(t0.elapsed())
}
