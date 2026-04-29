// agent/src/source.rs

use agrodash_shared::SourceConfig;
use anyhow::{Context, Result};
use sqlx::PgPool;

/// Lee el vector de valores actuales para la fuente configurada.
/// Retorna los valores en el mismo orden que sensors[] en el config.
pub async fn read_source(cfg: &SourceConfig, pool: &PgPool) -> Result<Vec<f64>> {
    match cfg {
        SourceConfig::PostgresSensor(src) => {
            let ids: Vec<uuid::Uuid> = src.sensors.iter()
                .map(|s| s.id.parse().context("UUID de sensor inválido"))
                .collect::<Result<Vec<_>>>()?;

            // Leer el último valor de cada sensor
            let mut values = vec![None::<f64>; ids.len()];

            for (i, sensor_id) in ids.iter().enumerate() {
                let row = sqlx::query!(
                    r#"
                    SELECT value
                    FROM readings
                    WHERE sensor_id = $1
                    ORDER BY created_at DESC
                    LIMIT 1
                    "#,
                    sensor_id,
                )
                .fetch_optional(pool)
                .await
                .context("Error consultando reading")?;

                values[i] = row.map(|r| r.value);
            }

            // Si algún sensor no tiene datos, usar el último conocido o 0
            let result: Vec<f64> = values.iter().enumerate().map(|(i, v)| {
                v.unwrap_or_else(|| {
                    tracing::warn!(
                        "Sensor {} ({}) sin datos, usando 0.0",
                        i, src.sensors[i].label
                    );
                    0.0
                })
            }).collect();

            Ok(result)
        }
    }
}
