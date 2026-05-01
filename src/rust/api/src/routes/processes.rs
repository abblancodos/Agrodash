// api/src/routes/processes.rs

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{convert::Infallible, time::{Duration, Instant}};
use tokio::time::interval;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use agrodash_shared::{
    ProcessConfig, CheckResult, CheckStatus, SelfTestReport,
};

use crate::AppState;
use crate::auth::Claims;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn err(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
}

fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "Sin permisos"})))
}

fn pool(state: &AppState) -> &PgPool { &state.pool }

// ── Rol helpers ───────────────────────────────────────────────────────────────

async fn get_process_role(
    pool: &PgPool,
    process_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            CASE
                WHEN p.owner_id = $2 THEN 'admin'
                WHEN c.role IS NOT NULL THEN c.role
                ELSE NULL
            END AS role
        FROM processes p
        LEFT JOIN process_collaborators c
            ON c.process_id = p.id AND c.user_id = $2
        WHERE p.id = $1
        "#,
        process_id, user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.role))
}

async fn require_process_role(
    pool: &PgPool,
    process_id: Uuid,
    user_id: Uuid,
    min_role: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let role = get_process_role(pool, process_id, user_id)
        .await
        .map_err(err)?
        .ok_or_else(forbidden)?;
    let level = |r: &str| match r { "admin" => 3, "operator" => 2, "viewer" => 1, _ => 0 };
    if level(&role) < level(min_role) { return Err(forbidden()); }
    Ok(role)
}

// ── HTTP proxy helpers (hacia control externo, legacy) ─────────────────────────

async fn control_get(url: &str, api_key: &str, path: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build()
        .map_err(|e| e.to_string())?;
    let res = client.get(format!("{}{}", url.trim_end_matches('/'), path))
        .header("X-API-Key", api_key).send().await.map_err(|e| e.to_string())?;
    res.json::<Value>().await.map_err(|e| e.to_string())
}

async fn control_post(url: &str, api_key: &str, path: &str, body: Value) -> Result<Value, String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build()
        .map_err(|e| e.to_string())?;
    let res = client.post(format!("{}{}", url.trim_end_matches('/'), path))
        .header("X-API-Key", api_key).json(&body).send().await.map_err(|e| e.to_string())?;
    res.json::<Value>().await.map_err(|e| e.to_string())
}

// ── Structs ───────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ProcessLogRow {
    pub id:         Uuid,
    pub process_id: Uuid,
    pub ts:         chrono::DateTime<chrono::Utc>,
    pub level:      String,
    pub source:     String,
    pub message:    String,
    pub data:       Option<Value>,
    pub user_id:    Option<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ValveEventRow {
    pub id:                Uuid,
    pub process_id:        Uuid,
    pub ts:                chrono::DateTime<chrono::Utc>,
    pub linea:             String,
    pub estado:            bool,
    pub modo:              String,
    pub triggered_by:      Option<Uuid>,
    pub kalman_convergido: Option<bool>,
    pub kalman_x_hat:      Option<Value>,
    pub context:           Option<Value>,
}

#[derive(Deserialize)]
pub struct CreateProcessRequest {
    pub name:        String,
    pub description: Option<String>,
    pub r#type:      Option<String>,
    pub control_url: String,
    pub api_key:     String,
    pub config:      Option<Value>,
}

#[derive(Deserialize)]
pub struct UpdateProcessRequest {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub config:      Option<Value>,
    pub status:      Option<String>,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub limit:  Option<i64>,
    pub since:  Option<chrono::DateTime<chrono::Utc>>,
    pub level:  Option<String>,
    pub source: Option<String>,
}

#[derive(Deserialize)]
pub struct ValveEventsQuery {
    pub limit: Option<i64>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub linea: Option<String>,
}

#[derive(Deserialize)]
pub struct ReadingsQuery {
    pub limit:       Option<i64>,
    pub since:       Option<chrono::DateTime<chrono::Utc>>,
    pub until:       Option<chrono::DateTime<chrono::Utc>>,
    pub pipeline_id: Option<String>,
}

