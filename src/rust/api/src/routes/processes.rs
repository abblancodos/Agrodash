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
use std::{convert::Infallible, time::Duration};
use tokio::time::interval;
use uuid::Uuid;

use crate::auth::Claims;
use crate::AppState;
use agrodash_shared::{AgentCommand, ProcessConfig};

fn err(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
}

fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "Sin permisos"})))
}

fn not_found() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"})))
}

// ── Helpers de rol ────────────────────────────────────────────────────────────

async fn get_process_role(
    pool:       &sqlx::PgPool,
    process_id: Uuid,
    user_id:    Uuid,
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
    pool:       &sqlx::PgPool,
    process_id: Uuid,
    user_id:    Uuid,
    min_role:   &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let role = get_process_role(pool, process_id, user_id)
        .await
        .map_err(err)?
        .ok_or_else(forbidden)?;

    let level = |r: &str| match r { "admin" => 3, "operator" => 2, "viewer" => 1, _ => 0 };
    if level(&role) < level(min_role) { return Err(forbidden()); }
    Ok(role)
}

// ── HTTP client al contenedor de control (legacy) ─────────────────────────────

async fn control_get(url: &str, api_key: &str, path: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get(format!("{}{}", url.trim_end_matches('/'), path))
        .header("X-API-Key", api_key)
        .send().await.map_err(|e| e.to_string())?;
    res.json::<Value>().await.map_err(|e| e.to_string())
}

