// api/src/tasks/mqtt_relay.rs
//
// Relay de ACKs MQTT → WebSocket.
//
// Cuando un cliente WebSocket se conecta a un proceso que tiene configuración
// MQTT, este task se lanza en background para suscribir el topic ack/valvula
// en el broker del proceso y forwarding cada mensaje al broadcast del hub WS.
//
// El task se cancela automáticamente cuando el `shutdown_rx` recibe señal,
// o cuando ya no hay receivers en el broadcast (todos los clientes WS se
// desconectaron).

use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::ws::{WsBroadcast, WsEvent};

/// Conectar al broker MQTT del proceso y relay de ack/valvula → WS.
///
/// # Argumentos
/// - `process_id`: para identificar el canal de broadcast correcto
/// - `broker_url`: "mqtt://host:port" o "host:port"
/// - `client_id`: client_id MQTT único para esta conexión relay
/// - `username` / `password`: credenciales opcionales
/// - `ack_topic`: topic a suscribir (normalmente "ack/valvula")
/// - `ws`: handle al broadcast hub
/// - `shutdown_rx`: señal para detener el relay (enviada desde ws.rs cuando
///   el último cliente WS se desconecta)
pub async fn run(
    process_id: Uuid,
    broker_url: String,
    client_id: String,
    username: Option<String>,
    password: Option<String>,
    ack_topic: String,
    ws: WsBroadcast,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    // Parsear broker_url: acepta "mqtt://host:port" o "host:port"
    let addr = broker_url
        .trim_start_matches("mqtt://")
        .trim_start_matches("mqtts://");

    let (host, port) = match addr.split_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(1883)),
        None => (addr.to_string(), 1883),
    };

    let mut options = MqttOptions::new(client_id, host, port);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_clean_session(true);

    if let (Some(u), Some(p)) = (username, password) {
        options.set_credentials(u, p);
    }

    let (client, mut event_loop) = AsyncClient::new(options, 32);

    // Suscribir al topic de ACKs
    if let Err(e) = client.subscribe(&ack_topic, QoS::AtMostOnce).await {
        warn!("mqtt_relay/{process_id}: no se pudo suscribir a {ack_topic}: {e}");
        return;
    }

    info!("mqtt_relay/{process_id}: suscrito a {ack_topic}");

    loop {
        tokio::select! {
            // ── Shutdown signal ───────────────────────────────────────────
            _ = &mut shutdown_rx => {
                debug!("mqtt_relay/{process_id}: shutdown recibido");
                client.disconnect().await.ok();
                break;
            }

            // ── Mensaje MQTT ──────────────────────────────────────────────
            event = event_loop.poll() => {
                match event {
                    Ok(Event::Incoming(Packet::Publish(msg))) => {
                        let payload = match std::str::from_utf8(&msg.payload) {
                            Ok(s) => s.to_string(),
                            Err(_) => continue,
                        };

                        debug!("mqtt_relay/{process_id}: ack recibido: {payload}");

                        // Parsear el actuator_id del mensaje si es posible.
                        // Formato: "CONCENTRADOR:Relay1,Activo,Bomba,Activa"
                        //          "MQTT_RECIBIDO:ON,1"
                        // Extraer el número de válvula/relay del payload.
                        let actuator_id = extract_actuator_id(&payload);

                        let receivers = {
                            // Verificar si hay clientes WS activos
                            // Si no hay nadie escuchando, detener el relay
                            let tx = ws.sender(process_id).await;
                            tx.receiver_count()
                        };

                        if receivers == 0 {
                            debug!("mqtt_relay/{process_id}: sin clientes WS, deteniendo relay");
                            client.disconnect().await.ok();
                            break;
                        }

                        ws.publish(
                            process_id,
                            WsEvent::MqttAck {
                                pipeline_id: String::new(), // se completará en el frontend
                                actuator_id,
                                msg: payload,
                            },
                        ).await;
                    }

                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        info!("mqtt_relay/{process_id}: MQTT conectado");
                    }

                    Err(e) => {
                        warn!("mqtt_relay/{process_id}: error MQTT: {e}");
                        // Esperar antes de reintentar
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        // Re-suscribir
                        client.subscribe(&ack_topic, QoS::AtMostOnce).await.ok();
                    }

                    _ => {}
                }
            }
        }
    }

    info!("mqtt_relay/{process_id}: relay terminado");
}

/// Extrae el identificador del actuador del mensaje de ACK.
/// Ejemplos:
///   "MQTT_RECIBIDO:ON,1"        → "1"
///   "LORA_ENVIANDO:OFF,3"       → "3"
///   "CONCENTRADOR:Relay5,Activo,Bomba,Activa" → "5"
fn extract_actuator_id(msg: &str) -> String {
    // Caso CONCENTRADOR: buscar RelayN
    if let Some(rest) = msg.strip_prefix("CONCENTRADOR:") {
        if let Some(relay_part) = rest.split(',').next() {
            if let Some(n) = relay_part.strip_prefix("Relay") {
                return n.to_string();
            }
            if relay_part == "RelaysInactivos" {
                return "ALL".to_string();
            }
        }
    }

    // Otros casos: ON,N u OFF,N al final
    if let Some(colon_idx) = msg.find(':') {
        let after_colon = &msg[colon_idx + 1..];
        if let Some(comma_idx) = after_colon.find(',') {
            return after_colon[comma_idx + 1..].to_string();
        }
    }

    String::new()
}
