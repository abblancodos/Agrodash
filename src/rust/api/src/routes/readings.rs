// src/routes/readings.rs
//
// TIMEZONE — regla definitiva (auditada 2026-05-22):
//
//   La columna created_at es `timestamp WITHOUT time zone` en hora CR.
//   Los datos entran vía INSERT con el timestamp CR naive (ya sea via NOW()
//   con sesión en America/Costa_Rica, o enviado explícitamente en hora CR).
//
//   Para convertir correctamente a UTC:
//     created_at AT TIME ZONE 'America/Costa_Rica' AT TIME ZONE 'UTC'
//   Primer AT TIME ZONE: "este naive está en CR" → produce timestamptz CR.
//   Segundo AT TIME ZONE: convierte ese timestamptz a UTC naive.
//   El frontend agrega 'Z' al string recibido → lo trata como UTC → correcto.
//
//   Para los filtros WHERE (from/to):
//   El frontend manda ISO-8601 UTC real ("...Z"). Axum los parsea como
//   DateTime<Utc>. La sesión de Postgres es CR (UTC-6), así que comparar
//   con ::timestamptz hace que Postgres interprete el parámetro en CR — MALO.
//   El fix: $2::timestamp AT TIME ZONE 'UTC' — fuerza la interpretación UTC
//   del parámetro antes de comparar con el naive created_at.
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

    // from/to son DateTime<Utc> reales — comparar directo con created_at (timestamptz).
    // NO usar AT TIME ZONE en los parámetros: eso los desplazaría 6h incorrectamente.
    let rows = sqlx::query!(
        r#"
        SELECT
            date_trunc('second',
                created_at::timestamptz - (
                    EXTRACT(EPOCH FROM created_at)::bigint % $4
                ) * INTERVAL '1 second'
            ) AT TIME ZONE 'America/Costa_Rica' AT TIME ZONE 'UTC' AS "bucket!: chrono::NaiveDateTime",
            AVG(value)::float8       AS "value!: f64"
        FROM readings
        WHERE
            sensor_id  = $1
            AND created_at >= $2::timestamp AT TIME ZONE 'UTC'
            AND created_at <= $3::timestamp AT TIME ZONE 'UTC'
        GROUP BY 1
        ORDER BY 1
        "#,
        params.sensor_id,
        params.from as _,
        params.to as _,
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
            MIN(created_at) AT TIME ZONE 'America/Costa_Rica' AT TIME ZONE 'UTC' AS "first!: chrono::NaiveDateTime",
            MAX(created_at) AT TIME ZONE 'America/Costa_Rica' AT TIME ZONE 'UTC' AS "last!:  chrono::NaiveDateTime"
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
            created_at AT TIME ZONE 'America/Costa_Rica' AT TIME ZONE 'UTC' AS "bucket!: chrono::NaiveDateTime",
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