async fn control_post(url: &str, api_key: &str, path: &str, body: Value) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .post(format!("{}{}", url.trim_end_matches('/'), path))
        .header("X-API-Key", api_key)
        .json(&body)
        .send().await.map_err(|e| e.to_string())?;
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
    .fetch_all(&state.pool)
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
    let id     = Uuid::new_v4();
    let config = body.config.unwrap_or(json!({}));
    let proc_type = body.r#type.unwrap_or_else(|| "irrigation_kalman".to_string());

    sqlx::query!(
        r#"
        INSERT INTO processes
            (id, name, description, type, control_url, api_key, config, owner_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        id, body.name, body.description, proc_type,
        body.control_url, body.api_key, config, claims.sub
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    sqlx::query!(
        "INSERT INTO process_logs (process_id, source, message) VALUES ($1, 'system', 'Proceso creado')",
        id
    )
    .execute(&state.pool)
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
    let role = require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        r#"
        SELECT id, name, description, type, status, control_url,
               config, last_state, last_seen_at, owner_id, created_at
        FROM processes WHERE id = $1
        "#,
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    Ok(Json(json!({
        "id":          row.id,
        "name":        row.name,
        "description": row.description,
        "type":        row.r#type,
        "status":      row.status,
        "control_url": row.control_url,
        "config":      row.config,
        "last_state":  row.last_state,
        "last_seen_at": row.last_seen_at,
        "owner_id":    row.owner_id,
        "created_at":  row.created_at,
        "user_role":   role,
    })))
}

// ── PATCH /processes/:id ──────────────────────────────────────────────────────

pub async fn update_process(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(body): Json<UpdateProcessRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "admin").await?;

    sqlx::query!(
        r#"
        UPDATE processes SET
            name        = COALESCE($1, name),
            description = COALESCE($2, description),
            config      = COALESCE($3, config),
            status      = COALESCE($4, status),
            updated_at  = now()
        WHERE id = $5
        "#,
        body.name, body.description, body.config, body.status, process_id,
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── POST /processes/:id/start ─────────────────────────────────────────────────

pub async fn start_process(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "operator").await?;

    let row = sqlx::query!(
        "SELECT config, status FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if row.status == "running" {
        return Err((StatusCode::CONFLICT, Json(json!({"error": "El proceso ya está corriendo"}))));
    }

    let cfg: ProcessConfig = serde_json::from_value(row.config)
        .map_err(|e| err(format!("Config inválida: {e}")))?;

    // Marcar como running
    sqlx::query!(
        "UPDATE processes SET status='running', updated_at=now() WHERE id=$1",
        process_id
    )
    .execute(&state.pool)
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
    .execute(&state.pool)
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
    require_process_role(&state.pool, process_id, claims.sub, "operator").await?;

    // Detener agentes (graceful: checkpoint → kill)
    state.manager.stop_process(process_id).await;

    sqlx::query!(
        "UPDATE processes SET status='stopped', updated_at=now() WHERE id=$1",
        process_id
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    sqlx::query!(
        "INSERT INTO process_logs (process_id, source, message, user_id) VALUES ($1,'system','Proceso detenido',$2)",
        process_id, claims.sub
    )
    .execute(&state.pool)
    .await
    .ok();

    Ok(StatusCode::NO_CONTENT)
}

// ── POST /processes/:id/test — self-test de infraestructura ───────────────────
//
// Modos:
//   - Si el proceso está running: envía SelfTest a cada agente vía socket (dry-run real)
//   - Si está stopped: hace chequeos estáticos (sensores, MQTT/HTTP ping desde la API)
//
// Retorna: { overall: "ok"|"warn"|"error", checks: [...] }

pub async fn self_test(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "operator").await?;

    let row = sqlx::query!(
        "SELECT status, config FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    let cfg: ProcessConfig = serde_json::from_value(row.config.clone())
        .map_err(|e| err(format!("Config inválida: {e}")))?;

    let mut checks: Vec<Value> = vec![];

    // ── 1. Verificar sensores (última lectura en DB) ───────────────────────
    for pipeline in &cfg.pipelines {
        for node in &pipeline.nodes {
            if let agrodash_shared::NodeKind::PostgresSensor(sensor_cfg) = &node.kind {
                for sensor in &sensor_cfg.sensors {
                    let sensor_id: uuid::Uuid = match sensor.id.parse() {
                        Ok(id) => id,
                        Err(_) => {
                            checks.push(json!({
                                "name":   format!("sensor:{}", sensor.label),
                                "status": "error",
                                "detail": format!("UUID inválido: {}", sensor.id),
                            }));
                            continue;
                        }
                    };

                    // age_secs calculado por Postgres con su propio now() —
                    // evita cualquier desfase de timezone entre la API y la DB.
                    let res = sqlx::query!(
                        r#"SELECT value::float8 AS "value!: f64",
                                  EXTRACT(EPOCH FROM (now() AT TIME ZONE 'America/Costa_Rica' - created_at))::bigint
                                      AS "age_secs!: i64"
                           FROM readings WHERE sensor_id = $1
                           ORDER BY created_at DESC LIMIT 1"#,
                        sensor_id,
                    )
                    .fetch_optional(&state.pool)
                    .await;

                    match res {
                        Err(e) => checks.push(json!({
                            "name":   format!("sensor:{}", sensor.label),
                            "status": "error",
                            "detail": e.to_string(),
                        })),
                        Ok(None) => checks.push(json!({
                            "name":   format!("sensor:{}", sensor.label),
                            "status": "warn",
                            "detail": "Sin datos en la base de datos",
                        })),
                        Ok(Some(r)) => {
                            let age_secs = r.age_secs;
                            let stale = age_secs > 600; // >10 min → stale
                            checks.push(json!({
                                "name":   format!("sensor:{}", sensor.label),
                                "status": if stale { "warn" } else { "ok" },
                                "detail": format!("valor={:.3}, hace {}s{}",
                                    r.value, age_secs,
                                    if stale { " (stale)" } else { "" }),
                            }));
                        }
                    }
                }
            }
        }
    }

    // ── 2. MQTT ping (si hay shared_connections.mqtt) ─────────────────────
    if let Some(shared) = &cfg.shared_connections {
        if let Some(mqtt) = &shared.mqtt {
            let t0  = std::time::Instant::now();
            let res = reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .ok();

            // Hacemos el ping directamente con rumqttc desde la API
            // (reutilizamos la lógica del agente via spawn_blocking)
            let mqtt_clone = mqtt.clone();
            let ping_result = tokio::time::timeout(
                Duration::from_secs(6),
                tokio::task::spawn_blocking(move || -> Result<(), String> {
                    let url = mqtt_clone.broker_url.trim_start_matches("mqtt://");
                    let addr = url.split_once(':')
                        .map(|(h, p)| format!("{}:{}", h, p.parse::<u16>().unwrap_or(1883)))
                        .unwrap_or_else(|| format!("{}:1883", url));
                    use std::net::ToSocketAddrs;
                    let sock_addr = addr.to_socket_addrs()
                        .map_err(|e| e.to_string())?
                        .next()
                        .ok_or_else(|| "No se resolvió la dirección".to_string())?;
                    std::net::TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5))
                        .map(|_| ()).map_err(|e| e.to_string())
                }),
            ).await;

            let elapsed = t0.elapsed().as_millis();
            match ping_result {
                Ok(Ok(Ok(()))) => checks.push(json!({
                    "name":   "mqtt:broker",
                    "status": "ok",
                    "detail": format!("TCP conectado en {}ms", elapsed),
                })),
                Ok(Ok(Err(e))) => checks.push(json!({
                    "name":   "mqtt:broker",
                    "status": "error",
                    "detail": e,
                })),
                _ => checks.push(json!({
                    "name":   "mqtt:broker",
                    "status": "error",
                    "detail": "Timeout o error de conexión",
                })),
            }
            let _ = res; // evitar warning unused
        }

        // ── 3. HTTP ping (si hay shared_connections.http) ─────────────────
        if let Some(http) = &shared.http {
            let t0     = std::time::Instant::now();
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(http.timeout_secs.unwrap_or(5)))
                .build();

            match client {
                Err(e) => checks.push(json!({
                    "name": "http:base_url", "status": "error", "detail": e.to_string()
                })),
                Ok(c) => {
                    match c.get(&http.base_url).send().await {
                        Err(e) => checks.push(json!({
                            "name":   "http:base_url",
                            "status": "error",
                            "detail": e.to_string(),
                        })),
                        Ok(res) => {
                            let elapsed = t0.elapsed().as_millis();
                            checks.push(json!({
                                "name":   "http:base_url",
                                "status": if res.status().is_server_error() { "error" } else { "ok" },
                                "detail": format!("status={}, {}ms", res.status(), elapsed),
                            }));
                        }
                    }
                }
            }
        }
    }

    // ── 4. Agent dry-run (solo si está running) ───────────────────────────
    if row.status == "running" {
        // process_sockets retorna Vec<(pipeline_id: String, sock_path: PathBuf)>
        let sockets = state.manager.process_sockets(process_id);
        if sockets.is_empty() {
            checks.push(json!({
                "name":   "agent:dry-run",
                "status": "warn",
                "detail": "Proceso marcado como running pero sin agentes activos",
            }));
        } else {
            for (pipeline_id, _sock_path) in &sockets {
                let key = crate::agent_manager::AgentKey {
                    process_id,
                    pipeline_id: pipeline_id.clone(),
                };
                let t0      = std::time::Instant::now();
                let result  = state.manager.socket_cmd(&key, AgentCommand::SelfTest).await;
                let elapsed = t0.elapsed().as_millis();

                match result {
                    None => checks.push(json!({
                        "name":   format!("agent:{pipeline_id}"),
                        "status": "error",
                        "detail": "Socket no responde",
                    })),
                    Some(resp) if !resp.ok => checks.push(json!({
                        "name":   format!("agent:{pipeline_id}"),
                        "status": "error",
                        "detail": resp.error.unwrap_or_default(),
                    })),
                    Some(resp) => {
                        let data = resp.data.unwrap_or(json!({}));
                        checks.push(json!({
                            "name":   format!("agent:{pipeline_id}"),
                            "status": "ok",
                            "detail": format!("dry-run ok en {elapsed}ms, is_ready={}",
                                data.get("is_ready").and_then(|v| v.as_bool()).unwrap_or(false),
                            ),
                            "nodes": data.get("nodes"),
                        }));
                    }
                }
            }
        }
    } else {
        checks.push(json!({
            "name":   "agent:dry-run",
            "status": "info",
            "detail": "Proceso detenido — iniciar para ejecutar dry-run en agentes reales",
        }));
    }

    // ── Overall ───────────────────────────────────────────────────────────
    let overall = if checks.iter().any(|c| c.get("status").and_then(|v| v.as_str()) == Some("error")) {
        "error"
    } else if checks.iter().any(|c| c.get("status").and_then(|v| v.as_str()) == Some("warn")) {
        "warn"
    } else {
        "ok"
    };

    Ok(Json(json!({ "overall": overall, "checks": checks })))
}

