// agent/src/nodes/filters.rs

use super::{expect_vector, NodeInstance};
use agrodash_shared::{
    EwmaConfig, KalmanConfig, LowpassConfig, MovingAvgConfig, NodeState, Signal,
};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use std::collections::VecDeque;

// ── Kalman ────────────────────────────────────────────────────────────────────

pub struct KalmanNode {
    id: String,
    cfg: KalmanConfig,
    dim: usize,
    x: Vec<f64>,
    p: Vec<f64>,
    q: Vec<f64>,
    r: Vec<f64>,
    n: usize,
    initialized: bool,
    last_k: Vec<f64>,
    last_innov: Vec<f64>,
}

impl KalmanNode {
    pub fn new(id: String, cfg: KalmanConfig) -> Self {
        // dim is unknown until first signal — initialize lazily
        Self {
            id,
            cfg,
            dim: 0,
            x: vec![],
            p: vec![],
            q: vec![],
            r: vec![],
            n: 0,
            initialized: false,
            last_k: vec![],
            last_innov: vec![],
        }
    }

    fn init(&mut self, dim: usize) {
        if self.initialized {
            return;
        }
        self.dim = dim;
        self.q = self.cfg.Q.expand(dim);
        self.r = self.cfg.R.expand(dim);
        self.p = vec![self.cfg.P0; dim];
        self.x = vec![0.0; dim];
        self.last_k = vec![0.0; dim];
        self.last_innov = vec![0.0; dim];
        self.initialized = true;
    }
}

#[async_trait]
impl NodeInstance for KalmanNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let z = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("Kalman sin entrada"))?,
            &self.id,
        )?;
        self.init(z.len());

        if self.n == 0 {
            self.x = z.clone();
        }

        #[allow(clippy::needless_range_loop)]
        for i in 0..self.dim {
            self.p[i] += self.q[i] * dt;
            let k = self.p[i] / (self.p[i] + self.r[i]);
            let innov = z[i] - self.x[i];
            self.x[i] += k * innov;
            self.p[i] *= 1.0 - k;
            self.last_k[i] = k;
            self.last_innov[i] = innov;
        }
        self.n += 1;
        // Emitir WeightedVector con pesos = 1/P[i] (mayor certeza → mayor peso).
        // Los nodos que no usan pesos (Logger, Hysteresis vieja, etc.) los ignoran.
        let weights = self
            .p
            .iter()
            .map(|&p| if p > 1e-12 { 1.0 / p } else { 1e12 })
            .collect();
        Ok(Some(Signal::WeightedVector {
            values: self.x.clone(),
            weights,
        }))
    }

    fn is_ready(&self) -> bool {
        self.n >= self.cfg.warmup_samples
            && self.p.iter().all(|&p| p < self.cfg.convergence_threshold)
    }

    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
            node_type: "kalman".into(),
            data: serde_json::json!({ "x": self.x, "p": self.p, "n": self.n }),
            is_ready: self.is_ready(),
        }
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        if self.dim == 0 || self.last_k.len() != self.dim {
            return vec![];
        }
        let mut m = Vec::new();
        for i in 0..self.dim {
            let suffix = if self.dim == 1 {
                String::new()
            } else {
                format!("[{i}]")
            };
            m.push((format!("p{suffix}"), self.p[i]));
            m.push((format!("k{suffix}"), self.last_k[i]));
            m.push((format!("innov{suffix}"), self.last_innov[i]));
        }
        m
    }

    fn load_state(&mut self, state: &NodeState) {
        if let Some(x) = state
            .data
            .get("x")
            .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok())
        {
            self.dim = x.len();
            self.x = x;
            self.initialized = true;
            self.q = self.cfg.Q.expand(self.dim);
            self.r = self.cfg.R.expand(self.dim);
            // Inicializar con longitud correcta para evitar index out of bounds
            // en el primer ciclo tras cargar estado guardado
            if self.last_k.len() != self.dim {
                self.last_k = vec![0.0; self.dim];
                self.last_innov = vec![0.0; self.dim];
            }
        }
        if let Some(p) = state
            .data
            .get("p")
            .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok())
        {
            self.p = p;
        }
        if let Some(n) = state.data.get("n").and_then(|v| v.as_u64()) {
            self.n = n as usize;
        }
    }
}

// ── MovingAvg ─────────────────────────────────────────────────────────────────

pub struct MovingAvgNode {
    id: String,
    cfg: MovingAvgConfig,
    buf: VecDeque<Vec<f64>>,
    n: usize,
    /// Varianza de la ventana por componente — usada como P para WeightedVector
    var: Vec<f64>,
}

impl MovingAvgNode {
    pub fn new(id: String, cfg: MovingAvgConfig) -> Self {
        Self {
            id,
            cfg,
            buf: VecDeque::new(),
            n: 0,
            var: vec![],
        }
    }
}

