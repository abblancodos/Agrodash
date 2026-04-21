// src/routes/experiment_features.rs
//
// Endpoints adicionales para el sistema de experimentos:
//   Colaboradores, correcciones con audit, definitions, objetivos,
//   export CSV, hora del servidor, búsqueda de usuarios.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::Claims;

fn err(msg: impl ToString) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": msg.to_string() })))
}
fn bad(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg })))
}
fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Sin permisos" })))
}
fn not_found() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "No encontrado" })))
}

// ── Helper: verificar rol del usuario en el experimento ──────────────────────

async fn get_experiment_role(
    pool: &PgPool,
    exp_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    // Primero verificar si es owner
    let is_owner = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM experiments WHERE id = $1 AND owner_id = $2)",
        exp_id, user_id,
    )
    .fetch_one(pool)
    .await
    .map_err(err)?
    .unwrap_or(false);

    if is_owner {
        return Ok(Some("admin".to_string()));
    }

    // Si no, buscar en collaborators
    let role = sqlx::query_scalar!(
        "SELECT role FROM experiment_collaborators WHERE experiment_id = $1 AND user_id = $2",
        exp_id, user_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(err)?;

    Ok(role)
}

async fn require_role(
    pool: &PgPool,
    exp_id: Uuid,
    user_id: Uuid,
    min_role: &str,  // "viewer" | "editor" | "admin"
) -> Result<String, (StatusCode, Json<Value>)> {
    let role = get_experiment_role(pool, exp_id, user_id)
        .await?
        .ok_or_else(forbidden)?;

    let rank = |r: &str| match r { "viewer" => 1, "editor" => 2, _ => 3 };
    if rank(&role) < rank(min_role) {
        return Err(forbidden());
    }
    Ok(role)
}

// ── GET /api/v1/time ──────────────────────────────────────────────────────────
// Hora del servidor — se muestra antes de guardar una entry.

pub async fn server_time() -> Json<Value> {
    let now_utc = chrono::Utc::now();
    let now_cr  = now_utc - chrono::Duration::hours(6);
    Json(serde_json::json!({
        "utc": now_utc.to_rfc3339(),
        "cr":  now_cr.format("%Y-%m-%d %H:%M:%S").to_string(),
        "unix": now_utc.timestamp(),
    }))
}

// ── GET /api/v1/users/search?q= ───────────────────────────────────────────────
// Busca usuarios por email o display_name (para agregar como colaborador).
// Solo devuelve id + display_name + email — no datos sensibles.

#[derive(Deserialize)]
pub struct UserSearchQuery {
    pub q: String,
}

pub async fn search_users(
    State(pool): State<PgPool>,
    claims: Claims,
    Query(q): Query<UserSearchQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if q.q.len() < 2 {
        return Err(bad("La búsqueda debe tener al menos 2 caracteres"));
    }
    let pattern = format!("%{}%", q.q.to_lowercase());
    let rows = sqlx::query!(
        r#"
        SELECT id AS "id: Uuid", email, display_name
        FROM users
        WHERE LOWER(email) LIKE $1 OR LOWER(display_name) LIKE $1
          AND id != $2
        LIMIT 10
        "#,
        pattern,
        claims.sub as Uuid,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    let list: Vec<_> = rows.iter().map(|r| serde_json::json!({
        "id": r.id, "email": r.email, "display_name": r.display_name,
    })).collect();

    Ok(Json(serde_json::json!(list)))
}

// ── Colaboradores ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CollaboratorRow {
    pub user_id:      Uuid,
    pub email:        String,
    pub display_name: String,
    pub role:         String,
    pub added_at:     chrono::DateTime<chrono::Utc>,
}

pub async fn list_collaborators(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
) -> Result<Json<Vec<CollaboratorRow>>, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "viewer").await?;

    let rows = sqlx::query!(
        r#"
        SELECT c.user_id AS "user_id: Uuid", c.role, c.added_at,
               u.email, u.display_name
        FROM experiment_collaborators c
        JOIN users u ON u.id = c.user_id
        WHERE c.experiment_id = $1
        ORDER BY c.added_at
        "#,
        exp_id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    Ok(Json(rows.into_iter().map(|r| CollaboratorRow {
        user_id:      r.user_id,
        email:        r.email,
        display_name: r.display_name,
        role:         r.role,
        added_at:     r.added_at,
    }).collect()))
}

