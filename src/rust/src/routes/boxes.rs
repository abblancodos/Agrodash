use axum::{extract::State, http::StatusCode, Json};
use sqlx::PgPool;

use crate::models::{BoxResponse, SensorResponse};

pub async fn get_boxes(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<BoxResponse>>, (StatusCode, String)> {
    let rows = sqlx::query!(
        r#"
        SELECT
            b.id      AS box_id,
            b.name    AS box_name,
            s.id      AS "sensor_id?: uuid::Uuid",
            s.sensor_number AS "sensor_number?: i32",
            s.type    AS "sensor_type?: String"
        FROM boxes b
        LEFT JOIN sensors s ON s.box_id = b.id
        ORDER BY b.name, s.sensor_number, s.type
        "#
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut boxes: Vec<BoxResponse> = Vec::new();

    for row in rows {
        let sensor = match (row.sensor_id, row.sensor_number, row.sensor_type) {
            (Some(sid), Some(num), Some(typ)) => Some(SensorResponse {
                id:            sid,
                sensor_number: num,
                sensor_type:   typ,
            }),
            _ => None,
        };

        let entry = boxes.iter_mut().find(|b| b.id == row.box_id);

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
