// src/routes/stats.rs

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
    pub box_id:         Option<Uuid>,
    pub min_score:      Option<f64>,
    pub corr_threshold: Option<f64>,
}

pub async fn get_stats(
    State(pool): State<PgPool>,
    Query(params): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, (StatusCode, String)> {
    let corr_threshold = params.corr_threshold.unwrap_or(0.85);

    // sqlx no acepta Option<Uuid> directamente en el macro con IS NULL.
    // Pasamos el UUID como bytes nulos cuando es None, y filtramos con CASE.
    // La forma más simple y compatible: dos queries según si box_id está presente.

    let sensors: Vec<SensorStat> = if let Some(box_id) = params.box_id {
        sqlx::query_as!(
            SensorStat,
            r#"
            SELECT
                ss.sensor_id    AS "sensor_id: Uuid",
                s.box_id        AS "box_id: Uuid",
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
            JOIN sensors s ON s.id   = ss.sensor_id
            JOIN boxes   b ON b.id   = s.box_id
            WHERE s.box_id = $1
              AND ($2::float8 IS NULL OR ss.anomaly_score >= $2)
            ORDER BY ss.anomaly_score DESC NULLS LAST, b.name, s.sensor_number
            "#,
            box_id as Uuid,
            params.min_score as Option<f64>,
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        sqlx::query_as!(
            SensorStat,
            r#"
            SELECT
                ss.sensor_id    AS "sensor_id: Uuid",
                s.box_id        AS "box_id: Uuid",
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
            JOIN sensors s ON s.id   = ss.sensor_id
            JOIN boxes   b ON b.id   = s.box_id
            WHERE ($1::float8 IS NULL OR ss.anomaly_score >= $1)
            ORDER BY ss.anomaly_score DESC NULLS LAST, b.name, s.sensor_number
            "#,
            params.min_score as Option<f64>,
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let correlations: Vec<SensorCorrelation> = if let Some(box_id) = params.box_id {
        sqlx::query_as!(
            SensorCorrelation,
            r#"
            SELECT
                box_id      AS "box_id: Uuid",
                sensor_type,
                sensor_id_a AS "sensor_id_a: Uuid",
                sensor_id_b AS "sensor_id_b: Uuid",
                pearson_r
            FROM sensor_correlations
            WHERE box_id = $1
              AND ABS(pearson_r) >= $2
            ORDER BY ABS(pearson_r) DESC
            "#,
            box_id as Uuid,
            corr_threshold,
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        sqlx::query_as!(
            SensorCorrelation,
            r#"
            SELECT
                box_id      AS "box_id: Uuid",
                sensor_type,
                sensor_id_a AS "sensor_id_a: Uuid",
                sensor_id_b AS "sensor_id_b: Uuid",
                pearson_r
            FROM sensor_correlations
            WHERE ABS(pearson_r) >= $1
            ORDER BY ABS(pearson_r) DESC
            "#,
            corr_threshold,
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    Ok(Json(StatsResponse {
        computed_at:  chrono::Utc::now(),
        sensors,
        correlations,
    }))
}