#[derive(Deserialize)]
pub struct AddCollaboratorRequest {
    pub user_id: Uuid,
    pub role:    String,
}

pub async fn add_collaborator(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
    Json(body): Json<AddCollaboratorRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "admin").await?;

    if !["viewer", "editor", "admin"].contains(&body.role.as_str()) {
        return Err(bad("role debe ser viewer, editor o admin"));
    }

    sqlx::query!(
        r#"
        INSERT INTO experiment_collaborators (experiment_id, user_id, role, added_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (experiment_id, user_id) DO UPDATE SET role = EXCLUDED.role
        "#,
        exp_id,
        body.user_id as Uuid,
        body.role,
        claims.sub as Uuid,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_collaborator(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((exp_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "admin").await?;

    sqlx::query!(
        "DELETE FROM experiment_collaborators WHERE experiment_id = $1 AND user_id = $2",
        exp_id, user_id as Uuid,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Correcciones ──────────────────────────────────────────────────────────────
// POST /experiments/:id/events/:eid/correct
//
// 1. Verifica que el usuario sea el autor del event original o admin
// 2. Marca el event original como is_voided = true
// 3. Crea un nuevo event con corrects_event_id apuntando al original

#[derive(Deserialize)]
pub struct CorrectEventRequest {
    pub step_key:          String,
    pub event_type:        String,
    pub soil_id:           Option<String>,
    pub iteration:         Option<i32>,
    pub data:              Value,
    pub note:              Option<String>,
    pub correction_reason: String,          // obligatorio
}

#[derive(Serialize, sqlx::FromRow)]
pub struct EventRow {
    pub id:                 Uuid,
    pub experiment_id:      Uuid,
    pub step_key:           String,
    pub event_type:         String,
    pub soil_id:            Option<String>,
    pub iteration:          Option<i32>,
    pub data:               Value,
    pub note:               Option<String>,
    pub recorded_by:        Option<Uuid>,
    pub corrects_event_id:  Option<Uuid>,
    pub correction_reason:  Option<String>,
    pub is_voided:          bool,
    pub recorded_at:        chrono::DateTime<chrono::Utc>,
}

pub async fn correct_event(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((exp_id, event_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CorrectEventRequest>,
) -> Result<Json<EventRow>, (StatusCode, Json<Value>)> {
    if body.correction_reason.trim().len() < 10 {
        return Err(bad("El motivo de corrección debe tener al menos 10 caracteres"));
    }

    // Verificar que el event existe y pertenece al experimento
    let original = sqlx::query!(
        r#"
        SELECT id AS "id: Uuid", recorded_by AS "recorded_by: Uuid", is_voided
        FROM experiment_events
        WHERE id = $1 AND experiment_id = $2
        "#,
        event_id, exp_id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if original.is_voided {
        return Err(bad("Esta entry ya está anulada — corregí la corrección activa"));
    }

    // Solo el autor o un admin puede corregir
    let role = require_role(&pool, exp_id, claims.sub, "editor").await?;
    let is_author = original.recorded_by == Some(claims.sub);
    if !is_author && role != "admin" {
        return Err(forbidden());
    }

    // Transacción: anular original + crear corrección
    let mut tx = pool.begin().await.map_err(err)?;

    sqlx::query!(
        "UPDATE experiment_events SET is_voided = true WHERE id = $1",
        event_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(err)?;

    let new_event = sqlx::query_as!(
        EventRow,
        r#"
        INSERT INTO experiment_events
            (experiment_id, step_key, event_type, soil_id, iteration,
             data, note, recorded_by, corrects_event_id, correction_reason,
             is_voided, recorded_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, false, NOW())
        RETURNING
            id AS "id: Uuid",
            experiment_id AS "experiment_id: Uuid",
            step_key, event_type, soil_id, iteration, data, note,
            recorded_by AS "recorded_by: Uuid",
            corrects_event_id AS "corrects_event_id: Uuid",
            correction_reason, is_voided, recorded_at
        "#,
        exp_id,
        body.step_key,
        body.event_type,
        body.soil_id,
        body.iteration,
        body.data,
        body.note,
        claims.sub as Uuid,
        event_id,
        body.correction_reason,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(err)?;

    tx.commit().await.map_err(err)?;

    Ok(Json(new_event))
}

// ── Definitions ───────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct DefinitionRow {
    pub id:            Uuid,
    pub experiment_id: Uuid,
    pub key:           String,
    pub r#type:        String,
    pub label:         String,
    pub payload:       Value,
    pub sort_order:    i32,
    pub created_at:    chrono::DateTime<chrono::Utc>,
}

pub async fn list_definitions(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
) -> Result<Json<Vec<DefinitionRow>>, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "viewer").await?;

    let rows = sqlx::query_as!(
        DefinitionRow,
        r#"
        SELECT id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
               key, type AS "type: String", label, payload, sort_order, created_at
        FROM experiment_definitions
        WHERE experiment_id = $1
        ORDER BY sort_order, created_at
        "#,
        exp_id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateDefinitionRequest {
    pub key:        String,
    pub r#type:     String,
    pub label:      String,
    pub payload:    Value,
    pub sort_order: Option<i32>,
}

pub async fn create_definition(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
    Json(body): Json<CreateDefinitionRequest>,
) -> Result<Json<DefinitionRow>, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "editor").await?;

    if !["constant", "expression", "step", "csv_schema"].contains(&body.r#type.as_str()) {
        return Err(bad("type debe ser constant, expression, step o csv_schema"));
    }
    if body.key.is_empty() || body.label.is_empty() {
        return Err(bad("key y label son requeridos"));
    }

    let row = sqlx::query_as!(
        DefinitionRow,
        r#"
        INSERT INTO experiment_definitions
            (experiment_id, key, type, label, payload, sort_order, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
                  key, type AS "type: String", label, payload, sort_order, created_at
        "#,
        exp_id,
        body.key,
        body.r#type,
        body.label,
        body.payload,
        body.sort_order.unwrap_or(0),
        claims.sub as Uuid,
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;

    Ok(Json(row))
}

pub async fn update_definition(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((exp_id, def_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CreateDefinitionRequest>,
) -> Result<Json<DefinitionRow>, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "editor").await?;

    let row = sqlx::query_as!(
        DefinitionRow,
        r#"
        UPDATE experiment_definitions
        SET label = $1, payload = $2, sort_order = $3, updated_at = NOW()
        WHERE id = $4 AND experiment_id = $5
        RETURNING id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
                  key, type AS "type: String", label, payload, sort_order, created_at
        "#,
        body.label,
        body.payload,
        body.sort_order.unwrap_or(0),
        def_id,
        exp_id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    Ok(Json(row))
}

pub async fn delete_definition(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((exp_id, def_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "admin").await?;

    sqlx::query!(
        "DELETE FROM experiment_definitions WHERE id = $1 AND experiment_id = $2",
        def_id, exp_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Objectives ────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ObjectiveRow {
    pub id:             Uuid,
    pub experiment_id:  Uuid,
    pub name:           String,
    pub condition_type: String,
    pub condition:      Value,
    pub severity:       String,
    pub goto_ok:        Option<String>,
    pub goto_violation: Option<String>,
    pub sort_order:     i32,
}

pub async fn list_objectives(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
) -> Result<Json<Vec<ObjectiveRow>>, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "viewer").await?;

    let rows = sqlx::query_as!(
        ObjectiveRow,
        r#"
        SELECT id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
               name, condition_type, condition, severity,
               goto_ok, goto_violation, sort_order
        FROM experiment_objectives
        WHERE experiment_id = $1
        ORDER BY sort_order, created_at
        "#,
        exp_id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateObjectiveRequest {
    pub name:           String,
    pub condition_type: String,
    pub condition:      Value,
    pub severity:       Option<String>,
    pub goto_ok:        Option<String>,
    pub goto_violation: Option<String>,
    pub sort_order:     Option<i32>,
}

pub async fn create_objective(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
    Json(body): Json<CreateObjectiveRequest>,
) -> Result<Json<ObjectiveRow>, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "editor").await?;

    if !["range", "expression"].contains(&body.condition_type.as_str()) {
        return Err(bad("condition_type debe ser range o expression"));
    }
    let severity = body.severity.unwrap_or_else(|| "warning".to_string());
    if !["info", "warning", "critical"].contains(&severity.as_str()) {
        return Err(bad("severity debe ser info, warning o critical"));
    }

    let row = sqlx::query_as!(
        ObjectiveRow,
        r#"
        INSERT INTO experiment_objectives
            (experiment_id, name, condition_type, condition, severity,
             goto_ok, goto_violation, sort_order)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
                  name, condition_type, condition, severity,
                  goto_ok, goto_violation, sort_order
        "#,
        exp_id,
        body.name,
        body.condition_type,
        body.condition,
        severity,
        body.goto_ok,
        body.goto_violation,
        body.sort_order.unwrap_or(0),
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;

    Ok(Json(row))
}

pub async fn delete_objective(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((exp_id, obj_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "admin").await?;

    sqlx::query!(
        "DELETE FROM experiment_objectives WHERE id = $1 AND experiment_id = $2",
        obj_id, exp_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Export CSV ────────────────────────────────────────────────────────────────
// GET /experiments/:id/export-csv
// Solo entries activas (is_voided = false).
// Las correcciones se incluyen marcadas con corrects_event_id.
// Las constantes del experimento van en las primeras filas como metadata.

pub async fn export_csv(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
) -> Result<([(axum::http::HeaderName, String); 2], String), (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "viewer").await?;

    let exp = sqlx::query!(
        r#"SELECT title, constants FROM experiments WHERE id = $1"#,
        exp_id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    let events = sqlx::query!(
        r#"
        SELECT e.step_key, e.event_type, e.soil_id, e.iteration,
               e.data, e.note, e.recorded_at,
               e.corrects_event_id AS "corrects_event_id: Uuid",
               e.correction_reason, e.is_voided,
               u.display_name AS recorded_by_name
        FROM experiment_events e
        LEFT JOIN users u ON u.id = e.recorded_by
        WHERE e.experiment_id = $1 AND e.is_voided = false
        ORDER BY e.recorded_at
        "#,
        exp_id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    let mut csv = String::new();

    // Metadata header
    csv.push_str(&format!("# experimento: {}\n", exp.title));
    if let Some(consts) = exp.constants.as_object() {
        for (k, v) in consts {
            csv.push_str(&format!("# {}: {}\n", k, v));
        }
    }
    csv.push('\n');

    // Column headers
    csv.push_str("timestamp_cr,step_key,event_type,soil_id,iteration,data,note,tipo,motivo_correccion,registrado_por\n");

    for ev in &events {
        let ts = ev.recorded_at
            .checked_sub_signed(chrono::Duration::hours(6))
            .unwrap_or(ev.recorded_at)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let tipo = if ev.corrects_event_id.is_some() { "correccion" } else { "activa" };
        let data_str = ev.data.to_string().replace(',', ";");
        let note_str = ev.note.as_deref().unwrap_or("").replace(',', ";");
        let reason   = ev.correction_reason.as_deref().unwrap_or("").replace(',', ";");
        let by       = ev.recorded_by_name.as_deref().unwrap_or("").replace(',', ";");
        let soil     = ev.soil_id.as_deref().unwrap_or("");
        let iter     = ev.iteration.map(|i| i.to_string()).unwrap_or_default();

        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            ts, ev.step_key, ev.event_type, soil, iter,
            data_str, note_str, tipo, reason, by,
        ));
    }

    let filename = format!("experimento-{}.csv", exp_id);
    Ok((
        [
            (axum::http::header::CONTENT_TYPE,
             "text/csv; charset=utf-8".to_string()),
            (axum::http::header::CONTENT_DISPOSITION,
             format!("attachment; filename=\"{}\"", filename)),
        ],
        csv,
    ))
}

// ── PATCH /experiments/:id/status ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub async fn update_status(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(exp_id): Path<Uuid>,
    Json(body): Json<UpdateStatusRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_role(&pool, exp_id, claims.sub, "admin").await?;

    if !["active", "completed", "archived"].contains(&body.status.as_str()) {
        return Err(bad("status debe ser active, completed o archived"));
    }

    sqlx::query!(
        "UPDATE experiments SET status = $1, updated_at = NOW() WHERE id = $2",
        body.status, exp_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}