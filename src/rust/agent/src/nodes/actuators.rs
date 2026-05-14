// agent/src/nodes/actuators.rs

use super::NodeInstance;
use agrodash_shared::{
    HttpActuatorConfig, HttpConnection, MqttActuatorConfig, MqttConnection,
    NodeAction, NodeState, Signal,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::mpsc;

// ── MQTT ──────────────────────────────────────────────────────────────────────

pub struct MqttActuatorNode {
    id:          String,
    cfg:         MqttActuatorConfig,
    conn_cfg:    MqttConnection,
    client:      Option<AsyncClient>,
    qos:         QoS,
    last_action: Option<NodeAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,

    /// Recibe mensajes del ack_topic (suscrito en el eventloop de background)
    inbox_rx: Option<mpsc::UnboundedReceiver<String>>,
}

impl MqttActuatorNode {
    pub async fn new(id: String, cfg: MqttActuatorConfig, conn: MqttConnection) -> Result<Self> {
        let qos = match conn.qos.unwrap_or(1) {
            0 => QoS::AtMostOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };

        let (inbox_tx, inbox_rx) = if cfg.ack_topic.is_some() {
            let (tx, rx) = mpsc::unbounded_channel::<String>();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let mut node = Self {
            id, cfg, conn_cfg: conn, client: None, qos,
            last_action: None, last_at: None,
            total_on: 0.0, on_since: None,
            inbox_rx,
        };
        node.connect(inbox_tx).await;
        Ok(node)
    }

    async fn connect(&mut self, inbox_tx: Option<mpsc::UnboundedSender<String>>) {
        let url = self.conn_cfg.broker_url.trim_start_matches("mqtt://");
        let (host, port) = url
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let mut opts = MqttOptions::new(self.conn_cfg.client_id.clone(), host, port);
        opts.set_keep_alive(Duration::from_secs(self.conn_cfg.keepalive_secs.unwrap_or(30)));
        if let (Some(u), Some(p)) = (self.conn_cfg.username.clone(), self.conn_cfg.password.clone()) {
            opts.set_credentials(u, p);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 32);
        let ack_topic    = self.cfg.ack_topic.clone();
        let client_clone = client.clone();
        let node_id      = self.id.clone();

        tokio::spawn(async move {
            let mut subscribed = false;
            let mut backoff    = Duration::from_secs(2);
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        backoff = Duration::from_secs(2);
                        if !subscribed {
                            if let Some(ref topic) = ack_topic {
                                match client_clone.subscribe(topic, QoS::AtMostOnce).await {
                                    Ok(_) => {
                                        tracing::info!("[MQTT {node_id}] suscrito a ack_topic={topic}");
                                        subscribed = true;
                                    }
                                    Err(e) => {
                                        tracing::warn!("[MQTT {node_id}] no se pudo suscribir a {topic}: {e}");
                                    }
                                }
                            }
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(msg))) => {
                        if let (Ok(payload), Some(ref tx)) = (
                            std::str::from_utf8(&msg.payload),
                            &inbox_tx,
                        ) {
                            // fire-and-forget; si el receiver se cerró, ignorar
                            let _ = tx.send(payload.to_string());
                        }
                    }
                    Ok(_) => { backoff = Duration::from_secs(2); }
                    Err(e) => {
                        subscribed = false;
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
        tracing::info!("[MQTT {}] conectado a {}", self.id, self.conn_cfg.broker_url);
    }

    async fn publish(&mut self, payload: &str) -> Result<()> {
        if self.client.is_none() {
            // Sin inbox_tx al reconectar — las stages ya no recibirán ACKs
            // pero la publicación sigue funcionando
            self.connect(None).await;
        }
        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sin cliente MQTT"))?;
        client.publish(
            &self.cfg.topic,
            self.qos,
            self.cfg.retain.unwrap_or(false),
            payload.as_bytes().to_vec(),
        ).await?;
        Ok(())
    }

    /// Espera las confirmaciones de etapas en background (spawn).
    /// No bloquea el ciclo del agente — el agente sigue corriendo normalmente.
    /// El progreso se publica via pg_notify → WS hub → frontend.
    fn spawn_stage_wait(
        &mut self,
        action:      String,
        payload:     String,
        pool:        PgPool,
        process_id:  String,
        pipeline_id: String,
    ) {
        let stages = match &self.cfg.stages {
            Some(s) if !s.is_empty() => s.clone(),
            _ => return,
        };

        let node_id    = self.id.clone();
        let mut inbox = match self.inbox_rx.take() {
            Some(r) => r,
            None    => return,
        };

        tokio::spawn(async move {
            tracing::debug!("[MQTT {node_id}] iniciando espera de {n} etapas", n = stages.len());

            for (i, stage) in stages.iter().enumerate() {
                let timeout_dur = Duration::from_secs_f64(stage.timeout_secs);
                let deadline    = tokio::time::Instant::now() + timeout_dur;

                notify_stage(
                    &pool, &process_id, &pipeline_id, &node_id,
                    &stage.name, "waiting", "", &action, &payload, i,
                ).await;

                let result: Result<String, String> = loop {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        break Err(format!(
                            "Timeout en etapa '{}' ({:.0}s)",
                            stage.name, stage.timeout_secs
                        ));
                    }

                    match tokio::time::timeout(remaining, inbox.recv()).await {
                        Ok(Some(msg)) => {
                            if stage.matches(&msg, &action, &payload) {
                                break Ok(msg);
                            }
                            if stage.is_error(&msg, &action, &payload) {
                                break Err(format!("Error en etapa '{}': {msg}", stage.name));
                            }
                            // Mensaje que no aplica a esta etapa — seguir esperando
                        }
                        Ok(None)  => break Err("Canal de ACK cerrado".into()),
                        Err(_)    => break Err(format!(
                            "Timeout en etapa '{}' ({:.0}s)",
                            stage.name, stage.timeout_secs
                        )),
                    }
                };

                match result {
                    Ok(msg) => {
                        tracing::info!("[MQTT {node_id}] etapa {i} '{}' ✓ → {msg}", stage.name);
                        notify_stage(
                            &pool, &process_id, &pipeline_id, &node_id,
                            &stage.name, "ok", &msg, &action, &payload, i,
                        ).await;
                        if stage.terminal {
                            tracing::info!(
                                "[MQTT {node_id}] ✓ confirmado en etapa terminal '{}'",
                                stage.name
                            );
                            break;
                        }
                    }
                    Err(reason) => {
                        tracing::warn!("[MQTT {node_id}] etapa {i} '{}' ✗ → {reason}", stage.name);
                        notify_stage(
                            &pool, &process_id, &pipeline_id, &node_id,
                            &stage.name, "error", &reason, &action, &payload, i,
                        ).await;
                        break; // Detener en la etapa que falló
                    }
                }
            }

            // Devolver el inbox al nodo no es posible desde aquí (moved).
            // Al próximo comando, inbox_rx será None y spawn_stage_wait
            // retornará sin hacer nada. Esto es aceptable: en la práctica
            // los comandos son secuenciales (busy lock en el frontend).
        });
    }
}

/// Publica el estado de una etapa via PG NOTIFY.
/// El canal es `ws_ack_{process_id}` — el WS hub lo escucha y forwarda al frontend.
async fn notify_stage(
    pool:        &PgPool,
    process_id:  &str,
    pipeline_id: &str,
    node_id:     &str,
    stage_name:  &str,
    status:      &str,
    message:     &str,
    action:      &str,
    payload:     &str,
    stage_index: usize,
) {
    let channel = format!("ws_ack_{}", process_id.replace('-', "_"));
    let data = serde_json::json!({
        "type":        "mqtt_ack",
        "pipeline_id": pipeline_id,
        "actuator_id": node_id,
        "stage_index": stage_index,
        "stage_name":  stage_name,
        "status":      status,
        "message":     message,
        "action":      action,
        "payload":     payload,
    });
    if let Ok(json) = serde_json::to_string(&data) {
        let _ = sqlx::query("SELECT pg_notify($1, $2)")
            .bind(&channel)
            .bind(&json)
            .execute(pool)
            .await;
    }
}

#[async_trait]
impl NodeInstance for MqttActuatorNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        pool: &PgPool,
        ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let action = match inputs.first() {
            Some(Signal::Action(a)) => a.clone(),
            _ => return Ok(None),
        };
        self.execute_with_ctx(action, pool, ctx).await
    }

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        // Sin pool disponible en este path — no hay stages
        match &action {
            NodeAction::On if self.last_action != Some(NodeAction::On) => {
                self.publish(&self.cfg.payload_on.clone()).await?;
                self.on_since    = Some(std::time::Instant::now());
                self.last_action = Some(NodeAction::On);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!(
                    "[MQTT {}] ▶ ON → topic='{}' payload='{}'",
                    self.id, self.cfg.topic, self.cfg.payload_on
                );
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() { self.total_on += t.elapsed().as_secs_f64(); }
                self.publish(&self.cfg.payload_off.clone()).await?;
                self.last_action = Some(NodeAction::Off);
                self.last_at     = Some(Utc::now().to_rfc3339());
                tracing::info!(
                    "[MQTT {}] ■ OFF → topic='{}' payload='{}'",
                    self.id, self.cfg.topic, self.cfg.payload_off
                );
            }
            _ => {}
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
        self.last_action = None;
        self.last_at = state.data.get("last_at")
            .and_then(|v| v.as_str().map(String::from));
        self.total_on = state.data.get("total_on")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
    }
}

impl MqttActuatorNode {
    /// Variante de execute con acceso al pool y contexto para lanzar el watcher de stages.
    async fn execute_with_ctx(
        &mut self,
        action: NodeAction,
        pool:   &PgPool,
        ctx:    &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let changed = match &action {
            NodeAction::On  => self.last_action != Some(NodeAction::On),
            NodeAction::Off => self.last_action != Some(NodeAction::Off),
            _ => false,
        };

        if changed {
            let payload = match &action {
                NodeAction::On  => self.cfg.payload_on.clone(),
                NodeAction::Off => self.cfg.payload_off.clone(),
                _               => String::new(),
            };

            self.publish(&payload).await?;

            match &action {
                NodeAction::On => {
                    self.on_since    = Some(std::time::Instant::now());
                    self.last_action = Some(NodeAction::On);
                    self.last_at     = Some(Utc::now().to_rfc3339());
                    tracing::info!(
                        "[MQTT {}] ▶ ON → topic='{}' payload='{}'",
                        self.id, self.cfg.topic, payload
                    );
                }
                NodeAction::Off => {
                    if let Some(t) = self.on_since.take() { self.total_on += t.elapsed().as_secs_f64(); }
                    self.last_action = Some(NodeAction::Off);
                    self.last_at     = Some(Utc::now().to_rfc3339());
                    tracing::info!(
                        "[MQTT {}] ■ OFF → topic='{}' payload='{}'",
                        self.id, self.cfg.topic, payload
                    );
                }
                _ => {}
            }

            // Lanzar el watcher de stages en background si está configurado
            if self.cfg.ack_topic.is_some() {
                // Necesitamos el process_id y pipeline_id — vienen del AgentShared.
                // Por ahora los ponemos como placeholder; se inyectarán en el siguiente paso
                // cuando refactoricemos NodeInstance para pasar SharedContext.
                // TODO: pasar process_id y pipeline_id via contexto del nodo
                self.spawn_stage_wait(
                    match &action { NodeAction::On => "on", _ => "off" }.into(),
                    payload,
                    pool.clone(),
                    ctx.process_id.clone(),
                    ctx.pipeline_id.clone(),
                );
            }
        }

        Ok(Some(Signal::Action(action)))
    }
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

pub struct HttpActuatorNode {
    id:          String,
    cfg:         HttpActuatorConfig,
    conn:        HttpConnection,
    client:      reqwest::Client,
    last_action: Option<NodeAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl HttpActuatorNode {
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
        Self { id, cfg, conn, client, last_action: None, last_at: None, total_on: 0.0, on_since: None }
    }

    async fn call(&self, path: &str, body: Option<&serde_json::Value>, method: &str) -> Result<()> {
        let url = format!("{}{}", self.conn.base_url.trim_end_matches('/'), path);
        let mut req = match method.to_uppercase().as_str() {
            "GET" => self.client.get(&url), "PUT" => self.client.put(&url),
            "PATCH" => self.client.patch(&url), _ => self.client.post(&url),
        };
        if let Some(extra) = &self.conn.extra_headers {
            for (k, v) in extra {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(v),
                ) { req = req.header(name, val); }
            }
        }
        if let Some(b) = body { req = req.json(b); }
        let res = req.send().await?;
        if res.status().is_server_error() {
            anyhow::bail!("HTTP actuator {}: status {}", self.id, res.status());
        }
        Ok(())
    }
}

#[async_trait]
impl NodeInstance for HttpActuatorNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool, _ctx: &super::NodeContext) -> Result<Option<Signal>> {
        let action = match inputs.first() { Some(Signal::Action(a)) => a.clone(), _ => return Ok(None) };
        self.execute_override(action).await
    }

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        let method = self.cfg.method.clone().unwrap_or_else(|| "POST".into());
        match &action {
            NodeAction::On if self.last_action != Some(NodeAction::On) => {
                self.call(&self.cfg.path_on.clone(), self.cfg.body_on.as_ref(), &method).await?;
                self.on_since = Some(std::time::Instant::now()); self.last_action = Some(NodeAction::On);
                self.last_at = Some(Utc::now().to_rfc3339());
                tracing::info!("[HTTP {}] ON → {}{}", self.id, self.conn.base_url, self.cfg.path_on);
            }
            NodeAction::Off if self.last_action != Some(NodeAction::Off) => {
                if let Some(t) = self.on_since.take() { self.total_on += t.elapsed().as_secs_f64(); }
                self.call(&self.cfg.path_off.clone(), self.cfg.body_off.as_ref(), &method).await?;
                self.last_action = Some(NodeAction::Off); self.last_at = Some(Utc::now().to_rfc3339());
                tracing::info!("[HTTP {}] OFF → {}{}", self.id, self.conn.base_url, self.cfg.path_off);
            }
            _ => {}
        }
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool { true }

    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "http_actuator".into(),
            data: serde_json::json!({ "last_action": self.last_action, "last_at": self.last_at, "total_on": self.total_on }),
            is_ready: true }
    }

    fn load_state(&mut self, state: &NodeState) {
        self.last_action = None;
        self.last_at = state.data.get("last_at").and_then(|v| v.as_str().map(String::from));
        self.total_on = state.data.get("total_on").and_then(|v| v.as_f64()).unwrap_or(0.0);
    }
}

