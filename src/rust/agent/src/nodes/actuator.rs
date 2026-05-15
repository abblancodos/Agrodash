// agent/src/nodes/actuator.rs
//
// Nodo Actuator unificado.
//
// Reemplaza: Hysteresis/SPRT/Mahalanobis + Watchdog + MqttActuator/HttpActuator
//
// Puerto de entrada:
//   Signal::Vector — señal filtrada del upstream (Kalman, MovingAvg, etc.)
//
// Puerto de salida:
//   Signal::Action — la acción decidida (para el próximo nodo si lo hay)
//
// Internamente:
//   1. Aplica DecisionMethod → NodeAction (On/Off/Hold)
//   2. Si cambió la acción → envía por OutputMethod
//   3. Si hay stages → lanza watcher de ACK en background via pg_notify
//   4. En cada ciclo → evalúa CoherenceCheck si está configurado

use super::{NodeContext, NodeInstance};
use agrodash_shared::{
    AckStage, ActuatorConfig, CoherenceCheck, DecisionMethod, NodeAction, NodeState, OutputMethod,
    Reduction, Signal, Trend,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// ── Estado interno ────────────────────────────────────────────────────────────

pub struct ActuatorNode {
    id: String,
    cfg: ActuatorConfig,

    // Decisión
    last_action: Option<NodeAction>,
    last_at: Option<String>,
    total_on: f64,
    on_since: Option<Instant>,

    // SPRT interno
    sprt_llr: f64,

    // Mahalanobis — covarianza adaptativa del Kalman (si use_kalman_p)
    kalman_p: Option<Vec<f64>>,

    // Override manual (API → nodo)
    override_action: Option<NodeAction>,

    // Bloqueo durante ACK — true mientras el watcher de stages está corriendo
    ack_locked: bool,

    // Canal para recibir mensajes del ack_topic MQTT
    inbox_rx: Option<mpsc::UnboundedReceiver<String>>,
    inbox_tx: Option<mpsc::UnboundedSender<String>>,

    // Cliente MQTT activo (para publicar y para el eventloop de ACK)
    mqtt_client: Option<rumqttc::AsyncClient>,

    // Coherence check — valor de señal al momento de la actuación
    coherence_baseline: Option<Vec<f64>>,
    coherence_started_at: Option<Instant>,
    coherence_alert: Option<String>, // None = OK, Some(msg) = alerta

    // Retry
    retry_count: u32,
}

impl ActuatorNode {
    pub async fn new(
        id: String,
        cfg: ActuatorConfig,
        shared_connections: &Option<agrodash_shared::SharedConnections>,
    ) -> Result<Self> {
        let has_ack = matches!(
            &cfg.output,
            OutputMethod::Mqtt {
                ack_topic: Some(_),
                ..
            }
        );
        let (inbox_tx, inbox_rx) = if has_ack {
            let (tx, rx) = mpsc::unbounded_channel::<String>();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let mut node = Self {
            id,
            cfg,
            last_action: None,
            last_at: None,
            total_on: 0.0,
            on_since: None,
            sprt_llr: 0.0,
            kalman_p: None,
            override_action: None,
            ack_locked: false,
            inbox_rx,
            inbox_tx,
            mqtt_client: None,
            coherence_baseline: None,
            coherence_started_at: None,
            coherence_alert: None,
            retry_count: 0,
        };

        if let OutputMethod::Mqtt { .. } = &node.cfg.output {
            node.connect_mqtt(shared_connections).await;
        }

        Ok(node)
    }

    // ── MQTT connect ──────────────────────────────────────────────────────────

    async fn connect_mqtt(&mut self, shared: &Option<agrodash_shared::SharedConnections>) {
        use super::resolve_mqtt_connection;
        use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};

        let (conn_ref, ack_topic) = match &self.cfg.output {
            OutputMethod::Mqtt {
                connection,
                ack_topic,
                ..
            } => (connection.clone(), ack_topic.clone()),
            _ => return,
        };

        let mqtt_conn = match resolve_mqtt_connection(&conn_ref, shared) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[Actuator {}] conexión MQTT: {e}", self.id);
                return;
            }
        };

        let url = mqtt_conn.broker_url.trim_start_matches("mqtt://");
        let (host, port) = url
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let mut opts = MqttOptions::new(mqtt_conn.client_id.clone(), host, port);
        opts.set_keep_alive(Duration::from_secs(mqtt_conn.keepalive_secs.unwrap_or(30)));
        if let (Some(u), Some(p)) = (mqtt_conn.username, mqtt_conn.password) {
            opts.set_credentials(u, p);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 32);
        let inbox_tx = self.inbox_tx.clone();
        let node_id = self.id.clone();

        tokio::spawn(async move {
            let mut subscribed = false;
            let mut backoff = Duration::from_secs(2);
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        backoff = Duration::from_secs(2);
                        if !subscribed {
                            if let Some(ref topic) = ack_topic {
                                if client.subscribe(topic, QoS::AtMostOnce).await.is_ok() {
                                    tracing::info!("[Actuator {node_id}] suscrito a {topic}");
                                    subscribed = true;
                                }
                            }
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(msg))) => {
                        if let (Ok(payload), Some(ref tx)) =
                            (std::str::from_utf8(&msg.payload), &inbox_tx)
                        {
                            tx.send(payload.to_string()).ok();
                        }
                    }
                    Ok(_) => {
                        backoff = Duration::from_secs(2);
                    }
                    Err(e) => {
                        subscribed = false;
                        tracing::warn!(
                            "[Actuator {node_id}] MQTT: {e} — retry en {}s",
                            backoff.as_secs()
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });

        self.mqtt_client = Some(client);
        tracing::info!("[Actuator {}] MQTT conectado", self.id);
    }

    // ── Decisión ──────────────────────────────────────────────────────────────

    fn decide(&mut self, signal: &[f64]) -> NodeAction {
        match &self.cfg.decision.clone() {
            DecisionMethod::Hysteresis {
                reduction,
                low,
                high,
                action_below,
                action_above,
            } => {
                let val = apply_reduction(signal, reduction);
                let current = self.last_action.clone().unwrap_or(NodeAction::Hold);
                // Histéresis: solo cambia si cruza el umbral opuesto
                match &current {
                    NodeAction::On if val <= *low => action_below.clone(),
                    NodeAction::Off if val >= *high => action_above.clone(),
                    NodeAction::Hold => {
                        if val >= *high {
                            action_above.clone()
                        } else if val <= *low {
                            action_below.clone()
                        } else {
                            NodeAction::Hold
                        }
                    }
                    other => other.clone(),
                }
            }

            DecisionMethod::Mahalanobis {
                target,
                threshold_act,
                threshold_deact,
                ..
            } => {
                let d = mahalanobis_distance(signal, target);
                let current = self.last_action.clone().unwrap_or(NodeAction::Hold);
                match &current {
                    NodeAction::On if d <= *threshold_deact => NodeAction::Off,
                    NodeAction::Off if d >= *threshold_act => NodeAction::On,
                    NodeAction::Hold => {
                        if d >= *threshold_act {
                            NodeAction::On
                        } else {
                            NodeAction::Off
                        }
                    }
                    other => other.clone(),
                }
            }

            DecisionMethod::Sprt {
                mu_h0,
                mu_h1,
                sigma,
                alpha,
                beta,
                reduction,
                reset_on_action,
            } => {
                let val = apply_reduction(signal, reduction);
                let log_ratio = sprt_step(val, *mu_h0, *mu_h1, *sigma);
                self.sprt_llr += log_ratio;

                let threshold_h1 = (1.0 - beta) / alpha;
                let threshold_h0 = beta / (1.0 - alpha);

                let action = if self.sprt_llr >= threshold_h1.ln() {
                    NodeAction::On
                } else if self.sprt_llr <= threshold_h0.ln() {
                    NodeAction::Off
                } else {
                    self.last_action.clone().unwrap_or(NodeAction::Hold)
                };

                if *reset_on_action && Some(&action) != self.last_action.as_ref() {
                    self.sprt_llr = 0.0;
                }

                action
            }
        }
    }

    // ── Envío ─────────────────────────────────────────────────────────────────

    async fn send_action(&mut self, action: &NodeAction) -> Result<()> {
        match &self.cfg.output.clone() {
            OutputMethod::Mqtt {
                topic,
                payload_on,
                payload_off,
                ..
            } => {
                let payload = match action {
                    NodeAction::On => payload_on.as_str(),
                    NodeAction::Off => payload_off.as_str(),
                    NodeAction::Hold => return Ok(()),
                };
                use rumqttc::QoS;
                let retain = matches!(
                    &self.cfg.output,
                    OutputMethod::Mqtt {
                        retain: Some(true),
                        ..
                    }
                );
                if let Some(ref client) = self.mqtt_client {
                    client
                        .publish(topic, QoS::AtLeastOnce, retain, payload.as_bytes().to_vec())
                        .await?;
                    tracing::info!(
                        "[Actuator {}] {} → {topic} = {payload}",
                        self.id,
                        if matches!(action, NodeAction::On) {
                            "▶ ON"
                        } else {
                            "■ OFF"
                        }
                    );
                }
            }

            OutputMethod::Http {
                connection,
                path_on,
                path_off,
                body_on,
                body_off,
                method,
            } => {
                // HTTP send — reutilizamos la lógica del HttpActuatorNode
                let (path, body) = match action {
                    NodeAction::On => (path_on.as_str(), body_on.as_ref()),
                    NodeAction::Off => (path_off.as_str(), body_off.as_ref()),
                    NodeAction::Hold => return Ok(()),
                };
                tracing::info!(
                    "[Actuator {}] HTTP {} {path}",
                    self.id,
                    method.as_deref().unwrap_or("POST")
                );
                // La conexión HTTP se resuelve al construir — aquí simplemente
                // notamos que está pendiente de implementar igual que HttpActuatorNode
                let _ = (path, body);
            }
        }
        Ok(())
    }

    // ── Stage watcher ─────────────────────────────────────────────────────────

    fn spawn_stage_watcher(
        &mut self,
        action: String,
        payload: String,
        pool: PgPool,
        process_id: String,
        pipeline_id: String,
    ) {
        let stages = match &self.cfg.stages {
            Some(s) if !s.is_empty() => s.clone(),
            _ => return,
        };

        let node_id = self.id.clone();
        let mut rx = match self.inbox_rx.take() {
            Some(r) => r,
            None => return,
        };

        tokio::spawn(async move {
            for (i, stage) in stages.iter().enumerate() {
                notify_stage(
                    &pool,
                    &process_id,
                    &pipeline_id,
                    &node_id,
                    &stage.name,
                    "waiting",
                    "",
                    &action,
                    &payload,
                    i,
                )
                .await;

                let timeout = Duration::from_secs_f64(stage.timeout_secs);
                let deadline = tokio::time::Instant::now() + timeout;

                let result: Result<String, String> = loop {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        break Err(format!(
                            "Timeout en '{}' ({:.0}s)",
                            stage.name, stage.timeout_secs
                        ));
                    }
                    match tokio::time::timeout(remaining, rx.recv()).await {
                        Ok(Some(msg)) => {
                            if stage.matches(&msg, &action, &payload) {
                                break Ok(msg);
                            }
                            if stage.is_error(&msg, &action, &payload) {
                                break Err(format!("Error en '{}': {msg}", stage.name));
                            }
                        }
                        Ok(None) => break Err("Canal cerrado".into()),
                        Err(_) => {
                            break Err(format!(
                                "Timeout en '{}' ({:.0}s)",
                                stage.name, stage.timeout_secs
                            ))
                        }
                    }
                };

                match result {
                    Ok(msg) => {
                        tracing::info!("[Actuator {node_id}] etapa {i} '{}' ✓", stage.name);
                        notify_stage(
                            &pool,
                            &process_id,
                            &pipeline_id,
                            &node_id,
                            &stage.name,
                            "ok",
                            &msg,
                            &action,
                            &payload,
                            i,
                        )
                        .await;
                        if stage.terminal {
                            break;
                        }
                    }
                    Err(reason) => {
                        tracing::warn!(
                            "[Actuator {node_id}] etapa {i} '{}' ✗ {reason}",
                            stage.name
                        );
                        notify_stage(
                            &pool,
                            &process_id,
                            &pipeline_id,
                            &node_id,
                            &stage.name,
                            "error",
                            &reason,
                            &action,
                            &payload,
                            i,
                        )
                        .await;
                        break;
                    }
                }
            }
        });
    }

    // ── Coherence check ───────────────────────────────────────────────────────

    fn update_coherence(&mut self, signal: &[f64], action: &NodeAction) {
        let cfg = match &self.cfg.coherence {
            Some(c) => c.clone(),
            None => return,
        };

        // Al cambiar la acción, registrar la baseline
        if Some(action) != self.last_action.as_ref() {
            self.coherence_baseline = Some(signal.to_vec());
            self.coherence_started_at = Some(Instant::now());
            self.coherence_alert = None;
            return;
        }

        // Evaluar solo después de la ventana
        let elapsed = match self.coherence_started_at {
            Some(t) => t.elapsed().as_secs_f64(),
            None => return,
        };
        if elapsed < cfg.window_secs {
            return;
        }

        let baseline = match &self.coherence_baseline {
            Some(b) => b.clone(),
            None => return,
        };

        let current_val = extract_component(signal, cfg.component);
        let baseline_val = extract_component(&baseline, cfg.component);

        if baseline_val.abs() < 1e-12 && cfg.relative {
            return;
        }

        let delta = current_val - baseline_val;
        let delta_norm = if cfg.relative && baseline_val.abs() > 1e-12 {
            delta / baseline_val.abs()
        } else {
            delta
        };

        let expected = match action {
            NodeAction::On => &cfg.expected_on,
            NodeAction::Off => &cfg.expected_off,
            NodeAction::Hold => return,
        };

        let ok = match expected {
            Trend::Ascending => delta_norm >= cfg.min_delta,
            Trend::Descending => delta_norm <= -cfg.min_delta,
            Trend::Stable => delta_norm.abs() < cfg.min_delta,
        };

        if !ok {
            let alert = cfg.alert_label.clone().unwrap_or_else(|| {
                format!(
                    "Incoherencia: acción={:?} pero Δseñal={:+.3} (esperado {:?} ≥{:.3})",
                    action, delta_norm, expected, cfg.min_delta
                )
            });
            if self.coherence_alert.as_deref() != Some(&alert) {
                tracing::warn!("[Actuator {}] ⚠ {alert}", self.id);
                self.coherence_alert = Some(alert);
            }
            // Resetear la ventana para la próxima evaluación
            self.coherence_baseline = Some(signal.to_vec());
            self.coherence_started_at = Some(Instant::now());
        } else {
            self.coherence_alert = None;
        }
    }
}

