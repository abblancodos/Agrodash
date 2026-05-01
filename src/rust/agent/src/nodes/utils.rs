// agent/src/nodes/utils.rs

use async_trait::async_trait;
use agrodash_shared::{NodeState, Signal, VecOrScalar};
use anyhow::Result;
use sqlx::PgPool;
use super::{NodeInstance, expect_vector};

// ── Concat — une vectores de múltiples entradas ───────────────────────────────

pub struct ConcatNode { id: String }
impl ConcatNode { pub fn new(id: String) -> Self { Self { id } } }

#[async_trait]
impl NodeInstance for ConcatNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let mut out = vec![];
        for sig in &inputs {
            out.extend(expect_vector(sig, &self.id)?);
        }
        Ok(Some(Signal::Vector(out)))
    }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "concat".into(), data: serde_json::json!({}), is_ready: true }
    }
    fn load_state(&mut self, _: &NodeState) {}
}

// ── WeightedMean ──────────────────────────────────────────────────────────────

pub struct WeightedMeanNode { id: String, weights: Vec<f64> }
impl WeightedMeanNode { pub fn new(id: String, weights: Vec<f64>) -> Self { Self { id, weights } } }

#[async_trait]
impl NodeInstance for WeightedMeanNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        if inputs.is_empty() { return Ok(None); }
        let vecs: Vec<Vec<f64>> = inputs.iter().map(|s| expect_vector(s, &self.id)).collect::<Result<_>>()?;
        let dim = vecs[0].len();
        let mut out = vec![0.0f64; dim];
        let w_sum: f64 = self.weights.iter().take(vecs.len()).sum();
        for (i, vec) in vecs.iter().enumerate() {
            let w = self.weights.get(i).copied().unwrap_or(1.0) / w_sum;
            for (j, &v) in vec.iter().enumerate() { if j < dim { out[j] += v * w; } }
        }
        Ok(Some(Signal::Vector(out)))
    }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "weighted_mean".into(), data: serde_json::json!({}), is_ready: true }
    }
    fn load_state(&mut self, _: &NodeState) {}
}

// ── Logger — pasa la señal sin modificar y la registra ───────────────────────

pub struct LoggerNode { id: String, tag: String }
impl LoggerNode { pub fn new(id: String, tag: String) -> Self { Self { id, tag } } }

#[async_trait]
impl NodeInstance for LoggerNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        if let Some(sig) = inputs.first() {
            tracing::debug!("[Logger:{}] {:?}", self.tag, sig);
            return Ok(Some(sig.clone()));
        }
        Ok(None)
    }
    fn save_state(&self) -> NodeState {
        NodeState {
            node_id:   self.id.clone(),
            node_type: "logger".into(),
            data:      serde_json::json!({ "tag": self.tag }),
            is_ready:  true,
        }
    }
    fn load_state(&mut self, _: &NodeState) {}
}

// ── Select — selecciona componentes del vector ────────────────────────────────

pub struct SelectNode { id: String, indices: Vec<usize> }
impl SelectNode { pub fn new(id: String, indices: Vec<usize>) -> Self { Self { id, indices } } }

#[async_trait]
impl NodeInstance for SelectNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let v = expect_vector(inputs.first().ok_or_else(|| anyhow::anyhow!("Select sin entrada"))?, &self.id)?;
        let out: Vec<f64> = self.indices.iter().filter_map(|&i| v.get(i).copied()).collect();
        Ok(Some(Signal::Vector(out)))
    }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "select".into(), data: serde_json::json!({}), is_ready: true }
    }
    fn load_state(&mut self, _: &NodeState) {}
}

// ── LinearScale — y = a*x + b ─────────────────────────────────────────────────

pub struct LinearScaleNode { id: String, a: VecOrScalar, b: VecOrScalar }
impl LinearScaleNode {
    pub fn new(id: String, a: VecOrScalar, b: VecOrScalar) -> Self { Self { id, a, b } }
}

#[async_trait]
impl NodeInstance for LinearScaleNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let x = expect_vector(inputs.first().ok_or_else(|| anyhow::anyhow!("LinearScale sin entrada"))?, &self.id)?;
        let n = x.len();
        let a = self.a.expand(n);
        let b = self.b.expand(n);
        let out: Vec<f64> = x.iter().enumerate().map(|(i, &v)| a[i] * v + b[i]).collect();
        Ok(Some(Signal::Vector(out)))
    }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "linear_scale".into(), data: serde_json::json!({}), is_ready: true }
    }
    fn load_state(&mut self, _: &NodeState) {}
}