// agent/src/nodes/decisions.rs

use super::{expect_vector, NodeInstance};
use agrodash_shared::{
    HysteresisConfig, MahalanobisConfig, NodeAction, NodeState, Reduction, Signal, SprtConfig,
    TrendConfig,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::collections::VecDeque;

// ── TrendTracker — compartido por todos los decisores ─────────────────────────
//
// Mantiene una ventana deslizante de las últimas N muestras del vector de entrada
// y calcula la derivada discreta por componente: (last - first) / N.
// Expone trend[] en el NodeState.data del decisor para que el Watchdog en
// modo Bad pueda leerlo sin necesidad de un nodo extra en el grafo.

struct TrendTracker {
    cfg: TrendConfig,
    buf: VecDeque<Vec<f64>>,
    window: usize,
}

impl TrendTracker {
    fn new(cfg: TrendConfig, fallback_window: usize) -> Self {
        let window = cfg.window_n.unwrap_or(fallback_window).max(2);
        Self {
            cfg,
            buf: VecDeque::with_capacity(window),
            window,
        }
    }

    fn push(&mut self, x: &[f64]) {
        if self.buf.len() == self.window {
            self.buf.pop_front();
        }
        self.buf.push_back(x.to_vec());
    }

    /// Derivada discreta por componente: (last[i] - first[i]) / window.
    /// Componentes cuyo delta absoluto esté por debajo de noise_floor → 0.0.
    /// Devuelve None si no hay suficientes muestras todavía.
    fn trend(&self) -> Option<Vec<f64>> {
        if self.buf.len() < 2 {
            return None;
        }
        let first = self.buf.front()?;
        let last = self.buf.back()?;
        let n = self.buf.len() as f64;
        let t: Vec<f64> = first
            .iter()
            .zip(last.iter())
            .map(|(f, l)| {
                let d = l - f;
                if d.abs() < self.cfg.noise_floor {
                    0.0
                } else {
                    d / n
                }
            })
            .collect();
        Some(t)
    }

    fn save(&self) -> serde_json::Value {
        serde_json::json!({
            "trend":         self.trend(),
            "trend_window":  self.window,
            "trend_samples": self.buf.len(),
        })
    }

    fn load(&mut self, data: &serde_json::Value) {
        // Restauramos el buffer serializado si está disponible.
        // En caso contrario el tracker simplemente vuelve a acumular.
        if let Some(buf) = data
            .get("_trend_buf")
            .and_then(|v| serde_json::from_value::<Vec<Vec<f64>>>(v.clone()).ok())
        {
            self.buf = buf.into_iter().collect();
        }
    }

    fn save_buf(&self) -> serde_json::Value {
        serde_json::to_value(self.buf.iter().collect::<Vec<_>>()).unwrap_or(serde_json::Value::Null)
    }
}

// ── Mahalanobis ───────────────────────────────────────────────────────────────

pub struct MahalanobisNode {
    id: String,
    cfg: MahalanobisConfig,
    hyst: Option<NodeAction>,
    last_d: Option<f64>,
    last_change: Option<String>,
    last_p: Option<Vec<f64>>,
    trend: Option<TrendTracker>,
}

impl MahalanobisNode {
    pub fn new(id: String, cfg: MahalanobisConfig) -> Self {
        let trend = cfg.trend.clone().map(|t| TrendTracker::new(t, 10));
        Self {
            id,
            cfg,
            hyst: None,
            last_d: None,
            last_change: None,
            last_p: None,
            trend,
        }
    }

    fn distance(&self, x: &[f64], p: Option<&[f64]>) -> f64 {
        let n = x.len().min(self.cfg.target.len());
        let sigma_inv: Vec<f64> = (0..n)
            .map(|i| {
                if self.cfg.use_kalman_P {
                    if let Some(p) = p.filter(|p| i < p.len()) {
                        return 1.0 / p[i].max(1e-12);
                    }
                }
                let s = self.cfg.sigma.unwrap_or(1.0).max(1e-12);
                1.0 / (s * s)
            })
            .collect();
        (0..n)
            .map(|i| {
                let d = x[i] - self.cfg.target[i];
                d * d * sigma_inv[i]
            })
            .sum::<f64>()
            .sqrt()
    }
}

#[async_trait]
impl NodeInstance for MahalanobisNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let x = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("Mahalanobis sin entrada"))?,
            &self.id,
        )?;
        if x.is_empty() {
            anyhow::bail!("Mahalanobis '{}': vector vacío", self.id);
        }
        if self.cfg.target.is_empty() {
            anyhow::bail!("Mahalanobis '{}': target vacío", self.id);
        }
        if x.len() != self.cfg.target.len() {
            anyhow::bail!(
                "Mahalanobis '{}': dim entrada ({}) != dim target ({})",
                self.id,
                x.len(),
                self.cfg.target.len()
            );
        }
        let d = self.distance(&x, self.last_p.as_deref());
        self.last_d = Some(d);
        if let Some(t) = &mut self.trend {
            t.push(&x);
        }
        let prev = self.hyst.clone();
        let action = if d > self.cfg.threshold_act {
            NodeAction::On
        } else if d < self.cfg.threshold_deact {
            NodeAction::Off
        } else {
            self.hyst.clone().unwrap_or(NodeAction::Hold)
        };
        if Some(&action) != prev.as_ref() {
            self.last_change = Some(Utc::now().to_rfc3339());
            tracing::info!("[Mahalanobis:{}] d={:.4} → {:?}", self.id, d, action);
        }
        self.hyst = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }

    fn save_state(&self) -> NodeState {
        let mut data = serde_json::json!({
            "hyst": self.hyst, "last_d": self.last_d, "last_change": self.last_change,
        });
        if let Some(t) = &self.trend {
            if let Some(obj) = data.as_object_mut() {
                let saved = t.save();
                obj.insert("trend".into(), saved["trend"].clone());
                obj.insert("trend_window".into(), saved["trend_window"].clone());
                obj.insert("trend_samples".into(), saved["trend_samples"].clone());
                obj.insert("_trend_buf".into(), t.save_buf());
            }
        }
        NodeState {
            node_id: self.id.clone(),
            node_type: "mahalanobis".into(),
            data,
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
        self.hyst = state
            .data
            .get("hyst")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_d = state.data.get("last_d").and_then(|v| v.as_f64());
        self.last_change = state
            .data
            .get("last_change")
            .and_then(|v| v.as_str().map(String::from));
        if let Some(t) = &mut self.trend {
            t.load(&state.data);
        }
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        let mut m = vec![];
        if let Some(d) = self.last_d {
            m.push(("d".into(), d));
        }
        if let Some(h) = &self.hyst {
            m.push((
                "decision".into(),
                match h {
                    NodeAction::On => 1.0,
                    _ => 0.0,
                },
            ));
        }
        m
    }
}

