// api/src/ws.rs
//
// WebSocket hub para comunicación bidireccional API ↔ frontend.
//
// Arquitectura:
//   - Un broadcast::Sender<WsEvent> por process_id, guardado en WsBroadcast.
//   - El agente publica estado vía POST /agent-state → post_agent_state() llama
//     a WsBroadcast::publish(). Todos los WS conectados a ese proceso lo reciben.
//   - El frontend puede enviar comandos por el WS (mismo protocolo que el
//     endpoint POST /command). La API los procesa en el mismo handler.
//   - Sin polling, sin SSE que llene la memoria.
//
// Mensajes servidor → cliente (JSON):
//   { "type": "pipeline_state",  "pipeline_id": "...", "state": {...} }
//   { "type": "process_status",  "status": "running", "last_seen_at": "..." }
//   { "type": "mqtt_ack",        "pipeline_id": "...", "actuator_id": "...", "msg": "CONCENTRADOR:..." }
//   { "type": "log",             "level": "info", "source": "...", "message": "..." }
//   { "type": "error",           "message": "..." }
//   { "type": "pong" }
//
// Mensajes cliente → servidor (JSON):
//   { "type": "ping" }
//   { "type": "command", "cmd": "Override", "action": "on", "pipeline_id": "...", "actuator_id": "..." }
//   { "type": "command", "cmd": "ClearOverride", ... }
//   (cualquier AgentCommand envuelto en { "type": "command", ...campos })

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::AppState;

// ── Capacidad del canal de broadcast por proceso ──────────────────────────────
// 64 mensajes en cola — suficiente para ráfagas de estado sin desperdiciar memoria.
// Si un cliente va muy lento y se queda atrás, recibirá RecvError::Lagged y
// el handler lo desconectará limpiamente.
const BROADCAST_CAPACITY: usize = 64;

// ── WsEvent: lo que se emite en el broadcast ─────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    PipelineState {
        pipeline_id: String,
        state: Value,
    },
    ProcessStatus {
        status: String,
        last_seen_at: Option<String>,
    },
    MqttAck {
        pipeline_id: String,
        actuator_id: String,
        msg: String,
    },
    Log {
        level: String,
        source: String,
        message: String,
    },
}

// ── WsBroadcast: tabla process_id → Sender ───────────────────────────────────

#[derive(Clone, Default)]
pub struct WsBroadcast {
    inner: Arc<RwLock<HashMap<Uuid, broadcast::Sender<WsEvent>>>>,
    /// Senders de shutdown para el relay MQTT de cada proceso.
    /// Cuando se envía (), el task mqtt_relay termina.
    relay_shutdown: Arc<RwLock<HashMap<Uuid, tokio::sync::oneshot::Sender<()>>>>,
}

impl WsBroadcast {
    /// Obtiene o crea un Sender para el proceso. El Sender vive mientras haya
    /// Receivers (clientes WS conectados) o mientras la API no lo limpie.
    pub async fn sender(&self, process_id: Uuid) -> broadcast::Sender<WsEvent> {
        // Fast path: ya existe
        {
            let map = self.inner.read().await;
            if let Some(tx) = map.get(&process_id) {
                return tx.clone();
            }
        }
        // Slow path: crear
        let mut map = self.inner.write().await;
        // Double-check tras upgrade
        if let Some(tx) = map.get(&process_id) {
            return tx.clone();
        }
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        map.insert(process_id, tx.clone());
        tx
    }

    /// Publica un evento a todos los clientes WS del proceso.
    /// Fire-and-forget — si no hay receptores simplemente se descarta.
    pub async fn publish(&self, process_id: Uuid, event: WsEvent) {
        let map = self.inner.read().await;
        if let Some(tx) = map.get(&process_id) {
            // send() falla si no hay receivers — ignorar
            drop(tx.send(event));
        }
    }

    /// Limpia entradas de procesos sin receptores activos.
    /// Llamar ocasionalmente para evitar acumulación de Senders muertos.
    pub async fn gc(&self) {
        let mut map = self.inner.write().await;
        map.retain(|_, tx| tx.receiver_count() > 0);
    }