// ── NodeInstance ──────────────────────────────────────────────────────────────

#[async_trait]
impl NodeInstance for ActuatorNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        pool: &PgPool,
        ctx: &NodeContext,
    ) -> Result<Option<Signal>> {
        let signal = match inputs.first() {
            Some(Signal::Vector(v)) => v.clone(),
            _ => return Ok(None),
        };

        // Override manual tiene prioridad
        let action = if let Some(ov) = &self.override_action.clone() {
            ov.clone()
        } else {
            self.decide(&signal)
        };

        // Coherence check (no bloquea, solo alerta)
        self.update_coherence(&signal, &action);

        // Si la acción cambió → enviar
        let changed = self.last_action.as_ref() != Some(&action) && action != NodeAction::Hold;

        if changed {
            self.send_action(&action).await?;

            // Actualizar contadores de tiempo encendido
            match &action {
                NodeAction::On => {
                    self.on_since = Some(Instant::now());
                }
                NodeAction::Off => {
                    if let Some(t) = self.on_since.take() {
                        self.total_on += t.elapsed().as_secs_f64();
                    }
                }
                _ => {}
            }

            self.last_at = Some(Utc::now().to_rfc3339());

            // Lanzar watcher de stages si está configurado
            if self
                .cfg
                .stages
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
            {
                let payload = match &self.cfg.output {
                    OutputMethod::Mqtt {
                        payload_on,
                        payload_off,
                        ..
                    } => match &action {
                        NodeAction::On => payload_on.clone(),
                        NodeAction::Off => payload_off.clone(),
                        _ => String::new(),
                    },
                    OutputMethod::Http {
                        path_on, path_off, ..
                    } => match &action {
                        NodeAction::On => path_on.clone(),
                        NodeAction::Off => path_off.clone(),
                        _ => String::new(),
                    },
                };
                let action_str = match &action {
                    NodeAction::On => "on",
                    NodeAction::Off => "off",
                    _ => "hold",
                }
                .to_string();

                self.ack_locked = true;
                self.spawn_stage_watcher(
                    action_str,
                    payload,
                    pool.clone(),
                    ctx.process_id.clone(),
                    ctx.pipeline_id.clone(),
                );
            }
        }

        self.last_action = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }

    async fn execute_override(&mut self, action: NodeAction) -> Result<Option<Signal>> {
        self.override_action = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }

    fn is_actuator(&self) -> bool {
        true
    }

    fn save_state(&self) -> NodeState {
        let stages_status = self.cfg.stages.as_ref().map(|s| s.len()).unwrap_or(0);
        NodeState {
            node_id: self.id.clone(),
            node_type: "actuator".into(),
            data: serde_json::json!({
                "label":           self.cfg.label,
                "last_action":     self.last_action,
                "last_at":         self.last_at,
                "total_on":        self.total_on,
                "ack_locked":      self.ack_locked,
                "stages_count":    stages_status,
                "retry_count":     self.retry_count,
                "coherence_alert": self.coherence_alert,
                "override_action": self.override_action,
                // Para set_watchdog_override() del scheduler
                "actuator_id":     self.id,
            }),
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
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
        self.retry_count = state
            .data
            .get("retry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
    }

    fn watchdog_set_override(&mut self, action: NodeAction) {
        self.override_action = Some(action);
    }

    fn has_watchdog_override(&self) -> bool {
        self.override_action.is_some()
    }

    fn watchdog_reset(&mut self) {
        self.override_action = None;
        self.ack_locked = false;
        self.coherence_alert = None;
        self.retry_count = 0;
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        vec![
            ("total_on".into(), self.total_on),
            ("ack_locked".into(), if self.ack_locked { 1.0 } else { 0.0 }),
            ("retry_count".into(), self.retry_count as f64),
            (
                "coherence_ok".into(),
                if self.coherence_alert.is_none() {
                    1.0
                } else {
                    0.0
                },
            ),
        ]
    }
}

// ── Helpers matemáticos ───────────────────────────────────────────────────────

fn apply_reduction(signal: &[f64], reduction: &Reduction) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    match reduction {
        Reduction::Mean => signal.iter().sum::<f64>() / signal.len() as f64,
        Reduction::Min => signal.iter().cloned().fold(f64::INFINITY, f64::min),
        Reduction::Max => signal.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        Reduction::Component { index } => signal.get(*index).copied().unwrap_or(0.0),
        Reduction::WeightedByP => signal.iter().sum::<f64>() / signal.len() as f64,
        _ => signal.iter().sum::<f64>() / signal.len() as f64,
    }
}