// ── Hysteresis ────────────────────────────────────────────────────────────────

pub struct HysteresisNode {
    id: String,
    cfg: HysteresisConfig,
    state: Option<NodeAction>,
    last_change: Option<String>,
    last_val: Option<f64>,
    trend: Option<TrendTracker>,
}

impl HysteresisNode {
    pub fn new(id: String, cfg: HysteresisConfig) -> Self {
        let trend = cfg.trend.clone().map(|t| TrendTracker::new(t, 10));
        Self {
            id,
            cfg,
            state: None,
            last_change: None,
            last_val: None,
            trend,
        }
    }
}

pub fn reduce(x: &[f64], r: &Reduction) -> f64 {
    match r {
        Reduction::Mean => x.iter().sum::<f64>() / x.len() as f64,
        Reduction::Min => x.iter().cloned().fold(f64::INFINITY, f64::min),
        Reduction::Max => x.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        Reduction::Component { index } => x[*index],
        Reduction::WeightedByP => x.iter().sum::<f64>() / x.len() as f64,
        Reduction::CountBelow {
            threshold,
            min_count,
        } => {
            if x.iter().filter(|&&v| v < *threshold).count() >= *min_count {
                1.0
            } else {
                0.0
            }
        }
        Reduction::CountAbove {
            threshold,
            min_count,
        } => {
            if x.iter().filter(|&&v| v > *threshold).count() >= *min_count {
                1.0
            } else {
                0.0
            }
        }
    }
}

