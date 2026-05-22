// src/routes/readings.rs
//
// La columna created_at es timestamptz. Los datos se insertaron en hora CR
// (America/Costa_Rica, UTC-6). La API devuelve timestamps en UTC puro para
// que el frontend pueda convertir a cualquier timezone correctamente.
#![allow(clippy::panic)]

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::models::{
    LastReadingQuery, ReadingBucket, ReadingsQuery, TemperatureResponse, TimeRange,
};

/// GET /api/v1/readings
pub async fn get_readings(
    State(pool): State<PgPool>,
    Query(params): Query<ReadingsQuery>,
) -> Result<Json<Vec<ReadingBucket>>, (StatusCode, String)> {
    let range_secs = (params.to - params.from).num_seconds().max(1);
    let bucket_secs = (range_secs / params.points).max(1);

    // Convertir parámetros CR del frontend a UTC para comparar con la DB
    // Los parámetros vienen como NaiveDateTime en CR (sin timezone)
    // La DB guarda como timestamptz — comparar directo funciona porque
    // Postgres interpreta el NaiveDateTime según la zona del servidor (CR)
    let rows = sqlx::query!(
        r#"
        SELECT
            date_trunc('second',
                created_at::timestamptz - (
                    EXTRACT(EPOCH FROM created_at)::bigint % $4
                ) * INTERVAL '1 second'
            ) AT TIME ZONE 'UTC'     AS "bucket!: chrono::NaiveDateTime",
            AVG(value)::float8       AS "value!: f64"
        FROM readings
        WHERE
            sensor_id  = $1
            AND created_at >= $2::timestamp AT TIME ZONE 'America/Costa_Rica'
            AND created_at <= $3::timestamp AT TIME ZONE 'America/Costa_Rica'
        GROUP BY 1
        ORDER BY 1
        "#,
        params.sensor_id,
        params.from,
        params.to,
        bucket_secs,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result: Vec<ReadingBucket> = rows
        .into_iter()
        .map(|r| ReadingBucket {
            bucket: r.bucket,
            value: r.value,
        })
        .collect();

    Ok(Json(result))
}

/// GET /api/v1/readings/time-range
pub async fn get_time_range(
    State(pool): State<PgPool>,
) -> Result<Json<TimeRange>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT
            MIN(created_at) AT TIME ZONE 'UTC' AS "first!: chrono::NaiveDateTime",
            MAX(created_at) AT TIME ZONE 'UTC' AS "last!:  chrono::NaiveDateTime"
        FROM readings
        "#
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TimeRange {
        first: row.first,
        last: row.last,
    }))
}

/// GET /api/v1/readings/last?sensor_id=
pub async fn get_last_reading(
    State(pool): State<PgPool>,
    Query(params): Query<LastReadingQuery>,
) -> Result<Json<Option<ReadingBucket>>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT
            created_at AT TIME ZONE 'UTC' AS "bucket!: chrono::NaiveDateTime",
            value::float8                 AS "value!: f64"
        FROM readings
        WHERE sensor_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        params.sensor_id,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(row.map(|r| ReadingBucket {
        bucket: r.bucket,
        value: r.value,
    })))
}

/// GET /api/v1/environment/temperature
pub async fn get_temperature(
    State(pool): State<PgPool>,
) -> Result<Json<TemperatureResponse>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT r.value::float8 AS "value!"
        FROM readings r
        JOIN sensors s ON s.id = r.sensor_id
        WHERE LOWER(s.type) IN ('temperatura', 'temperature', 'air temperature (°c)', 't')
          AND r.created_at >= NOW() - INTERVAL '14 days'
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
