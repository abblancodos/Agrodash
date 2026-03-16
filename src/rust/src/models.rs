// src/models.rs
// Structs que mapean directo a las tablas de PostgreSQL.
// sqlx::FromRow permite hacer query_as! sin boilerplate.
// serde::Serialize convierte a JSON para las responses de Axum.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Tablas base ───────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct Box {
    pub id:   Uuid,
    pub name: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Sensor {
    pub id:            Uuid,
    pub box_id:        Uuid,
    pub sensor_number: i32,
    #[sqlx(rename = "type")]
    pub sensor_type:   String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Reading {
    pub id:         Uuid,
    pub sensor_id:  Uuid,
    pub value:      f64,
    pub created_at: NaiveDateTime,
}

// ── Response shapes (lo que se serializa a JSON) ──────────────────────────────

/// Un sensor dentro de la respuesta de /api/v1/boxes
#[derive(Debug, Serialize)]
pub struct SensorResponse {
    pub id:            Uuid,
    pub sensor_number: i32,
    #[serde(rename = "type")]
    pub sensor_type:   String,
}

/// Una caja con sus sensores — respuesta de GET /api/v1/boxes
#[derive(Debug, Serialize)]
pub struct BoxResponse {
    pub id:      Uuid,
    pub name:    String,
    pub sensors: Vec<SensorResponse>,
}

/// Un punto de lectura submuestreado — respuesta de GET /api/v1/readings
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReadingBucket {
    /// Timestamp del bucket (ya formateado en SQL con date_trunc)
    pub bucket: NaiveDateTime,
    /// Promedio de las lecturas en ese bucket
    pub value:  f64,
}

/// Rango temporal de los datos — respuesta de GET /api/v1/readings/time-range
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TimeRange {
    pub first: NaiveDateTime,
    pub last:  NaiveDateTime,
}

/// Temperatura ambiente — respuesta de GET /api/v1/environment/temperature
#[derive(Debug, Serialize)]
pub struct TemperatureResponse {
    pub temperature_c: f64,
}

/// Query params de GET /api/v1/readings
#[derive(Debug, Deserialize)]
pub struct ReadingsQuery {
    pub sensor_id: Uuid,
    pub from:      chrono::DateTime<chrono::Utc>,
    pub to:        chrono::DateTime<chrono::Utc>,
    /// Número de puntos deseados — el servidor decide el bucket size
    #[serde(default = "default_points")]
    pub points:    i64,
}

fn default_points() -> i64 { 300 }

/// Query params de GET /api/v1/readings/last
#[derive(Debug, Deserialize)]
pub struct LastReadingQuery {
    pub sensor_id: Uuid,
}