// src/routes/experiments.rs
//
// CRUD completo para el sistema de experimentos.
//
// Plantillas:
//   GET    /api/v1/experiment-templates          — listar (públicas + propias)
//   POST   /api/v1/experiment-templates          — crear (auth)
//   GET    /api/v1/experiment-templates/:id      — detalle
//   PUT    /api/v1/experiment-templates/:id      — actualizar (owner)
//
// Experimentos:
//   GET    /api/v1/experiments                   — listar (públicos + propios)
//   POST   /api/v1/experiments                   — crear (auth)
//   GET    /api/v1/experiments/:id               — detalle completo
//   PUT    /api/v1/experiments/:id/constants     — actualizar constantes
//   PATCH  /api/v1/experiments/:id/status        — cambiar estado
//
// Eventos:
//   GET    /api/v1/experiments/:id/events        — listar eventos
//   POST   /api/v1/experiments/:id/events        — registrar evento
//   DELETE /api/v1/experiments/:id/events/:eid   — borrar evento
//
// Series:
//   GET    /api/v1/experiments/:id/series        — listar series
//   POST   /api/v1/experiments/:id/series        — agregar punto
//
// CSV:
//   POST   /api/v1/experiments/:id/upload-csv    — subir y parsear CSV

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{Claims, OptionalClaims};

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

// ── Plantillas ────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct TemplateRow {
    pub id:          Uuid,
    pub owner_id:    Uuid,
    pub name:        String,
    pub description: Option<String>,
    pub public:      bool,
    pub steps:       Value,
    pub constants_schema: Value,
    pub created_at:  chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateTemplateRequest {
    pub name:             String,
    pub description:      Option<String>,
    pub public:           Option<bool>,
    pub steps:            Value,
    pub constants_schema: Option<Value>,
}