#[derive(Deserialize)]
pub struct StreamQuery {
    pub interval_secs: Option<u64>,
}

#[derive(Deserialize)]
pub struct AddCollabRequest {
    pub user_id: Uuid,
    pub role:    String,
}

// ── GET /processes ────────────────────────────────────────────────────────────

pub async fn list_processes(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<Vec<Value>>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query!(
        r#"
        SELECT
            p.id, p.name, p.description, p.type, p.status,
            p.control_url, p.config, p.last_state, p.last_seen_at,
            p.owner_id, p.created_at,
            CASE
                WHEN p.owner_id = $1 THEN 'admin'
                WHEN c.role IS NOT NULL THEN c.role
                ELSE NULL
            END AS user_role
        FROM processes p
        LEFT JOIN process_collaborators c
            ON c.process_id = p.id AND c.user_id = $1
        WHERE p.owner_id = $1 OR c.user_id = $1
        ORDER BY p.created_at DESC
        "#,
        claims.sub
    )
    .fetch_all(pool(&state))
    .await
    .map_err(err)?;

    let result: Vec<Value> = rows.iter().map(|r| json!({
        "id":          r.id,
        "name":        r.name,
        "description": r.description,
        "type":        r.r#type,
        "status":      r.status,
        "control_url": r.control_url,
        "config":      r.config,
        "last_seen_at": r.last_seen_at,
        "owner_id":    r.owner_id,
        "created_at":  r.created_at,
        "user_role":   r.user_role,
    })).collect();

    Ok(Json(result))
}

// ── POST /processes ───────────────────────────────────────────────────────────