#[async_trait]
impl NodeInstance for MovingAvgNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let z = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("MovingAvg sin entrada"))?,
            &self.id,
        )?;
        if self.buf.len() == self.cfg.window_n {
            self.buf.pop_front();
        }
        self.buf.push_back(z.clone());
        self.n += 1;
        let dim = z.len();
        let mut mean = vec![0.0f64; dim];
        for s in &self.buf {
            for (i, &v) in s.iter().enumerate() {
                mean[i] += v;
            }
        }
        let n = self.buf.len() as f64;
        mean.iter_mut().for_each(|v| *v /= n);

        // Varianza de la ventana → peso = 1/var (mayor varianza = menor confianza)
        let dim = mean.len();
        if self.var.len() != dim {
            self.var = vec![1.0; dim];
        }
        for i in 0..dim {
            let v = self
                .buf
                .iter()
                .map(|s| (s[i] - mean[i]).powi(2))
                .sum::<f64>()
                / n;
            self.var[i] = v.max(1e-12);
        }
        let weights = self.var.iter().map(|&v| 1.0 / v).collect();
        Ok(Some(Signal::WeightedVector {
            values: mean,
            weights,
        }))
    }
    fn is_ready(&self) -> bool {
        self.n >= self.cfg.warmup_samples && self.buf.len() == self.cfg.window_n
    }
    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
            node_type: "moving_avg".into(),
            data: serde_json::json!({ "buf": self.buf, "n": self.n }),
            is_ready: self.is_ready(),
        }
    }
    fn load_state(&mut self, state: &NodeState) {
        if let Some(buf) = state
            .data
            .get("buf")
            .and_then(|v| serde_json::from_value::<Vec<Vec<f64>>>(v.clone()).ok())
        {
            self.buf = buf.into_iter().collect();
        }
        if let Some(n) = state.data.get("n").and_then(|v| v.as_u64()) {
            self.n = n as usize;
        }
    }
}

// ── EWMA ──────────────────────────────────────────────────────────────────────

pub struct EwmaNode {
    id: String,
    cfg: EwmaConfig,
    alpha: Vec<f64>,
    y: Option<Vec<f64>>,
    /// Varianza exponencial running por componente (Welford online para EWMA)
    var: Vec<f64>,
    n: usize,
}

impl EwmaNode {
    pub fn new(id: String, cfg: EwmaConfig) -> Self {
        Self {
            id,
            cfg,
            alpha: vec![],
            y: None,
            var: vec![],
            n: 0,
        }
    }
}

#[async_trait]
impl NodeInstance for EwmaNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let z = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("EWMA sin entrada"))?,
            &self.id,
        )?;
        if self.alpha.is_empty() {
            self.alpha = self.cfg.alpha.expand(z.len());
        }
        if self.var.len() != z.len() {
            self.var = vec![1.0; z.len()];
        }
        let y = self.y.get_or_insert_with(|| z.clone());
        #[allow(clippy::needless_range_loop)]
        for i in 0..z.len() {
            let innov = z[i] - y[i];
            // Varianza exponencial: V = α*innov² + (1-α)*V
            self.var[i] = self.alpha[i] * innov * innov + (1.0 - self.alpha[i]) * self.var[i];
            self.var[i] = self.var[i].max(1e-12);
            y[i] = self.alpha[i] * z[i] + (1.0 - self.alpha[i]) * y[i];
        }
        self.n += 1;
        let weights = self.var.iter().map(|&v| 1.0 / v).collect();
        Ok(Some(Signal::WeightedVector {
            values: y.clone(),
            weights,
        }))
    }
    fn is_ready(&self) -> bool {
        self.n >= self.cfg.warmup_samples
    }
    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
            node_type: "ewma".into(),
            data: serde_json::json!({ "y": self.y, "var": self.var, "n": self.n }),
            is_ready: self.is_ready(),
        }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.y = state
            .data
            .get("y")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.var = state
            .data
            .get("var")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if let Some(n) = state.data.get("n").and_then(|v| v.as_u64()) {
            self.n = n as usize;
        }
    }
}

// ── Lowpass ───────────────────────────────────────────────────────────────────

pub struct LowpassNode {
    id: String,
    cfg: LowpassConfig,
    tau: Vec<f64>,
    y: Option<Vec<f64>>,
    var: Vec<f64>,
    n: usize,
}

impl LowpassNode {
    pub fn new(id: String, cfg: LowpassConfig) -> Self {
        Self {
            id,
            cfg,
            tau: vec![],
            y: None,
            var: vec![],
            n: 0,
        }
    }
}

#[async_trait]
impl NodeInstance for LowpassNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let z = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("Lowpass sin entrada"))?,
            &self.id,
        )?;
        if self.tau.is_empty() {
            self.tau = self.cfg.tau_seconds.expand(z.len());
        }
        if self.var.len() != z.len() {
            self.var = vec![1.0; z.len()];
        }
        let y = self.y.get_or_insert_with(|| z.clone());
        #[allow(clippy::needless_range_loop)]
        for i in 0..z.len() {
            let a = dt / (self.tau[i] + dt);
            let innov = z[i] - y[i];
            self.var[i] = a * innov * innov + (1.0 - a) * self.var[i];
            self.var[i] = self.var[i].max(1e-12);
            y[i] = a * z[i] + (1.0 - a) * y[i];
        }
        self.n += 1;
        let weights = self.var.iter().map(|&v| 1.0 / v).collect();
        Ok(Some(Signal::WeightedVector {
            values: y.clone(),
            weights,
        }))
    }
    fn is_ready(&self) -> bool {
        self.n >= self.cfg.warmup_samples
    }
    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
            node_type: "lowpass".into(),
            data: serde_json::json!({ "y": self.y, "var": self.var, "n": self.n }),
            is_ready: self.is_ready(),
        }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.y = state
            .data
            .get("y")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.var = state
            .data
            .get("var")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if let Some(n) = state.data.get("n").and_then(|v| v.as_u64()) {
            self.n = n as usize;
        }
    }
}

// ── Passthrough ───────────────────────────────────────────────────────────────

pub struct PassthroughNode {
    id: String,
}
impl PassthroughNode {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

#[async_trait]
impl NodeInstance for PassthroughNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        // Propaga la señal tal cual — incluyendo WeightedVector si viene del Kalman u otro filtro
        Ok(inputs.into_iter().next())
    }
    fn save_state(&self) -> NodeState {
        NodeState {
            node_id: self.id.clone(),
            node_type: "passthrough".into(),
            data: serde_json::json!({}),
            is_ready: true,
        }
    }
    fn load_state(&mut self, _: &NodeState) {}
}
