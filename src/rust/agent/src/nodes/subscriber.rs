// agent/src/nodes/subscriber.rs
//
// MqttSubscriberNode — nodo fuente de feedback para el Watchdog (modo Good).
//
// Suscribe un topic MQTT y mantiene el último estado recibido (On/Off).
// Su salida es Signal::Action — se conecta al puerto "feedback" del Watchdog
// con EdgeKind::Feedback en el EdgeConfig.
//
// Arquitectura del eventloop:
//   El cliente MQTT corre en una task tokio separada y comunica el último
//   payload recibido via un Arc<Mutex<SubscriberState>>. El execute() lee
//   ese estado en cada ciclo del agente sin bloquear.

use super::NodeInstance;
use agrodash_shared::{MqttConnection, MqttSubscriberConfig, NodeAction, NodeState, Signal};
use anyhow::Result;
use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── Estado compartido entre la task MQTT y el nodo ───────────────────────────

#[derive(Debug, Clone)]
struct SubscriberState {
    last_action: Option<NodeAction>,
    last_received: Option<Instant>,
    last_payload: Option<String>,
}

impl SubscriberState {
    fn new() -> Self {
        Self {
            last_action: None,
            last_received: None,
            last_payload: None,
        }
    }
}

// ── Nodo ─────────────────────────────────────────────────────────────────────

pub struct MqttSubscriberNode {
    id: String,
    cfg: MqttSubscriberConfig,
    state: Arc<Mutex<SubscriberState>>,
    // Guardamos el cliente para que el Drop no lo cierre antes de tiempo
    _client: Option<AsyncClient>,
}

impl MqttSubscriberNode {
    pub async fn new(id: String, cfg: MqttSubscriberConfig, conn: MqttConnection) -> Result<Self> {
        let state = Arc::new(Mutex::new(SubscriberState::new()));
        let client = Self::spawn_subscriber(id.clone(), &cfg, conn, Arc::clone(&state)).await?;
        Ok(Self {
            id,
            cfg,
            state,
            _client: Some(client),
        })
    }

    async fn spawn_subscriber(
        node_id: String,
        cfg: &MqttSubscriberConfig,
        conn: MqttConnection,
        state: Arc<Mutex<SubscriberState>>,
    ) -> Result<AsyncClient> {
        let url = conn.broker_url.trim_start_matches("mqtt://");
        let (host, port) = url
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let client_id = format!("{}-sub-{}", conn.client_id, uuid::Uuid::new_v4());
        let mut opts = MqttOptions::new(client_id, host, port);
        opts.set_keep_alive(Duration::from_secs(conn.keepalive_secs.unwrap_or(30)));
        if let (Some(u), Some(p)) = (conn.username.clone(), conn.password.clone()) {
            opts.set_credentials(u, p);
        }

        let qos = match conn.qos.unwrap_or(1) {
            0 => QoS::AtMostOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };

        let (client, mut eventloop) = AsyncClient::new(opts, 16);
        client.subscribe(&cfg.topic, qos).await?;

        let topic = cfg.topic.clone();
        let payload_on = cfg.payload_on.clone();
        let payload_off = cfg.payload_off.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(2);
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(msg))) => {
                        backoff = Duration::from_secs(2);
                        let payload = String::from_utf8_lossy(&msg.payload).to_string();
                        let action = if payload == payload_on {
                            Some(NodeAction::On)
                        } else if payload == payload_off {
                            Some(NodeAction::Off)
                        } else {
                            tracing::warn!(
                                "[MqttSubscriber:{}] payload desconocido en '{}': '{}'",
                                node_id,
                                topic,
                                payload
                            );
                            None
                        };
                        if let Some(a) = action {
                            let mut s = state
                                .lock()
                                .expect("subscriber state lock poisoned — task panicked");
                            s.last_action = Some(a);
                            s.last_received = Some(Instant::now());
                            s.last_payload = Some(payload);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            "[MqttSubscriber:{}] error MQTT: {} — reintento en {}s",
                            node_id,
                            e,
                            backoff.as_secs()
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });

        Ok(client)
    }

    /// Devuelve el estado actual considerando staleness.
    fn read_action(&self) -> (Option<NodeAction>, bool) {
        let s = self
            .state
            .lock()
            .expect("subscriber state lock poisoned — task panicked");
        let stale = match (self.cfg.stale_after_secs, &s.last_received) {
            (Some(secs), Some(t)) => t.elapsed().as_secs() > secs,
            (Some(_), None) => true, // nunca recibimos nada → stale
            (None, _) => false,      // sin límite de staleness
        };
        (s.last_action.clone(), stale)
    }
}

#[async_trait]
impl NodeInstance for MqttSubscriberNode {
    async fn execute(
        &mut self,
        _inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
    ) -> Result<Option<Signal>> {
        let (action, stale) = self.read_action();
        if stale {
            tracing::debug!("[MqttSubscriber:{}] lectura obsoleta (stale)", self.id);
            // Emitimos Hold para indicar que no sabemos el estado actual.
            // El Watchdog en modo Good trata Hold como "stale" → inicia retry.
            return Ok(Some(Signal::Action(NodeAction::Hold)));
        }
        match action {
            Some(a) => Ok(Some(Signal::Action(a))),
            // Todavía no recibimos ningún mensaje → mismo tratamiento que stale
            None => Ok(Some(Signal::Action(NodeAction::Hold))),
        }
    }

    fn save_state(&self) -> NodeState {
        let s = self
            .state
            .lock()
            .expect("subscriber state lock poisoned — task panicked");
        let stale = match (self.cfg.stale_after_secs, &s.last_received) {
            (Some(secs), Some(t)) => t.elapsed().as_secs() > secs,
            (Some(_), None) => true,
            (None, _) => false,
        };
        NodeState {
            node_id: self.id.clone(),
            node_type: "mqtt_subscriber".into(),
            data: serde_json::json!({
                "last_action":  s.last_action,
                "last_payload": s.last_payload,
                "stale":        stale,
                "topic":        self.cfg.topic,
            }),
            is_ready: true,
        }
    }

    fn load_state(&mut self, _state: &NodeState) {
        // El estado de feedback siempre viene del broker en tiempo real —
        // no restauramos last_action desde DB para evitar leer un estado
        // obsoleto de antes del reinicio.
    }
}
