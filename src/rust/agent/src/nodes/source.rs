// agent/src/nodes/source.rs

use async_trait::async_trait;
use agrodash_shared::{NodeState, Signal, NodeAction, PostgresSensorConfig};
use anyhow::Result;
use sqlx::PgPool;
use super::NodeInstance;

pub struct PostgresSensorNode {
    id:     String,
    config: PostgresSensorConfig,
}

impl PostgresSensorNode {
    pub fn new(id: String, config: PostgresSensorConfig) -> Self {
        Self { id, config }
    }
}

#[async_trait]
impl NodeInstance for PostgresSensorNode {
    async fn execute(&mut self, _inputs: Vec<Signal>, _dt: f64, pool: &PgPool) -> Result<Option<Signal>> {
        let ids: Vec<uuid::Uuid> = self.config.sensors.iter()
            .map(|s| s.id.parse().unwrap())
            .collect();

        let mut values = vec![0.0f64; ids.len()];
        for (i, sensor_id) in ids.iter().enumerate() {
            let row = sqlx::query!(
                r#"SELECT value::float8 AS "value!: f64" FROM readings WHERE sensor_id = $1 ORDER BY created_at DESC LIMIT 1"#,
                sensor_id,
            )
            .fetch_optional(pool)
            .await?;
            values[i] = row.map(|r| r.value).unwrap_or(0.0);
        }

        Ok(Some(Signal::Vector(values)))
    }

    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "postgres_sensor".into(), data: serde_json::json!({}), is_ready: true }
    }
    fn load_state(&mut self, _: &NodeState) {}
}