    /// Lanzar el relay MQTT para un proceso si aún no está corriendo.
    /// Se llama cuando el primer cliente WS conecta al proceso.
    pub async fn ensure_mqtt_relay(
        &self,
        process_id: Uuid,
        broker_url: String,
        client_id: String,
        username: Option<String>,
        password: Option<String>,
        ack_topic: String,
    ) {
        let already_running = self.relay_shutdown.read().await.contains_key(&process_id);
        if already_running {
            return;
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.relay_shutdown.write().await.insert(process_id, tx);

        let ws_clone = self.clone();
        tokio::spawn(crate::tasks::mqtt_relay::run(
            process_id, broker_url, client_id, username, password, ack_topic, ws_clone, rx,
        ));
    }

    /// Detener el relay MQTT de un proceso (cuando todos los clientes WS se desconectan).
    pub async fn stop_mqtt_relay(&self, process_id: Uuid) {
        if let Some(tx) = self.relay_shutdown.write().await.remove(&process_id) {
            tx.send(()).ok();
        }
    }
}

// ── Mensaje cliente → servidor ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ClientMsg {
    #[serde(rename = "type")]
    msg_type: String,
    // El resto de campos se pasan directamente al command handler
    #[serde(flatten)]
    payload: Value,
}

// ── Handler WebSocket ─────────────────────────────────────────────────────────

/// GET /api/v1/processes/:id/ws
///
/// Upgrade a WebSocket. Requiere cookie de sesión válida (verificada con JWT
/// antes del upgrade, igual que el resto de los endpoints de proceso).
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(process_id): Path<Uuid>,
    // Nota: la auth se verifica ANTES del upgrade para poder devolver 401 HTTP.
    // axum::extract::ws no permite errores HTTP post-upgrade.
    claims: crate::auth::Claims,
) -> impl IntoResponse {
    // Verificar rol
    let role_ok = sqlx::query_scalar!(
        "SELECT role FROM process_collaborators WHERE process_id = $1 AND user_id = $2",
        process_id,
        claims.sub,
    )
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .map(|r| matches!(r.as_str(), "viewer" | "operator" | "admin"))
    .unwrap_or(false);

    if !role_ok {
        // No se puede devolver 403 tras upgrade — rechazar antes
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let state_clone = state.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, state_clone, process_id, claims.sub))
        .into_response()
}