pub async fn list_templates(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
) -> Result<Json<Vec<TemplateRow>>, (StatusCode, Json<Value>)> {
    let user_id = claims.map(|c| c.sub);
    let rows = sqlx::query_as!(
        TemplateRow,
        r#"
        SELECT id AS "id: Uuid", owner_id AS "owner_id: Uuid", name,
               description, public, steps, constants_schema, created_at
        FROM experiment_templates
        WHERE public = true OR owner_id = $1
        ORDER BY created_at DESC
        "#,
        user_id as Option<Uuid>,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;
    Ok(Json(rows))
}

pub async fn create_template(
    State(pool): State<PgPool>,
    claims: Claims,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<Json<TemplateRow>, (StatusCode, Json<Value>)> {
    if body.name.is_empty() { return Err(bad("name es requerido")); }
    let row = sqlx::query_as!(
        TemplateRow,
        r#"
        INSERT INTO experiment_templates (owner_id, name, description, public, steps, constants_schema)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id AS "id: Uuid", owner_id AS "owner_id: Uuid", name,
                  description, public, steps, constants_schema, created_at
        "#,
        claims.sub as Uuid,
        body.name,
        body.description,
        body.public.unwrap_or(false),
        body.steps,
        body.constants_schema.unwrap_or(serde_json::json!([])),
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;
    Ok(Json(row))
}

pub async fn get_template(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<TemplateRow>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as!(
        TemplateRow,
        r#"
        SELECT id AS "id: Uuid", owner_id AS "owner_id: Uuid", name,
               description, public, steps, constants_schema, created_at
        FROM experiment_templates WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if !row.public && claims.map(|c| c.sub) != Some(row.owner_id) {
        return Err(forbidden());
    }
    Ok(Json(row))
}

// ── Experimentos ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ExperimentRow {
    pub id:          Uuid,
    pub template_id: Uuid,
    pub owner_id:    Uuid,
    pub title:       String,
    pub description: Option<String>,
    pub public:      bool,
    pub constants:   Value,
    pub status:      String,
    pub created_at:  chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateExperimentRequest {
    pub template_id: Uuid,
    pub title:       String,
    pub description: Option<String>,
    pub public:      Option<bool>,
    pub constants:   Option<Value>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub template_id: Option<Uuid>,
}

pub async fn list_experiments(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ExperimentRow>>, (StatusCode, Json<Value>)> {
    let user_id = claims.map(|c| c.sub);
    let rows = sqlx::query_as!(
        ExperimentRow,
        r#"
        SELECT id AS "id: Uuid", template_id AS "template_id: Uuid",
               owner_id AS "owner_id: Uuid", title, description,
               public, constants, status, created_at
        FROM experiments
        WHERE (public = true OR owner_id = $1)
          AND ($2::uuid IS NULL OR template_id = $2)
        ORDER BY created_at DESC
        "#,
        user_id as Option<Uuid>,
        q.template_id as Option<Uuid>,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;
    Ok(Json(rows))
}

pub async fn create_experiment(
    State(pool): State<PgPool>,
    claims: Claims,
    Json(body): Json<CreateExperimentRequest>,
) -> Result<Json<ExperimentRow>, (StatusCode, Json<Value>)> {
    if body.title.is_empty() { return Err(bad("title es requerido")); }
    let row = sqlx::query_as!(
        ExperimentRow,
        r#"
        INSERT INTO experiments (template_id, owner_id, title, description, public, constants)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id AS "id: Uuid", template_id AS "template_id: Uuid",
                  owner_id AS "owner_id: Uuid", title, description,
                  public, constants, status, created_at
        "#,
        body.template_id as Uuid,
        claims.sub as Uuid,
        body.title,
        body.description,
        body.public.unwrap_or(false),
        body.constants.unwrap_or(serde_json::json!({})),
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;
    Ok(Json(row))
}

pub async fn get_experiment(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<ExperimentRow>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as!(
        ExperimentRow,
        r#"
        SELECT id AS "id: Uuid", template_id AS "template_id: Uuid",
               owner_id AS "owner_id: Uuid", title, description,
               public, constants, status, created_at
        FROM experiments WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if !row.public && claims.map(|c| c.sub) != Some(row.owner_id) {
        return Err(forbidden());
    }
    Ok(Json(row))
}

#[derive(Deserialize)]
pub struct UpdateConstantsRequest {
    pub constants: Value,
}

pub async fn update_constants(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateConstantsRequest>,
) -> Result<Json<ExperimentRow>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as!(
        ExperimentRow,
        r#"
        UPDATE experiments SET constants = $1, updated_at = NOW()
        WHERE id = $2 AND owner_id = $3
        RETURNING id AS "id: Uuid", template_id AS "template_id: Uuid",
                  owner_id AS "owner_id: Uuid", title, description,
                  public, constants, status, created_at
        "#,
        body.constants,
        id,
        claims.sub as Uuid,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;
    Ok(Json(row))
}

// ── Eventos ───────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct EventRow {
    pub id:            Uuid,
    pub experiment_id: Uuid,
    pub step_key:      String,
    pub event_type:    String,
    pub soil_id:       Option<String>,
    pub iteration:     Option<i32>,
    pub data:          Value,
    pub note:          Option<String>,
    pub recorded_at:   chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateEventRequest {
    pub step_key:    String,
    pub event_type:  String,
    pub soil_id:     Option<String>,
    pub iteration:   Option<i32>,
    pub data:        Value,
    pub note:        Option<String>,
    pub recorded_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_events(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<EventRow>>, (StatusCode, Json<Value>)> {
    // Verificar acceso al experimento
    let exp = sqlx::query!(
        r#"SELECT public, owner_id AS "owner_id: Uuid" FROM experiments WHERE id = $1"#,
        id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if !exp.public && claims.map(|c| c.sub) != Some(exp.owner_id) {
        return Err(forbidden());
    }

    let rows = sqlx::query_as!(
        EventRow,
        r#"
        SELECT id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
               step_key, event_type, soil_id, iteration, data, note, recorded_at
        FROM experiment_events
        WHERE experiment_id = $1
        ORDER BY recorded_at ASC
        "#,
        id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;
    Ok(Json(rows))
}

pub async fn create_event(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateEventRequest>,
) -> Result<Json<EventRow>, (StatusCode, Json<Value>)> {
    // Solo el owner puede registrar eventos
    let exp = sqlx::query!(
        r#"SELECT owner_id AS "owner_id: Uuid" FROM experiments WHERE id = $1"#,
        id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if exp.owner_id != claims.sub {
        return Err(forbidden());
    }

    let recorded_at = body.recorded_at.unwrap_or_else(chrono::Utc::now);

    let row = sqlx::query_as!(
        EventRow,
        r#"
        INSERT INTO experiment_events
            (experiment_id, step_key, event_type, soil_id, iteration, data, note, recorded_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
                  step_key, event_type, soil_id, iteration, data, note, recorded_at
        "#,
        id,
        body.step_key,
        body.event_type,
        body.soil_id,
        body.iteration,
        body.data,
        body.note,
        recorded_at,
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;
    Ok(Json(row))
}

pub async fn delete_event(
    State(pool): State<PgPool>,
    claims: Claims,
    Path((exp_id, event_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let exp = sqlx::query!(
        r#"SELECT owner_id AS "owner_id: Uuid" FROM experiments WHERE id = $1"#,
        exp_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if exp.owner_id != claims.sub && !claims.is_admin() {
        return Err(forbidden());
    }

    sqlx::query!(
        "DELETE FROM experiment_events WHERE id = $1 AND experiment_id = $2",
        event_id, exp_id,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Series ────────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct SeriesPoint {
    pub id:            Uuid,
    pub experiment_id: Uuid,
    pub series_key:    String,
    pub soil_id:       Option<String>,
    pub value:         f64,
    pub unit:          Option<String>,
    pub note:          Option<String>,
    pub recorded_at:   chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateSeriesPointRequest {
    pub series_key:  String,
    pub soil_id:     Option<String>,
    pub value:       f64,
    pub unit:        Option<String>,
    pub note:        Option<String>,
    pub recorded_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_series(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<SeriesPoint>>, (StatusCode, Json<Value>)> {
    let exp = sqlx::query!(
        r#"SELECT public, owner_id AS "owner_id: Uuid" FROM experiments WHERE id = $1"#,
        id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if !exp.public && claims.map(|c| c.sub) != Some(exp.owner_id) {
        return Err(forbidden());
    }

    let rows = sqlx::query_as!(
        SeriesPoint,
        r#"
        SELECT id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
               series_key, soil_id, value, unit, note, recorded_at
        FROM experiment_series
        WHERE experiment_id = $1
        ORDER BY recorded_at ASC
        "#,
        id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;
    Ok(Json(rows))
}

pub async fn create_series_point(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateSeriesPointRequest>,
) -> Result<Json<SeriesPoint>, (StatusCode, Json<Value>)> {
    let exp = sqlx::query!(
        r#"SELECT owner_id AS "owner_id: Uuid" FROM experiments WHERE id = $1"#,
        id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if exp.owner_id != claims.sub {
        return Err(forbidden());
    }

    let recorded_at = body.recorded_at.unwrap_or_else(chrono::Utc::now);

    let row = sqlx::query_as!(
        SeriesPoint,
        r#"
        INSERT INTO experiment_series
            (experiment_id, series_key, soil_id, value, unit, note, recorded_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id AS "id: Uuid", experiment_id AS "experiment_id: Uuid",
                  series_key, soil_id, value, unit, note, recorded_at
        "#,
        id,
        body.series_key,
        body.soil_id,
        body.value,
        body.unit,
        body.note,
        recorded_at,
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;
    Ok(Json(row))
}

// ── CSV Upload ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct UploadCsvResponse {
    pub id:          Uuid,
    pub filename:    String,
    pub row_count:   i32,
    pub columns:     Vec<String>,
}

pub async fn upload_csv(
    State(pool): State<PgPool>,
    claims: Claims,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<UploadCsvResponse>, (StatusCode, Json<Value>)> {
    // Verificar acceso
    let exp = sqlx::query!(
        r#"SELECT owner_id AS "owner_id: Uuid" FROM experiments WHERE id = $1"#,
        id
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if exp.owner_id != claims.sub {
        return Err(forbidden());
    }

    // Leer el campo del multipart
    let mut filename  = String::from("upload.csv");
    let mut step_key  = String::from("csv");
    let mut csv_bytes = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| bad(&e.to_string()))? {
        match field.name() {
            Some("file") => {
                if let Some(n) = field.file_name() { filename = n.to_string(); }
                csv_bytes = field.bytes().await.map_err(|e| bad(&e.to_string()))?.to_vec();
            }
            Some("step_key") => {
                step_key = field.text().await.map_err(|e| bad(&e.to_string()))?;
            }
            _ => {}
        }
    }

    if csv_bytes.is_empty() {
        return Err(bad("No se recibió ningún archivo"));
    }

    // Parsear CSV
    let mut reader = csv::Reader::from_reader(csv_bytes.as_slice());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| bad(&e.to_string()))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| bad(&e.to_string()))?;
        let obj: serde_json::Map<String, serde_json::Value> = headers
            .iter()
            .zip(record.iter())
            .map(|(h, v)| {
                let val = v.trim().parse::<f64>()
                    .map(serde_json::Value::from)
                    .unwrap_or_else(|_| serde_json::Value::String(v.trim().to_string()));
                (h.clone(), val)
            })
            .collect();
        rows.push(serde_json::Value::Object(obj));
    }

    let row_count = rows.len() as i32;
    let columns_json = serde_json::to_value(&headers).unwrap();
    let parsed_json  = serde_json::to_value(&rows).unwrap();

    let file_id = sqlx::query_scalar!(
        r#"
        INSERT INTO experiment_files
            (experiment_id, step_key, filename, row_count, columns, parsed_data)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id AS "id: Uuid"
        "#,
        id,
        step_key,
        filename,
        row_count,
        columns_json,
        parsed_json,
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;

    Ok(Json(UploadCsvResponse {
        id: file_id,
        filename,
        row_count,
        columns: headers,
    }))
}

// ── Script execution ──────────────────────────────────────────────────────────
// POST /api/v1/experiments/:id/steps/:step_key/run
//
// Ejecuta el script Rhai del paso indicado con el contexto actual del
// experimento (constantes + valores de pasos anteriores) y devuelve
// outputs, goto y eventuales errores. No modifica la DB.

use crate::script_engine::{run_script, ScriptResult};

pub async fn run_step_script(
    State(pool): State<PgPool>,
    OptionalClaims(claims): OptionalClaims,
    Path((exp_id, step_key)): Path<(Uuid, String)>,
) -> Result<Json<ScriptResult>, (StatusCode, Json<Value>)> {

    // 1. Verificar acceso al experimento
    let exp = sqlx::query!(
        r#"
        SELECT e.public, e.constants, e.owner_id AS "owner_id: Uuid",
               t.steps
        FROM experiments e
        JOIN experiment_templates t ON t.id = e.template_id
        WHERE e.id = $1
        "#,
        exp_id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(not_found)?;

    if !exp.public && claims.map(|c| c.sub) != Some(exp.owner_id) {
        return Err(forbidden());
    }

    // 2. Buscar el step en la plantilla y extraer su script
    let steps_arr = exp.steps.as_array().ok_or_else(|| bad("steps inválidos en plantilla"))?;
    let step = steps_arr.iter()
        .find(|s| s.get("key").and_then(|k| k.as_str()) == Some(&step_key))
        .ok_or_else(not_found)?;

    let script = step.get("script")
        .and_then(|s| s.as_str())
        .ok_or_else(|| bad("El paso no tiene script"))?;

    // 3. Construir el contexto: constantes del experimento + últimos valores de cada step
    let constants = &exp.constants;

    // Obtener el último valor registrado por cada step_key
    let events = sqlx::query!(
        r#"
        SELECT DISTINCT ON (step_key)
            step_key, data, recorded_at
        FROM experiment_events
        WHERE experiment_id = $1
        ORDER BY step_key, recorded_at DESC
        "#,
        exp_id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    let mut steps_context = serde_json::Map::new();
    for ev in &events {
        let mut entry = serde_json::Map::new();
        // Extraer value numérico si existe
        if let Some(v) = ev.data.get("value").and_then(|v| v.as_f64()) {
            entry.insert("value".into(), serde_json::json!(v));
        }
        entry.insert("data".into(), ev.data.clone());
        entry.insert("recorded_at".into(), serde_json::json!(ev.recorded_at.to_rfc3339()));
        steps_context.insert(ev.step_key.clone(), Value::Object(entry));
    }

    // También incluir series — último punto de cada serie
    let series = sqlx::query!(
        r#"
        SELECT DISTINCT ON (series_key)
            series_key, value, recorded_at
        FROM experiment_series
        WHERE experiment_id = $1
        ORDER BY series_key, recorded_at DESC
        "#,
        exp_id,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    for s in &series {
        steps_context.entry(s.series_key.clone()).or_insert_with(|| {
            serde_json::json!({ "value": s.value, "recorded_at": s.recorded_at.to_rfc3339() })
        });
    }

    let steps_json = Value::Object(steps_context);

    // 4. Ejecutar script en hilo bloqueante (Rhai es sync)
    let script_owned   = script.to_string();
    let constants_owned = constants.clone();
    let steps_owned    = steps_json;

    let result = tokio::task::spawn_blocking(move || {
        run_script(&script_owned, &constants_owned, &steps_owned)
    })
    .await
    .map_err(|e| err(e.to_string()))?;

    Ok(Json(result))
}

// ── Validate script ───────────────────────────────────────────────────────────
// POST /api/v1/scripts/validate
// Compila el script y devuelve errores de sintaxis sin ejecutarlo.

#[derive(serde::Deserialize)]
pub struct ValidateScriptRequest {
    pub script: String,
}

pub async fn validate_script(
    Json(body): Json<ValidateScriptRequest>,
) -> Json<Value> {
    use rhai::Engine;
    let engine = Engine::new();
    match engine.compile(&body.script) {
        Ok(_)  => Json(serde_json::json!({ "valid": true })),
        Err(e) => Json(serde_json::json!({ "valid": false, "error": e.to_string() })),
    }
}