// ── POST /processes/:id/command ───────────────────────────────────────────────

pub async fn send_command(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "operator").await?;

    let row = sqlx::query!(
        "SELECT control_url, api_key FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    body["_user_id"] = json!(claims.sub.to_string());

    let result = control_post(&row.control_url, &row.api_key, "/command", body.clone())
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    let cmd = body.get("cmd").and_then(|v| v.as_str()).unwrap_or("unknown");
    sqlx::query!(
        r#"
        INSERT INTO process_logs (process_id, level, source, message, data, user_id)
        VALUES ($1, 'info', 'user', $2, $3, $4)
        "#,
        process_id, format!("cmd:{cmd}"), body, claims.sub,
    )
    .execute(&state.pool)
    .await
    .ok();

    Ok(Json(result))
}

// ── GET /processes/:id/state ──────────────────────────────────────────────────

pub async fn get_state(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        "SELECT status, last_state, last_seen_at FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    Ok(Json(json!({
        "status":      row.status,
        "last_state":  row.last_state,
        "last_seen_at": row.last_seen_at,
    })))
}

// ── GET /processes/:id/stream — SSE ──────────────────────────────────────────

pub async fn stream_state(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let secs = q.interval_secs.unwrap_or(5).max(2).min(60);

    let stream = stream::unfold(
        (state.pool.clone(), process_id, None::<String>),
        move |(pool, pid, last_seen)| async move {
            let mut ticker = interval(Duration::from_secs(secs));
            ticker.tick().await;

            let row = sqlx::query!(
                r#"
                SELECT status, last_seen_at,
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
                pid, last_seen,
            )
            .fetch_optional(&pool)
            .await;

            match row {
                Ok(Some(r)) => {
                    let new_last = r.last_seen_at
                        .map(|t| t.to_rfc3339())
                        .or(last_seen.clone());

                    // SSE solo manda status + last_seen_at + logs nuevos.
                    // last_state (JSON grande con node_states) se omite aquí
                    // y se carga una sola vez en GET /processes/:id al montar.
                    let payload = json!({
                        "status":      r.status,
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

// ── GET /processes/:id/logs ───────────────────────────────────────────────────

pub async fn get_logs(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let limit = q.limit.unwrap_or(100).min(500);

    let rows = sqlx::query_as!(
        ProcessLogRow,
        r#"
        SELECT id, process_id, ts, level, source, message, data, user_id
        FROM process_logs
        WHERE process_id = $1
          AND ($2::timestamptz IS NULL OR ts > $2)
          AND ($3::text IS NULL OR level = $3)
          AND ($4::text IS NULL OR source = $4)
        ORDER BY ts DESC
        LIMIT $5
        "#,
        process_id, q.since, q.level, q.source, limit,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(err)?;

    Ok(Json(json!({ "logs": rows })))
}

// ── GET /processes/:id/readings ───────────────────────────────────────────────

pub async fn get_readings(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<ReadingsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let limit = q.limit.unwrap_or(500).min(2000);

    let rows = sqlx::query!(
        r#"
        SELECT id, pipeline_id, ts, raw, filtered, p_diag, decision, actuator
        FROM process_readings
        WHERE process_id = $1
          AND ($2::text IS NULL OR pipeline_id = $2)
          AND ($3::timestamptz IS NULL OR ts >= $3)
          AND ($4::timestamptz IS NULL OR ts <= $4)
        ORDER BY ts DESC
        LIMIT $5
        "#,
        process_id, q.pipeline_id, q.since, q.until, limit,
    )
    .fetch_all(&state.pool)
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

// ── GET /processes/:id/valve-events ──────────────────────────────────────────

pub async fn get_valve_events(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<ValveEventsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let limit = q.limit.unwrap_or(100).min(500);

    let rows = sqlx::query_as!(
        ValveEventRow,
        r#"
        SELECT id, process_id, ts, linea, estado, modo,
               triggered_by, kalman_convergido, kalman_x_hat, context
        FROM process_valve_events
        WHERE process_id = $1
          AND ($2::timestamptz IS NULL OR ts > $2)
          AND ($3::text IS NULL OR linea = $3)
        ORDER BY ts DESC
        LIMIT $4
        "#,
        process_id, q.since, q.linea, limit,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(err)?;

    Ok(Json(json!({ "events": rows })))
}

// ── GET /processes/:id/logs/tail ──────────────────────────────────────────────

pub async fn tail_control_log(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        "SELECT control_url, api_key FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    let result = control_get(&row.control_url, &row.api_key, "/logs/tail?n=150")
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    Ok(Json(result))
}

// ── Rutas internas para el agente ─────────────────────────────────────────────

pub async fn get_config(
    State(state): State<AppState>,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query!(
        "SELECT config FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    Ok(Json(row.config))
}

pub async fn get_agent_state(
    State(state): State<AppState>,
    Path((process_id, pipeline_id)): Path<(Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query!(
        r#"
        SELECT state FROM process_pipeline_states
        WHERE process_id = $1 AND pipeline_id = $2
        "#,
        process_id, pipeline_id,
    )
    .fetch_optional(&state.pool)
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
        r#"
        INSERT INTO process_pipeline_states (process_id, pipeline_id, state, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (process_id, pipeline_id)
        DO UPDATE SET state = $3, updated_at = now()
        "#,
        process_id, pipeline_id, body,
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    sqlx::query!(
        "UPDATE processes SET last_seen_at = now(), status = 'running', updated_at = now() WHERE id = $1",
        process_id,
    )
    .execute(&state.pool)
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
    .execute(&state.pool)
    .await
    .map_err(err)?;

    sqlx::query!(
        "UPDATE processes SET status = 'error', updated_at = now() WHERE id = $1",
        process_id,
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Colaboradores ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddCollabRequest {
    pub user_id: Uuid,
    pub role:    String,
}

pub async fn add_collaborator(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(body): Json<AddCollabRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "admin").await?;

    if !["viewer", "operator", "admin"].contains(&body.role.as_str()) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Rol inválido"}))));
    }

    sqlx::query!(
        r#"
        INSERT INTO process_collaborators (process_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (process_id, user_id) DO UPDATE SET role = $3
        "#,
        process_id, body.user_id, body.role,
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_collaborator(
    State(state): State<AppState>,
    claims: Claims,
    Path((process_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "admin").await?;

    sqlx::query!(
        "DELETE FROM process_collaborators WHERE process_id = $1 AND user_id = $2",
        process_id, user_id,
    )
    .execute(&state.pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_collaborators(
    State(state): State<AppState>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&state.pool, process_id, claims.sub, "viewer").await?;

    let rows = sqlx::query!(
        r#"
        SELECT c.user_id, c.role, c.added_at, u.email
        FROM process_collaborators c
        JOIN users u ON u.id = c.user_id
        WHERE c.process_id = $1
        ORDER BY c.added_at
        "#,
        process_id,
    )
    .fetch_all(&state.pool)
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