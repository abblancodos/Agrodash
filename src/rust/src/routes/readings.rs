// src/routes/readings.rs
//
// La DB guarda timestamps en hora CR (UTC-6) sin indicación de timezone.
// Todas las conversiones se hacen aquí para que el frontend trabaje en UTC puro.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::models::{LastReadingQuery, ReadingBucket, ReadingsQuery, TemperatureResponse, TimeRange};

const CR_OFFSET_SECS: i64 = 6 * 3600; // UTC-6

/// Convierte un timestamp UTC del frontend a hora CR para comparar con la DB.
fn to_cr(dt: chrono::DateTime<chrono::Utc>) -> chrono::NaiveDateTime {
    (dt - chrono::Duration::seconds(CR_OFFSET_SECS)).naive_utc()
}

/// Convierte un timestamp CR de la DB a UTC para devolver al frontend.
fn to_utc(dt: chrono::NaiveDateTime) -> chrono::NaiveDateTime {
    dt + chrono::Duration::seconds(CR_OFFSET_SECS)
}

/// GET /api/v1/readings
pub async fn get_readings(
    State(pool): State<PgPool>,
    Query(params): Query<ReadingsQuery>,
) -> Result<Json<Vec<ReadingBucket>>, (StatusCode, String)> {
    let from_cr = to_cr(params.from);
    let to_cr   = to_cr(params.to);

    let range_secs = (to_cr - from_cr).num_seconds().max(1);
    let bucket_secs = (range_secs / params.points).max(1);

    let rows = sqlx::query!(
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
        from_cr,
        to_cr,
        bucket_secs,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Convert bucket timestamps from CR back to UTC before returning
    let result: Vec<ReadingBucket> = rows.into_iter().map(|r| ReadingBucket {
        bucket: to_utc(r.bucket),
        value: r.value,
    }).collect();

    Ok(Json(result))
}

/// GET /api/v1/readings/time-range
pub async fn get_time_range(
    State(pool): State<PgPool>,
) -> Result<Json<TimeRange>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT
            MIN(created_at) AS "first!: chrono::NaiveDateTime",
            MAX(created_at) AS "last!:  chrono::NaiveDateTime"
        FROM readings
        "#
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Return as UTC
    Ok(Json(TimeRange {
        first: to_utc(row.first),
        last:  to_utc(row.last),
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
            created_at AS "bucket!: chrono::NaiveDateTime",
            value::float8 AS "value!: f64"
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
        bucket: to_utc(r.bucket),
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