pub async fn create_process(
    State(state): State<AppState>,
    claims: Claims,
    Json(body): Json<CreateProcessRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = Uuid::new_v4();
    let config = body.config.unwrap_or(json!({}));
    let proc_type = body.r#type.unwrap_or_else(|| "irrigation_kalman".to_string());

    sqlx::query!(
        r#"INSERT INTO processes (id, name, description, type, control_url, api_key, config, owner_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        id, body.name, body.description, proc_type, body.control_url, body.api_key, config, claims.sub
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    sqlx::query!(
        "INSERT INTO process_logs (process_id, source, message) VALUES ($1, 'system', 'Proceso creado')",
        id
    )
    .execute(pool(&state))
    .await
    .ok();

    Ok(Json(json!({ "id": id, "type": proc_type, "name": body.name })))
}

// ── GET /processes/:id ────────────────────────────────────────────────────────

pub async fn get_process(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let role = require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        r#"SELECT id, name, description, type, status, control_url,
                  config, last_state, last_seen_at, owner_id, created_at
           FROM processes WHERE id = $1"#,
        process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Proceso no encontrado"}))))?;

    // Contar agentes activos en el manager
    let active_agents = state.manager.active_count(process_id).await;

    Ok(Json(json!({
        "id":           row.id,
        "name":         row.name,
        "description":  row.description,
        "type":         row.r#type,
        "status":       row.status,
        "control_url":  row.control_url,
        "config":       row.config,
        "last_state":   row.last_state,
        "last_seen_at": row.last_seen_at,
        "owner_id":     row.owner_id,
        "created_at":   row.created_at,
        "user_role":    role,
        "active_agents": active_agents,
    })))
}

// ── POST /processes/:id/start ─────────────────────────────────────────────────

pub async fn start_process(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "operator").await?;

    let row = sqlx::query!(
        "SELECT config, status FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    if row.status == "running" {
        return Err((StatusCode::CONFLICT, Json(json!({"error": "El proceso ya está corriendo"}))));
    }

    let cfg = serde_json::from_value::<ProcessConfig>(row.config)
        .map_err(|e| err(format!("Config inválida: {e}")))?;

    // Marcar en DB primero
    sqlx::query!(
        "UPDATE processes SET status='running', updated_at=now() WHERE id=$1",
        process_id
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    // Spawnear un agente por pipeline
    for pipeline in &cfg.pipelines {
        state.manager.spawn(process_id, pipeline.id.clone()).await;
    }

    sqlx::query!(
        "INSERT INTO process_logs (process_id, source, message, user_id) VALUES ($1,'system','Proceso iniciado',$2)",
        process_id, claims.sub
    )
    .execute(pool(&state))
    .await
    .ok();

    Ok(StatusCode::NO_CONTENT)
}

// ── POST /processes/:id/stop ──────────────────────────────────────────────────

pub async fn stop_process(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "operator").await?;

    // Detener agentes (remover del mapa + kill)
    state.manager.stop_process(process_id).await;

    sqlx::query!(
        "UPDATE processes SET status='stopped', updated_at=now() WHERE id=$1",
        process_id
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    sqlx::query!(
        "INSERT INTO process_logs (process_id, source, message, user_id) VALUES ($1,'system','Proceso detenido',$2)",
        process_id, claims.sub
    )
    .execute(pool(&state))
    .await
    .ok();

    Ok(StatusCode::NO_CONTENT)
}

// ── POST /processes/:id/test — Self-test de infraestructura ───────────────────

#[derive(Deserialize)]
pub struct SelfTestQuery {
    /// true = solo verifica agentes ya corriendo (health check)
    pub health_only: Option<bool>,
}

pub async fn run_self_test(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<SelfTestQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "operator").await?;

    let t_start = Instant::now();
    let mut checks: Vec<CheckResult> = Vec::new();

    if q.health_only.unwrap_or(false) {
        // ── Modo health: verificar agentes activos ────────────────────────
        let active = state.manager.active_count(process_id).await;

        // Ping via socket a cada agente
        let row = sqlx::query!(
            "SELECT config FROM processes WHERE id = $1", process_id
        )
        .fetch_optional(pool(&state))
        .await
        .map_err(err)?;

        if let Some(row) = row {
            if let Ok(cfg) = serde_json::from_value::<ProcessConfig>(row.config) {
                for pipeline in &cfg.pipelines {
                    let key = crate::agent_manager::AgentKey {
                        process_id,
                        pipeline_id: pipeline.id.clone(),
                    };
                    let t0 = Instant::now();
                    let resp = state.manager.socket_cmd(
                        &key,
                        agrodash_shared::AgentCommand::GetState,
                    ).await;
                    let lat = t0.elapsed().as_millis() as u64;

                    checks.push(match resp {
                        Some(r) if r.ok => CheckResult {
                            name:       format!("agent:{}", pipeline.id),
                            status:     CheckStatus::Ok,
                            detail:     format!("respondió en {}ms", lat),
                            latency_ms: Some(lat),
                        },
                        Some(r) => CheckResult {
                            name:       format!("agent:{}", pipeline.id),
                            status:     CheckStatus::Error,
                            detail:     r.error.unwrap_or_else(|| "error desconocido".into()),
                            latency_ms: Some(lat),
                        },
                        None => CheckResult {
                            name:   format!("agent:{}", pipeline.id),
                            status: CheckStatus::Error,
                            detail: "socket no responde".into(),
                            latency_ms: None,
                        },
                    });
                }
            }
        }

        let report = SelfTestReport::new(checks, t_start.elapsed().as_millis() as u64);
        return Ok(Json(serde_json::to_value(&report).unwrap_or_default()));
    }

    // ── Modo infra: chequeo completo ──────────────────────────────────────

    let row = sqlx::query!(
        "SELECT config FROM processes WHERE id = $1", process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    let cfg = serde_json::from_value::<ProcessConfig>(row.config)
        .map_err(|e| err(format!("Config inválida: {e}")))?;

    // 1. Check DB — lecturas recientes por sensor
    for pipeline in &cfg.pipelines {
        for node in &pipeline.nodes {
            if let agrodash_shared::NodeKind::PostgresSensor(sensor_cfg) = &node.kind {
                for sensor in &sensor_cfg.sensors {
                    let t0 = Instant::now();
                    let sensor_id: Result<uuid::Uuid, _> = sensor.id.parse();
                    let check = match sensor_id {
                        Err(_) => CheckResult {
                            name:       format!("sensor:{}", sensor.label),
                            status:     CheckStatus::Error,
                            detail:     format!("ID inválido: '{}'", sensor.id),
                            latency_ms: None,
                        },
                        Ok(sid) => {
                            let result = sqlx::query!(
                                r#"SELECT value::float8 AS "v!", created_at
                                   FROM readings WHERE sensor_id = $1
                                   ORDER BY created_at DESC LIMIT 1"#,
                                sid
                            )
                            .fetch_optional(pool(&state))
                            .await;
                            let lat = t0.elapsed().as_millis() as u64;
                            match result {
                                Ok(Some(row)) => {
                                    let age_secs = chrono::Utc::now()
                                        .signed_duration_since(row.created_at)
                                        .num_seconds();
                                    let (status, detail) = if age_secs > 600 {
                                        (CheckStatus::Warn,
                                         format!("último dato hace {}min (valor: {:.3})", age_secs/60, row.v))
                                    } else {
                                        (CheckStatus::Ok,
                                         format!("ok — hace {}s, valor: {:.3}", age_secs, row.v))
                                    };
                                    CheckResult { name: format!("sensor:{}", sensor.label), status, detail, latency_ms: Some(lat) }
                                }
                                Ok(None) => CheckResult {
                                    name: format!("sensor:{}", sensor.label),
                                    status: CheckStatus::Error,
                                    detail: "sin datos en la tabla readings".into(),
                                    latency_ms: Some(lat),
                                },
                                Err(e) => CheckResult {
                                    name: format!("sensor:{}", sensor.label),
                                    status: CheckStatus::Error,
                                    detail: format!("DB error: {e}"),
                                    latency_ms: Some(lat),
                                },
                            }
                        }
                    };
                    checks.push(check);
                }
            }
        }
    }

    // 2. Check MQTT — intentar conectar al broker
    let mqtt_conn = cfg.shared_connections.as_ref().and_then(|sc| sc.mqtt.as_ref());
    if let Some(mqtt) = mqtt_conn {
        let t0 = Instant::now();
        let check = check_mqtt_broker(mqtt).await;
        let lat = t0.elapsed().as_millis() as u64;
        checks.push(CheckResult { latency_ms: Some(lat), ..check });
    } else {
        // Buscar en pipelines individuales
        let mut found = false;
        for pipeline in &cfg.pipelines {
            if let Some(conn) = &pipeline.connections {
                if let Some(mqtt) = &conn.mqtt {
                    let t0 = Instant::now();
                    let check = check_mqtt_broker(mqtt).await;
                    let lat = t0.elapsed().as_millis() as u64;
                    checks.push(CheckResult {
                        name: format!("mqtt:{}", pipeline.id),
                        latency_ms: Some(lat),
                        ..check
                    });
                    found = true;
                }
            }
        }
        if !found {
            checks.push(CheckResult {
                name:       "mqtt".into(),
                status:     CheckStatus::Warn,
                detail:     "No hay conexión MQTT configurada".into(),
                latency_ms: None,
            });
        }
    }

    // 3. Check HTTP actuators
    for pipeline in &cfg.pipelines {
        for node in &pipeline.nodes {
            if let agrodash_shared::NodeKind::HttpActuator(http_cfg) = &node.kind {
                let base_url = resolve_http_base_url(
                    &http_cfg.connection,
                    pipeline.connections.as_ref(),
                    cfg.shared_connections.as_ref(),
                );
                let t0 = Instant::now();
                let check = match base_url {
                    None => CheckResult {
                        name:       format!("http:{}", node.id),
                        status:     CheckStatus::Error,
                        detail:     "No se pudo resolver la conexión HTTP".into(),
                        latency_ms: None,
                    },
                    Some(url) => {
                        let result = reqwest::Client::builder()
                            .timeout(Duration::from_secs(5))
                            .build()
                            .unwrap_or_default()
                            .get(&url)
                            .send()
                            .await;
                        let lat = t0.elapsed().as_millis() as u64;
                        match result {
                            Ok(r) => CheckResult {
                                name:       format!("http:{}", node.id),
                                status:     if r.status().is_success() || r.status().as_u16() == 404 {
                                    CheckStatus::Ok  // 404 = servidor responde, ruta no existe (ok para health)
                                } else {
                                    CheckStatus::Warn
                                },
                                detail:     format!("HTTP {} en {}ms desde {}", r.status(), lat, url),
                                latency_ms: Some(lat),
                            },
                            Err(e) => CheckResult {
                                name:       format!("http:{}", node.id),
                                status:     CheckStatus::Error,
                                detail:     format!("{e}"),
                                latency_ms: Some(lat),
                            },
                        }
                    }
                };
                checks.push(check);
            }
        }
    }

    let report = SelfTestReport::new(checks, t_start.elapsed().as_millis() as u64);

    // Loguear en process_logs
    let overall_str = format!("{:?}", report.overall).to_lowercase();
    sqlx::query!(
        r#"INSERT INTO process_logs (process_id, level, source, message, user_id)
           VALUES ($1, $2, 'self_test', 'Self-test ejecutado', $3)"#,
        process_id,
        overall_str,
        claims.sub,
    )
    .execute(pool(&state))
    .await
    .ok();

    Ok(Json(serde_json::to_value(&report).unwrap_or_default()))
}

// ── Helpers del self-test ─────────────────────────────────────────────────────

async fn check_mqtt_broker(mqtt: &agrodash_shared::MqttConnection) -> CheckResult {
    use rumqttc::{MqttOptions, AsyncClient};

    let url = mqtt.broker_url.trim_start_matches("mqtt://");
    let (host, port) = url.split_once(':')
        .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
        .unwrap_or((url.to_string(), 1883));

    let client_id = format!("agrodash-test-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    let mut opts = MqttOptions::new(client_id, host.clone(), port);
    opts.set_keep_alive(Duration::from_secs(5));
    opts.set_connection_timeout(3);
    if let (Some(u), Some(p)) = (mqtt.username.clone(), mqtt.password.clone()) {
        opts.set_credentials(u, p);
    }

    let (_client, mut eventloop) = AsyncClient::new(opts, 4);

    // Intentar el primer poll (debería ser ConnAck)
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        eventloop.poll(),
    ).await;

    match result {
        Ok(Ok(_)) => CheckResult {
            name:       "mqtt:broker".into(),
            status:     CheckStatus::Ok,
            detail:     format!("conectado a {}:{}", host, port),
            latency_ms: None,
        },
        Ok(Err(e)) => CheckResult {
            name:   "mqtt:broker".into(),
            status: CheckStatus::Error,
            detail: format!("{}:{} — {e}", host, port),
            latency_ms: None,
        },
        Err(_) => CheckResult {
            name:   "mqtt:broker".into(),
            status: CheckStatus::Error,
            detail: format!("timeout conectando a {}:{}", host, port),
            latency_ms: None,
        },
    }
}

fn resolve_http_base_url(
    conn_ref:             &agrodash_shared::ConnectionRef,
    pipeline_connections: Option<&agrodash_shared::SharedConnections>,
    shared_connections:   Option<&agrodash_shared::SharedConnections>,
) -> Option<String> {
    match conn_ref {
        agrodash_shared::ConnectionRef::Inline(inline) => {
            inline.http.as_ref().map(|h| h.base_url.clone())
        }
        agrodash_shared::ConnectionRef::Named(_) => {
            pipeline_connections
                .and_then(|c| c.http.as_ref())
                .or_else(|| shared_connections.and_then(|c| c.http.as_ref()))
                .map(|h| h.base_url.clone())
        }
    }
}

// ── Resto de handlers (sin cambios de lógica, solo AppState) ──────────────────

pub async fn send_command(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "operator").await?;

    let row = sqlx::query!(
        "SELECT control_url, api_key FROM processes WHERE id = $1", process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Proceso no encontrado"}))))?;

    body["_user_id"] = json!(claims.sub.to_string());

    let result = control_post(&row.control_url, &row.api_key, "/command", body.clone())
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    let cmd = body.get("cmd").and_then(|v| v.as_str()).unwrap_or("unknown");
    sqlx::query!(
        r#"INSERT INTO process_logs (process_id, level, source, message, data, user_id)
           VALUES ($1, 'info', 'user', $2, $3, $4)"#,
        process_id, format!("cmd:{cmd}"), body, claims.sub,
    )
    .execute(pool(&state))
    .await
    .ok();

    Ok(Json(result))
}

pub async fn get_state(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        "SELECT status, last_state, last_seen_at FROM processes WHERE id = $1", process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    Ok(Json(json!({
        "status":      row.status,
        "last_state":  row.last_state,
        "last_seen_at": row.last_seen_at,
    })))
}

pub async fn stream_state(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let secs = q.interval_secs.unwrap_or(5).max(2).min(60);
    let pool = state.pool.clone();

    let stream = stream::unfold(
        (pool, process_id, None::<String>),
        move |(pool, pid, last_seen)| async move {
            let mut ticker = interval(Duration::from_secs(secs));
            ticker.tick().await;

            let row = sqlx::query!(
                r#"
                SELECT status, last_state, last_seen_at,
                       (
                           SELECT json_agg(json_build_object(
                               'ts', l.ts, 'level', l.level,
                               'source', l.source, 'message', l.message
                           ) ORDER BY l.ts)
                           FROM process_logs l
                           WHERE l.process_id = $1
                             AND ($2::text IS NULL OR l.ts > $2::timestamptz)
                           LIMIT 20
                       ) AS new_logs
                FROM processes WHERE id = $1
                "#,
                pid,
                last_seen,
            )
            .fetch_optional(&pool)
            .await;

            match row {
                Ok(Some(r)) => {
                    let new_last = r.last_seen_at
                        .map(|t| t.to_rfc3339())
                        .or(last_seen.clone());

                    let payload = json!({
                        "status":      r.status,
                        "last_state":  r.last_state,
                        "last_seen_at": r.last_seen_at,
                        "new_logs":    r.new_logs,
                    });

                    let event = Event::default()
                        .data(payload.to_string())
                        .event("state");

                    Some((Ok(event), (pool, pid, new_last)))
                }
                _ => None,
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

pub async fn get_logs(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let limit = q.limit.unwrap_or(100).min(500);
    let rows = sqlx::query_as!(
        ProcessLogRow,
        r#"SELECT id, process_id, ts, level, source, message, data, user_id
           FROM process_logs
           WHERE process_id = $1
             AND ($2::timestamptz IS NULL OR ts > $2)
             AND ($3::text IS NULL OR level = $3)
             AND ($4::text IS NULL OR source = $4)
           ORDER BY ts DESC LIMIT $5"#,
        process_id, q.since, q.level, q.source, limit,
    )
    .fetch_all(pool(&state))
    .await
    .map_err(err)?;

    Ok(Json(json!({ "logs": rows })))
}

pub async fn get_valve_events(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<ValveEventsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let limit = q.limit.unwrap_or(100).min(500);
    let rows = sqlx::query_as!(
        ValveEventRow,
        r#"SELECT id, process_id, ts, linea, estado, modo,
                  triggered_by, kalman_convergido, kalman_x_hat, context
           FROM process_valve_events
           WHERE process_id = $1
             AND ($2::timestamptz IS NULL OR ts > $2)
             AND ($3::text IS NULL OR linea = $3)
           ORDER BY ts DESC LIMIT $4"#,
        process_id, q.since, q.linea, limit,
    )
    .fetch_all(pool(&state))
    .await
    .map_err(err)?;

    Ok(Json(json!({ "events": rows })))
}

pub async fn tail_control_log(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        "SELECT control_url, api_key FROM processes WHERE id = $1", process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    let result = control_get(&row.control_url, &row.api_key, "/logs/tail?n=150")
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    Ok(Json(result))
}

pub async fn add_collaborator(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(body): Json<AddCollabRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "admin").await?;

    if !["viewer", "operator", "admin"].contains(&body.role.as_str()) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Rol inválido"}))));
    }

    sqlx::query!(
        r#"INSERT INTO process_collaborators (process_id, user_id, role) VALUES ($1, $2, $3)
           ON CONFLICT (process_id, user_id) DO UPDATE SET role = $3"#,
        process_id, body.user_id, body.role,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_collaborator(
    State(state): State<AppState>,
    claims: Claims,
    Path((process_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "admin").await?;

    sqlx::query!(
        "DELETE FROM process_collaborators WHERE process_id = $1 AND user_id = $2",
        process_id, user_id,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_collaborators(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let rows = sqlx::query!(
        r#"SELECT c.user_id, c.role, c.added_at, u.email
           FROM process_collaborators c
           JOIN users u ON u.id = c.user_id
           WHERE c.process_id = $1 ORDER BY c.added_at"#,
        process_id,
    )
    .fetch_all(pool(&state))
    .await
    .map_err(err)?;

    let collabs: Vec<Value> = rows.iter().map(|r| json!({
        "user_id":  r.user_id,
        "role":     r.role,
        "email":    r.email,
        "added_at": r.added_at,
    })).collect();

    Ok(Json(json!({ "collaborators": collabs })))
}

// ── Rutas internas para el agente ─────────────────────────────────────────────

pub async fn get_config(
    State(state): State<AppState>,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query!(
        "SELECT config FROM processes WHERE id = $1", process_id
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    Ok(Json(row.config))
}

pub async fn get_agent_state(
    State(state): State<AppState>,
    Path((process_id, pipeline_id)): Path<(Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query!(
        r#"SELECT state FROM process_pipeline_states
           WHERE process_id = $1 AND pipeline_id = $2"#,
        process_id, pipeline_id,
    )
    .fetch_optional(pool(&state))
    .await
    .map_err(err)?;

    Ok(Json(row.map(|r| r.state).unwrap_or(json!({}))))
}

pub async fn post_agent_state(
    State(state): State<AppState>,
    Path((process_id, pipeline_id)): Path<(Uuid, String)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    sqlx::query!(
        r#"INSERT INTO process_pipeline_states (process_id, pipeline_id, state, updated_at)
           VALUES ($1, $2, $3, now())
           ON CONFLICT (process_id, pipeline_id)
           DO UPDATE SET state = $3, updated_at = now()"#,
        process_id, pipeline_id, body,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    sqlx::query!(
        "UPDATE processes SET last_seen_at = now(), status = 'running', updated_at = now() WHERE id = $1",
        process_id,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn post_agent_error(
    State(state): State<AppState>,
    Path(process_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let msg = body.get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("error desconocido");

    sqlx::query!(
        "INSERT INTO process_logs (process_id, level, source, message) VALUES ($1, 'error', 'worker', $2)",
        process_id, msg,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    sqlx::query!(
        "UPDATE processes SET status = 'error', updated_at = now() WHERE id = $1",
        process_id,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_process(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(body): Json<UpdateProcessRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "admin").await?;

    sqlx::query!(
        r#"UPDATE processes SET
               name        = COALESCE($1, name),
               description = COALESCE($2, description),
               config      = COALESCE($3, config),
               status      = COALESCE($4, status),
               updated_at  = now()
           WHERE id = $5"#,
        body.name, body.description, body.config, body.status, process_id,
    )
    .execute(pool(&state))
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_readings(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<ReadingsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(pool(&state), process_id, claims.sub, "viewer").await?;

    let limit = q.limit.unwrap_or(500).min(2000);

    let rows = sqlx::query!(
        r#"SELECT id, pipeline_id, ts, raw, filtered, p_diag, decision, actuator
           FROM process_readings
           WHERE process_id = $1
             AND ($2::text IS NULL OR pipeline_id = $2)
             AND ($3::timestamptz IS NULL OR ts >= $3)
             AND ($4::timestamptz IS NULL OR ts <= $4)
           ORDER BY ts DESC LIMIT $5"#,
        process_id, q.pipeline_id, q.since, q.until, limit,
    )
    .fetch_all(pool(&state))
    .await
    .map_err(err)?;

    let readings: Vec<Value> = rows.iter().map(|r| json!({
        "id":          r.id,
        "pipeline_id": r.pipeline_id,
        "ts":          r.ts,
        "raw":         r.raw,
        "filtered":    r.filtered,
        "p_diag":      r.p_diag,
        "decision":    r.decision,
        "actuator":    r.actuator,
    })).collect();

    Ok(Json(json!({ "readings": readings })))
}