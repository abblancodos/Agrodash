// src/rust/src/routes/processes.rs
//
// Rutas de automatización / procesos.
// El Rust actúa como proxy autenticado entre el frontend y el contenedor de control.

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
use std::{convert::Infallible, time::Duration};
use tokio::time::interval;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use crate::auth::Claims;

fn err(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
}

fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "Sin permisos"})))
}

// ── Helpers de rol ────────────────────────────────────────────────────────────

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
    if level(&role) < level(min_role) {
        return Err(forbidden());
    }
    Ok(role)
}

// ── HTTP client al contenedor de control ──────────────────────────────────────

async fn control_get(url: &str, api_key: &str, path: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(format!("{}{}", url.trim_end_matches('/'), path))
        .header("X-API-Key", api_key)
        .send()
        .await
        .map_err(|e| e.to_string())?;

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
        .send()
        .await
        .map_err(|e| e.to_string())?;

    res.json::<Value>().await.map_err(|e| e.to_string())
}

// ── Structs ───────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ProcessRow {
    pub id:          Uuid,
    pub name:        String,
    pub description: Option<String>,
    pub r#type:      String,
    pub status:      String,
    pub control_url: String,
    pub config:      Value,
    pub last_state:  Option<Value>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub owner_id:    Uuid,
    pub created_at:  chrono::DateTime<chrono::Utc>,
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

// ── GET /processes ────────────────────────────────────────────────────────────

pub async fn list_processes(
    State(pool): State<PgPool>,
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
    .fetch_all(&pool)
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
    State(pool): State<PgPool>,
    claims: Claims,
    Json(body): Json<CreateProcessRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = Uuid::new_v4();
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
    .execute(&pool)
    .await
    .map_err(err)?;

    // Log inicial
    sqlx::query!(
        "INSERT INTO process_logs (process_id, source, message) VALUES ($1, 'system', 'Proceso creado')",
        id
    )
    .execute(&pool)
    .await
    .ok();

    Ok(Json(json!({ "id": id, "type": proc_type, "name": body.name })))
}

// ── GET /processes/:id ────────────────────────────────────────────────────────

pub async fn get_process(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let role = require_process_role(&pool, process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        r#"
        SELECT id, name, description, type, status, control_url,
               config, last_state, last_seen_at, owner_id, created_at
        FROM processes WHERE id = $1
        "#,
        process_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Proceso no encontrado"}))))?;

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

// ── POST /processes/:id/command ───────────────────────────────────────────────

pub async fn send_command(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Operadores y admins pueden mandar comandos
    require_process_role(&pool, process_id, claims.sub, "operator").await?;

    let row = sqlx::query!(
        "SELECT control_url, api_key FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Proceso no encontrado"}))))?;

    // Inyectar user_id para que el FastAPI lo loguee
    body["_user_id"] = json!(claims.sub.to_string());

    let result = control_post(&row.control_url, &row.api_key, "/command", body.clone())
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    // Loguear en DB
    let cmd = body.get("cmd").and_then(|v| v.as_str()).unwrap_or("unknown");
    sqlx::query!(
        r#"
        INSERT INTO process_logs (process_id, level, source, message, data, user_id)
        VALUES ($1, 'info', 'user', $2, $3, $4)
        "#,
        process_id,
        format!("cmd:{cmd}"),
        body,
        claims.sub,
    )
    .execute(&pool)
    .await
    .ok();

    Ok(Json(result))
}

// ── GET /processes/:id/state (on-demand desde cache DB) ───────────────────────

pub async fn get_state(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        "SELECT status, last_state, last_seen_at FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    Ok(Json(json!({
        "status":      row.status,
        "last_state":  row.last_state,
        "last_seen_at": row.last_seen_at,
    })))
}

// ── GET /processes/:id/stream — SSE ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct StreamQuery {
    pub interval_secs: Option<u64>,
}

pub async fn stream_state(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "viewer").await?;

    let secs = q.interval_secs.unwrap_or(5).max(2).min(60);

    let stream = stream::unfold(
        (pool, process_id, None::<String>),
        move |(pool, pid, last_seen)| async move {
            let mut ticker = interval(Duration::from_secs(secs));
            ticker.tick().await;

            let row = sqlx::query!(
                r#"
                SELECT status, last_state, last_seen_at,
                       -- logs nuevos desde el último envío
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

// ── GET /processes/:id/logs ───────────────────────────────────────────────────

pub async fn get_logs(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "viewer").await?;

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
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    Ok(Json(json!({ "logs": rows })))
}

// ── GET /processes/:id/valve-events ──────────────────────────────────────────

pub async fn get_valve_events(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Query(q): Query<ValveEventsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "viewer").await?;

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
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    Ok(Json(json!({ "events": rows })))
}

// ── GET /processes/:id/logs/tail — proxy al Control.log ──────────────────────

pub async fn tail_control_log(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "viewer").await?;

    let row = sqlx::query!(
        "SELECT control_url, api_key FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    let result = control_get(&row.control_url, &row.api_key, "/logs/tail?n=150")
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    Ok(Json(result))
}

// ── Colaboradores (mismo patrón que experimentos) ─────────────────────────────

#[derive(Deserialize)]
pub struct AddCollabRequest {
    pub user_id: Uuid,
    pub role:    String,
}

pub async fn add_collaborator(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
    Json(body): Json<AddCollabRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "admin").await?;

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
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_collaborator(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((process_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "admin").await?;

    sqlx::query!(
        "DELETE FROM process_collaborators WHERE process_id = $1 AND user_id = $2",
        process_id, user_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_collaborators(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_process_role(&pool, process_id, claims.sub, "viewer").await?;

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
    .fetch_all(&pool)
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
    State(pool): State<PgPool>,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query!(
        "SELECT config FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    Ok(Json(row.config))
}

pub async fn get_agent_state(
    State(pool): State<PgPool>,
    Path(process_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query!(
        "SELECT last_state FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "No encontrado"}))))?;

    Ok(Json(row.last_state.unwrap_or(json!({}))))
}

pub async fn post_agent_state(
    State(pool): State<PgPool>,
    Path(process_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    sqlx::query!(
        r#"
        UPDATE processes
        SET last_state   = $1,
            last_seen_at = now(),
            status       = 'running',
            updated_at   = now()
        WHERE id = $2
        "#,
        body, process_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn post_agent_error(
    State(pool): State<PgPool>,
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
    .execute(&pool)
    .await
    .map_err(err)?;

    sqlx::query!(
        "UPDATE processes SET status = 'error', updated_at = now() WHERE id = $1",
        process_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}