// ── Ping helpers ───────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn mqtt_ping(conn: &MqttConnection) -> Result<Duration> {
    use tokio::time::timeout;
    let url = conn.broker_url.trim_start_matches("mqtt://");
    let (host, port) = url.split_once(':')
        .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
        .unwrap_or((url.to_string(), 1883));
    let client_id = format!("agrodash-ping-{}", uuid::Uuid::new_v4());
    let mut opts = MqttOptions::new(client_id, host, port);
    opts.set_keep_alive(Duration::from_secs(5));
    if let (Some(u), Some(p)) = (conn.username.clone(), conn.password.clone()) { opts.set_credentials(u, p); }
    let (_client, mut eventloop) = AsyncClient::new(opts, 4);
    let t0 = std::time::Instant::now();
    let result = timeout(Duration::from_secs(5), async {
        loop { if let Event::Incoming(Packet::ConnAck(_)) = eventloop.poll().await? { return Ok::<(), anyhow::Error>(()); } }
    }).await;
    match result {
        Ok(Ok(())) => Ok(t0.elapsed()),
        Ok(Err(e)) => Err(e),
        Err(_)     => anyhow::bail!("MQTT ping timeout (>5s)"),
    }
}

#[allow(dead_code)]
pub async fn http_ping(conn: &HttpConnection) -> Result<Duration> {
    let timeout_dur = Duration::from_secs(conn.timeout_secs.unwrap_or(5));
    let client = reqwest::Client::builder().timeout(timeout_dur).build()?;
    let t0 = std::time::Instant::now();
    let res = client.get(&conn.base_url).send().await?;
    if res.status().is_server_error() { anyhow::bail!("HTTP ping: status {}", res.status()); }
    Ok(t0.elapsed())
}