// src/routes/readings.rs
// GET /api/v1/readings?sensor_id=&from=&to=&points=
// GET /api/v1/readings/time-range

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::models::{ReadingBucket, ReadingsQuery, TemperatureResponse, TimeRange};

/// GET /api/v1/readings
///
/// Devuelve lecturas submuestreadas para un sensor en un rango de tiempo.
/// El bucket size se calcula automáticamente según el rango y los puntos pedidos,
/// así el frontend siempre recibe ~points puntos sin importar el zoom.
pub async fn get_readings(
    State(pool): State<PgPool>,
    Query(params): Query<ReadingsQuery>,
) -> Result<Json<Vec<ReadingBucket>>, (StatusCode, String)> {
    // Calculamos el intervalo de bucket en segundos.
    // Ej: rango de 24h con 300 puntos → bucket de ~288 segundos (~5 min).
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
/// Devuelve el primer y último timestamp con datos en toda la base.
/// El frontend lo usa para inicializar el rango del date picker.
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
        "#
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(row))
}

/// GET /api/v1/environment/temperature
///
/// Devuelve la lectura más reciente de cualquier sensor de tipo temperatura.
/// El Topbar lo usa para mostrar la temperatura ambiente actual.
pub async fn get_temperature(
    State(pool): State<PgPool>,
) -> Result<Json<TemperatureResponse>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT r.value::float8 AS "value!"
        FROM readings r
        JOIN sensors  s ON s.id = r.sensor_id
        WHERE LOWER(s.type) IN ('temperatura', 'temperature', 'air temperature (°c)', 't')
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