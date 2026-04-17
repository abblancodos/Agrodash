// src/routes/stats.rs
//
// GET /api/v1/stats
//
// Lee de sensor_stats y sensor_correlations (pre-calculadas por el worker).
// No toca la tabla readings — cero hammering.
//
// Query params opcionales:
//   ?box_id=<uuid>          filtrar por caja
//   ?min_score=<f64>        solo sensores con anomaly_score >= valor
//   ?corr_threshold=<f64>   umbral de correlación (default 0.85)

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{SensorCorrelation, SensorStat, StatsResponse};

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub box_id:          Option<Uuid>,
    pub min_score:       Option<f64>,
    pub corr_threshold:  Option<f64>,
}

pub async fn get_stats(
    State(pool): State<PgPool>,
    Query(params): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, (StatusCode, String)> {
    let corr_threshold = params.corr_threshold.unwrap_or(0.85);

    // ── Sensores con sus stats ────────────────────────────────────────────────
    let sensors: Vec<SensorStat> = sqlx::query_as!(
        SensorStat,
        r#"
        SELECT
            ss.sensor_id,
            s.box_id,
            b.name          AS box_name,
            s.sensor_number,
            s.type          AS sensor_type,
            ss.last_value,
            ss.last_seen_at,
            ss.mean_24h,
            ss.stddev_24h,
            ss.min_24h,
            ss.max_24h,
            ss.count_24h,
            ss.anomaly_score,
            ss.rate_of_change
        FROM sensor_stats ss
        JOIN sensors s  ON s.id     = ss.sensor_id
        JOIN boxes   b  ON b.id     = s.box_id
        WHERE ($1::uuid IS NULL OR s.box_id = $1)
          AND ($2::float8 IS NULL OR ss.anomaly_score >= $2)
        ORDER BY
            ss.anomaly_score DESC NULLS LAST,
            b.name,
            s.sensor_number
        "#,
        params.box_id as Option<Uuid>,
        params.min_score as Option<f64>,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Correlaciones ─────────────────────────────────────────────────────────
    let correlations: Vec<SensorCorrelation> = sqlx::query_as!(
        SensorCorrelation,
        r#"
        SELECT
            box_id,
            sensor_type,
            sensor_id_a,
            sensor_id_b,
            pearson_r
        FROM sensor_correlations
        WHERE ($1::uuid IS NULL OR box_id = $1)
          AND ABS(pearson_r) >= $2
        ORDER BY ABS(pearson_r) DESC
        "#,
        params.box_id as Option<Uuid>,
        corr_threshold,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(StatsResponse {
        computed_at:  chrono::Utc::now(),
        sensors,
        correlations,
    }))
}