#[async_trait]
impl NodeInstance for HysteresisNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let x = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("Hysteresis sin entrada"))?,
            &self.id,
        )?;
        let val = reduce(&x, &self.cfg.reduction);
        if let Some(t) = &mut self.trend {
            t.push(&x);
        }
        self.last_val = Some(val);
        let prev = self.state.clone();
        let action = if val < self.cfg.low {
            self.cfg.action_below_low.clone()
        } else if val > self.cfg.high {
            self.cfg.action_above_high.clone()
        } else {
            self.state.clone().unwrap_or(NodeAction::Hold)
        };
        if Some(&action) != prev.as_ref() {
            self.last_change = Some(Utc::now().to_rfc3339());
            tracing::info!(
                "[Hysteresis:{}] val={:.4} → {:?} (low={}, high={})",
                self.id,
                val,
                action,
                self.cfg.low,
                self.cfg.high
            );
        }
        self.state = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }

    fn save_state(&self) -> NodeState {
        let mut data = serde_json::json!({ "state": self.state, "last_change": self.last_change });
        if let Some(t) = &self.trend {
            if let Some(obj) = data.as_object_mut() {
                let saved = t.save();
                obj.insert("trend".into(), saved["trend"].clone());
                obj.insert("trend_window".into(), saved["trend_window"].clone());
                obj.insert("trend_samples".into(), saved["trend_samples"].clone());
                obj.insert("_trend_buf".into(), t.save_buf());
            }
        }
        NodeState {
            node_id: self.id.clone(),
            node_type: "hysteresis".into(),
            data,
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
        self.state = state
            .data
            .get("state")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_change = state
            .data
            .get("last_change")
            .and_then(|v| v.as_str().map(String::from));
        if let Some(t) = &mut self.trend {
            t.load(&state.data);
        }
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        let mut m = vec![("low".into(), self.cfg.low), ("high".into(), self.cfg.high)];
        if let Some(v) = self.last_val {
            m.push(("val".into(), v));
        }
        if let Some(s) = &self.state {
            m.push((
                "decision".into(),
                match s {
                    NodeAction::On => 1.0,
                    _ => 0.0,
                },
            ));
        }
        m
    }
}

// ── SPRT ──────────────────────────────────────────────────────────────────────

pub struct SprtNode {
    id: String,
    cfg: SprtConfig,
    log_a: f64,
    log_b: f64,
    llr: f64,
    last: Option<NodeAction>,
    last_change: Option<String>,
    samples: u64,
    trend: Option<TrendTracker>,
}

impl SprtNode {
    pub fn new(id: String, cfg: SprtConfig) -> Self {
        let log_a = ((1.0 - cfg.beta) / cfg.alpha).ln();
        let log_b = (cfg.beta / (1.0 - cfg.alpha)).ln();
        let trend = cfg.trend.clone().map(|t| TrendTracker::new(t, 10));
        Self {
            id,
            cfg,
            log_a,
            log_b,
            llr: 0.0,
            last: None,
            last_change: None,
            samples: 0,
            trend,
        }
    }
}

fn gaussian_log_pdf(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    -0.5 * z * z - sigma.ln()
}

#[async_trait]
impl NodeInstance for SprtNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
        _ctx: &super::NodeContext,
    ) -> Result<Option<Signal>> {
        let x = expect_vector(
            inputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("SPRT sin entrada"))?,
            &self.id,
        )?;
        let val = reduce(&x, &self.cfg.reduction);
        if let Some(t) = &mut self.trend {
            t.push(&x);
        }
        self.samples += 1;
        self.llr += gaussian_log_pdf(val, self.cfg.mu_H1, self.cfg.sigma)
            - gaussian_log_pdf(val, self.cfg.mu_H0, self.cfg.sigma);
        let prev = self.last.clone();
        let action = if self.llr >= self.log_a {
            if self.cfg.reset_on_action {
                self.llr = 0.0;
                self.samples = 0;
            }
            NodeAction::On
        } else if self.llr <= self.log_b {
            if self.cfg.reset_on_action {
                self.llr = 0.0;
                self.samples = 0;
            }
            NodeAction::Off
        } else {
            self.last.clone().unwrap_or(NodeAction::Hold)
        };
        if Some(&action) != prev.as_ref() {
            self.last_change = Some(Utc::now().to_rfc3339());
        }
        self.last = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }

    fn save_state(&self) -> NodeState {
        let mut data = serde_json::json!({
            "llr": self.llr, "last": self.last, "samples": self.samples,
        });
        if let Some(t) = &self.trend {
            if let Some(obj) = data.as_object_mut() {
                let saved = t.save();
                obj.insert("trend".into(), saved["trend"].clone());
                obj.insert("trend_window".into(), saved["trend_window"].clone());
                obj.insert("trend_samples".into(), saved["trend_samples"].clone());
                obj.insert("_trend_buf".into(), t.save_buf());
            }
        }
        NodeState {
            node_id: self.id.clone(),
            node_type: "sprt".into(),
            data,
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
        self.llr = state
            .data
            .get("llr")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        self.last = state
            .data
            .get("last")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        self.samples = state
            .data
            .get("samples")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if let Some(t) = &mut self.trend {
            t.load(&state.data);
        }
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        let mut m = vec![("llr".into(), self.llr)];
        if let Some(l) = &self.last {
            m.push((
                "decision".into(),
                match l {
                    NodeAction::On => 1.0,
                    _ => 0.0,
                },
            ));
        }
        m
    }
}