async fn handle_socket(socket: WebSocket, state: AppState, process_id: Uuid, user_id: Uuid) {
    let (mut sink, mut stream) = socket.split();

    // Suscribirse al broadcast de este proceso
    let tx = state.ws.sender(process_id).await;
    let mut rx = tx.subscribe();

    info!("WS conectado: proceso={process_id} user={user_id}");

    // ── Lanzar relay MQTT si el proceso tiene broker configurado ──────────
    {
        let cfg_row = sqlx::query!("SELECT config FROM processes WHERE id = $1", process_id)
            .fetch_optional(&state.pool)
            .await;

        if let Ok(Some(row)) = cfg_row {
            // Extraer broker MQTT directamente del JSON para no depender de
            // que ProcessConfig deserialice exactamente (campos extra → error silencioso).
            let cfg = &row.config;
            let broker_url = cfg
                .pointer("/shared_connections/mqtt/broker_url")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            if let Some(broker_url) = broker_url {
                let username = cfg
                    .pointer("/shared_connections/mqtt/username")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let password = cfg
                    .pointer("/shared_connections/mqtt/password")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);

                let relay_client_id = format!("agrodash-ws-relay-{process_id}");
                let ack_topic =
                    std::env::var("MQTT_ACK_TOPIC").unwrap_or_else(|_| "ack/valvula".to_string());

                info!("WS: lanzando relay MQTT → {broker_url} topic={ack_topic}");
                state
                    .ws
                    .ensure_mqtt_relay(
                        process_id,
                        broker_url,
                        relay_client_id,
                        username,
                        password,
                        ack_topic,
                    )
                    .await;
            } else {
                info!("WS: proceso sin shared_connections.mqtt — relay no lanzado");
            }
        }
    }

    // Enviar estado inicial del proceso al conectar
    {
        // Pipeline states
        let rows = sqlx::query!(
            "SELECT pipeline_id, state FROM process_pipeline_states WHERE process_id = $1",
            process_id
        )
        .fetch_all(&state.pool)
        .await;

        if let Ok(rows) = rows {
            for row in rows {
                let evt = WsEvent::PipelineState {
                    pipeline_id: row.pipeline_id,
                    state: row.state,
                };
                if let Ok(json) = serde_json::to_string(&evt) {
                    sink.send(Message::Text(json)).await.ok();
                }
            }
        }

        // Status del proceso
        let status_row = sqlx::query!(
            "SELECT status, last_seen_at FROM processes WHERE id = $1",
            process_id
        )
        .fetch_optional(&state.pool)
        .await;

        if let Ok(Some(row)) = status_row {
            let evt = WsEvent::ProcessStatus {
                status: row.status,
                last_seen_at: row.last_seen_at.map(|t| t.to_rfc3339()),
            };
            if let Ok(json) = serde_json::to_string(&evt) {
                sink.send(Message::Text(json)).await.ok();
            }
        }
    }

    // Loop principal: leer del broadcast Y del cliente simultáneamente
    loop {
        tokio::select! {
            // ── Broadcast → cliente ───────────────────────────────────────
            result = rx.recv() => {
                match result {
                    Ok(evt) => {
                        match serde_json::to_string(&evt) {
                            Ok(json) => {
                                if sink.send(Message::Text(json)).await.is_err() {
                                    debug!("WS sink cerrado: proceso={process_id}");
                                    break;
                                }
                            }
                            Err(e) => warn!("WS serialize error: {e}"),
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("WS cliente lagged {n} mensajes: proceso={process_id}");
                        // Enviar notificación de que se perdieron mensajes
                        let msg = json!({ "type": "error", "message": format!("Se perdieron {n} mensajes por conexión lenta") });
                        sink.send(Message::Text(msg.to_string())).await.ok();
                        // Continuar — no desconectar por lag
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // No hay más senders — proceso terminado
                        break;
                    }
                }
            }

            // ── Cliente → API ─────────────────────────────────────────────
            result = stream.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        handle_client_msg(&state, process_id, user_id, &text, &mut sink).await;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        sink.send(Message::Pong(data)).await.ok();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("WS cliente cerró: proceso={process_id}");
                        break;
                    }
                    Some(Err(e)) => {
                        debug!("WS error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("WS desconectado: proceso={process_id} user={user_id}");

    // Si no quedan clientes WS para este proceso, detener el relay MQTT
    let tx = state.ws.sender(process_id).await;
    if tx.receiver_count() == 0 {
        state.ws.stop_mqtt_relay(process_id).await;
    }
}

/// Procesa un mensaje JSON del cliente.
async fn handle_client_msg(
    state: &AppState,
    process_id: Uuid,
    user_id: Uuid,
    text: &str,
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) {
    let msg: ClientMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(_) => {
            let err = json!({ "type": "error", "message": "JSON inválido" });
            sink.send(Message::Text(err.to_string())).await.ok();
            return;
        }
    };

    match msg.msg_type.as_str() {
        "ping" => {
            sink.send(Message::Text(json!({ "type": "pong" }).to_string()))
                .await
                .ok();
        }

        "command" => {
            // Verificar que el usuario tiene rol operator/admin para comandos
            let role = sqlx::query_scalar!(
                "SELECT role FROM process_collaborators WHERE process_id = $1 AND user_id = $2",
                process_id,
                user_id,
            )
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

            if !matches!(role.as_str(), "operator" | "admin") {
                let err = json!({ "type": "error", "message": "Sin permiso para enviar comandos" });
                sink.send(Message::Text(err.to_string())).await.ok();
                return;
            }

            // Reusar la lógica de send_command: construir el payload completo
            // y llamar al mismo traductor cmd→AgentCommand.
            // Re-usamos la función de traducción de processes.rs exponiéndola como pub(crate).
            let pipeline_id = msg
                .payload
                .get("pipeline_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if pipeline_id.is_empty() {
                let err = json!({ "type": "error", "message": "Falta pipeline_id" });
                sink.send(Message::Text(err.to_string())).await.ok();
                return;
            }

            let key = crate::agent_manager::AgentKey {
                process_id,
                pipeline_id,
            };

            // Traducir payload → AgentCommand usando el mismo código que el endpoint REST
            match crate::routes::processes::parse_agent_command(&msg.payload) {
                Ok(agent_cmd) => {
                    if let Err(e) = state.manager.notify(&key, agent_cmd).await {
                        let err = json!({ "type": "error", "message": format!("Error enviando comando: {e}") });
                        sink.send(Message::Text(err.to_string())).await.ok();
                    }
                    // El ACK llegará por el broadcast cuando el agente postee su estado
                }
                Err(e) => {
                    let err = json!({ "type": "error", "message": e });
                    sink.send(Message::Text(err.to_string())).await.ok();
                }
            }
        }

        _ => {
            let err = json!({ "type": "error", "message": format!("Tipo de mensaje desconocido: {}", msg.msg_type) });
            sink.send(Message::Text(err.to_string())).await.ok();
        }
    }
}
