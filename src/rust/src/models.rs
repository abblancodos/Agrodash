// src/models.rs

use chrono::{DateTime, NaiveDateTime, Utc};
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

// ── Response shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SensorResponse {
    pub id:            Uuid,
    pub sensor_number: i32,
    #[serde(rename = "type")]
    pub sensor_type:   String,
}

#[derive(Debug, Serialize)]
pub struct BoxResponse {
    pub id:      Uuid,
    pub name:    String,
    pub sensors: Vec<SensorResponse>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReadingBucket {
    pub bucket: NaiveDateTime,
    pub value:  f64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TimeRange {
    pub first: NaiveDateTime,
    pub last:  NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct TemperatureResponse {
    pub temperature_c: f64,
}

// ── Stats ─────────────────────────────────────────────────────────────────────

/// Lo que devuelve GET /api/v1/stats — una entrada por sensor,
/// ordenada por anomaly_score DESC (los más raros primero).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SensorStat {
    pub sensor_id:      Uuid,
    pub box_id:         Uuid,
    pub box_name:       String,
    pub sensor_number:  i32,
    pub sensor_type:    String,

    pub last_value:     Option<f64>,
    /// UTC timestamp del último dato — el frontend calcula el tiempo relativo
    pub last_seen_at:   Option<DateTime<Utc>>,

    pub mean_24h:       Option<f64>,
    pub stddev_24h:     Option<f64>,
    pub min_24h:        Option<f64>,
    pub max_24h:        Option<f64>,
    pub count_24h:      Option<i32>,

    /// |last_value - mean_24h| / stddev_24h. NULL si no hay suficientes datos.
    pub anomaly_score:  Option<f64>,
    /// Cambio por hora. NULL si no hay dato previo.
    pub rate_of_change: Option<f64>,
}

/// Correlaciones entre sensores de la misma caja para la misma variable.
/// Solo se devuelven pares con |r| >= umbral (0.85 por defecto).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SensorCorrelation {
    pub box_id:       Uuid,
    pub sensor_type:  String,
    pub sensor_id_a:  Uuid,
    pub sensor_id_b:  Uuid,
    pub pearson_r:    f64,
}

/// Respuesta completa de GET /api/v1/stats
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub computed_at:   DateTime<Utc>,
    pub sensors:       Vec<SensorStat>,
    pub correlations:  Vec<SensorCorrelation>,
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ReadingsQuery {
    pub sensor_id: Uuid,
    pub from:      DateTime<Utc>,
    pub to:        DateTime<Utc>,
    #[serde(default = "default_points")]
    pub points:    i64,
}

fn default_points() -> i64 { 300 }

#[derive(Debug, Deserialize)]
pub struct LastReadingQuery {
    pub sensor_id: Uuid,
}