fn extract_component(signal: &[f64], component: Option<usize>) -> f64 {
    match component {
        Some(i) => signal.get(i).copied().unwrap_or(0.0),
        None => signal.iter().map(|x| x * x).sum::<f64>().sqrt(),
    }
}

fn mahalanobis_distance(signal: &[f64], target: &[f64]) -> f64 {
    let n = signal.len().min(target.len());
    if n == 0 {
        return 0.0;
    }
    let sum_sq: f64 = (0..n).map(|i| (signal[i] - target[i]).powi(2)).sum();
    sum_sq.sqrt() / (n as f64).sqrt()
}

fn sprt_step(val: f64, mu_h0: f64, mu_h1: f64, sigma: f64) -> f64 {
    if sigma < 1e-12 {
        return 0.0;
    }
    let s2 = sigma * sigma;
    ((mu_h1 - mu_h0) / s2) * val - (mu_h1.powi(2) - mu_h0.powi(2)) / (2.0 * s2)
}

// ── pg_notify helper ──────────────────────────────────────────────────────────

async fn notify_stage(
    pool: &PgPool,
    process_id: &str,
    pipeline_id: &str,
    node_id: &str,
    stage_name: &str,
    status: &str,
    message: &str,
    action: &str,
    payload: &str,
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
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(&channel)
            .bind(&json)
            .execute(pool)
            .await
            .ok();
    }
}
