// src/routes/boxes.rs
// GET /api/v1/boxes
// Devuelve todas las cajas con sus sensores anidados.

use axum::{extract::State, http::StatusCode, Json};
use sqlx::PgPool;

use crate::models::{BoxResponse, SensorResponse};

pub async fn get_boxes(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<BoxResponse>>, (StatusCode, String)> {
    // Traemos boxes y sensors en una sola query con JOIN.
    // Usamos query! (no query_as!) para poder manejar el join manualmente.
    let rows = sqlx::query!(
        r#"
        SELECT
            b.id      AS box_id,
            b.name    AS box_name,
            s.id      AS sensor_id,
            s.sensor_number,
            s.type    AS sensor_type
        FROM boxes b
        LEFT JOIN sensors s ON s.box_id = b.id
        ORDER BY b.name, s.sensor_number, s.type
        "#
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Agrupamos manualmente en Rust: [row] → Vec<BoxResponse>
    // Más explícito que JSON aggregation en SQL, más fácil de debuggear.
    let mut boxes: Vec<BoxResponse> = Vec::new();

    for row in rows {
        // ¿Ya existe esta caja en el vec?
        let entry = boxes.iter_mut().find(|b| b.id == row.box_id);

        let sensor = row.sensor_id.map(|sid| SensorResponse {
            id:            sid,
            sensor_number: row.sensor_number.unwrap_or(0),
            sensor_type:   row.sensor_type.unwrap_or_default(),
        });

        match entry {
            Some(b) => {
                if let Some(s) = sensor {
                    b.sensors.push(s);
                }
            }
            None => {
                boxes.push(BoxResponse {
                    id:      row.box_id,
                    name:    row.box_name,
                    sensors: sensor.into_iter().collect(),
                });
            }
        }
    }

    Ok(Json(boxes))
}