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

    // sqlx no puede manejar Option<Uuid> como parámetro en query_as! con
    // columnas UUID no-nullable. Resolvemos con query! + mapeo manual,
    // usando "field!: Type" para forzar NOT NULL en columnas que lo son.

    let sensor_rows = sqlx::query!(
        r#"
        SELECT
            ss.sensor_id    AS "sensor_id!: Uuid",
            s.box_id        AS "box_id!: Uuid",
            b.name          AS "box_name!",
            s.sensor_number AS "sensor_number!",
            s.type          AS "sensor_type!",
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
        JOIN sensors s ON s.id = ss.sensor_id
        JOIN boxes   b ON b.id = s.box_id
        WHERE ($1::uuid IS NULL OR s.box_id = $1)
          AND ($2::float8 IS NULL OR ss.anomaly_score >= $2)
        ORDER BY ss.anomaly_score DESC NULLS LAST, b.name, s.sensor_number
        "#,
        params.box_id as Option<Uuid>,
        params.min_score,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sensors: Vec<SensorStat> = sensor_rows
        .into_iter()
        .map(|r| SensorStat {
            sensor_id:      r.sensor_id,
            box_id:         r.box_id,
            box_name:       r.box_name,
            sensor_number:  r.sensor_number,
            sensor_type:    r.sensor_type,
            last_value:     r.last_value,
            last_seen_at:   r.last_seen_at,
            mean_24h:       r.mean_24h,
            stddev_24h:     r.stddev_24h,
            min_24h:        r.min_24h,
            max_24h:        r.max_24h,
            count_24h:      r.count_24h,
            anomaly_score:  r.anomaly_score,
            rate_of_change: r.rate_of_change,
        })
        .collect();

    let corr_rows = sqlx::query!(
        r#"
        SELECT
            box_id      AS "box_id!: Uuid",
            sensor_type AS "sensor_type!",
            sensor_id_a AS "sensor_id_a!: Uuid",
            sensor_id_b AS "sensor_id_b!: Uuid",
            pearson_r   AS "pearson_r!"
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

    let correlations: Vec<SensorCorrelation> = corr_rows
        .into_iter()
        .map(|r| SensorCorrelation {
            box_id:      r.box_id,
            sensor_type: r.sensor_type,
            sensor_id_a: r.sensor_id_a,
            sensor_id_b: r.sensor_id_b,
            pearson_r:   r.pearson_r,
        })
        .collect();

    Ok(Json(StatsResponse {
        computed_at: chrono::Utc::now(),
        sensors,
        correlations,
    }))
}
