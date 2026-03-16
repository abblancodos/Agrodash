// src/routes/readings.rs

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::models::{LastReadingQuery, ReadingBucket, ReadingsQuery, TemperatureResponse, TimeRange};

/// GET /api/v1/readings
pub async fn get_readings(
    State(pool): State<PgPool>,
    Query(params): Query<ReadingsQuery>,
) -> Result<Json<Vec<ReadingBucket>>, (StatusCode, String)> {
    let range_secs = (params.to - params.from).num_seconds().max(1);
    let bucket_secs = (range_secs / params.points).max(1);

    let rows = sqlx::query_as!(
        ReadingBucket,
        r#"
        SELECT
            date_trunc('second',
                created_at - (
                    EXTRACT(EPOCH FROM created_at)::bigint % $4
                ) * INTERVAL '1 second'
            )                    AS "bucket!: chrono::NaiveDateTime",
            AVG(value)::float8   AS "value!: f64"
        FROM readings
        WHERE
            sensor_id  = $1
            AND created_at >= $2
            AND created_at <= $3
        GROUP BY 1
        ORDER BY 1
        "#,
        params.sensor_id,
        params.from.naive_utc(),
        params.to.naive_utc(),
        bucket_secs,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(rows))
}

/// GET /api/v1/readings/time-range
///
/// Filtra fechas razonables (entre 2020 y 2030) para excluir datos basura en la DB.
pub async fn get_time_range(
    State(pool): State<PgPool>,
) -> Result<Json<TimeRange>, (StatusCode, String)> {
    let row = sqlx::query_as!(
        TimeRange,
        r#"
        SELECT
            MIN(created_at) AS "first!: chrono::NaiveDateTime",
            MAX(created_at) AS "last!:  chrono::NaiveDateTime"
        FROM readings
        WHERE created_at BETWEEN '2020-01-01' AND NOW()
        "#
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(row))
}

/// GET /api/v1/environment/temperature
///
/// Lectura más reciente de sensores de temperatura, dentro del rango válido.
pub async fn get_temperature(
    State(pool): State<PgPool>,
) -> Result<Json<TemperatureResponse>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT r.value::float8 AS "value!"
        FROM readings r
        JOIN sensors  s ON s.id = r.sensor_id
        WHERE LOWER(s.type) IN ('temperatura', 'temperature', 'air temperature (°c)', 't')
          AND r.created_at BETWEEN '2020-01-01' AND NOW()
          AND r.value BETWEEN -10 AND 60
        ORDER BY r.created_at DESC
        LIMIT 1
        "#
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TemperatureResponse {
        temperature_c: row.value,
    }))
}

/// GET /api/v1/readings/last?sensor_id=
///
/// Devuelve la última lectura conocida de un sensor, sin importar el rango.
pub async fn get_last_reading(
    State(pool): State<PgPool>,
    Query(params): Query<LastReadingQuery>,
) -> Result<Json<Option<ReadingBucket>>, (StatusCode, String)> {
    let row = sqlx::query_as!(
        ReadingBucket,
        r#"
        SELECT
            created_at AS "bucket!: chrono::NaiveDateTime",
            value::float8 AS "value!: f64"
        FROM readings
        WHERE sensor_id = $1
          AND created_at BETWEEN '2020-01-01' AND NOW()
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        params.sensor_id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(row))
}