// agent/src/nodes/decisions.rs

use async_trait::async_trait;
use agrodash_shared::{
    NodeState, Signal, NodeAction,
    MahalanobisConfig, HysteresisConfig, SprtConfig, Reduction,
};
use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use super::{NodeInstance, expect_vector};

// ── Mahalanobis ───────────────────────────────────────────────────────────────

pub struct MahalanobisNode {
    id:          String,
    cfg:         MahalanobisConfig,
    hyst:        Option<NodeAction>,
    last_d:      Option<f64>,
    last_change: Option<String>,
    // P diagonal del Kalman upstream (inyectada via señal extendida)
    last_p:      Option<Vec<f64>>,
}

impl MahalanobisNode {
    pub fn new(id: String, cfg: MahalanobisConfig) -> Self {
        Self { id, cfg, hyst: None, last_d: None, last_change: None, last_p: None }
    }

    fn distance(&self, x: &[f64], p: Option<&[f64]>) -> f64 {
        // target.len() >= x.len() ya verificado en execute() antes de llamar acá
        let n = x.len().min(self.cfg.target.len());
        let sigma_inv: Vec<f64> = (0..n).map(|i| {
            if self.cfg.use_kalman_P {
                if let Some(p) = p.filter(|p| i < p.len()) {
                    return 1.0 / p[i].max(1e-12);
                }
            }
            let s = self.cfg.sigma.unwrap_or(1.0).max(1e-12);
            1.0 / (s * s)
        }).collect();
        (0..n).map(|i| { let d = x[i] - self.cfg.target[i]; d * d * sigma_inv[i] }).sum::<f64>().sqrt()
    }
}

#[async_trait]
impl NodeInstance for MahalanobisNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let x = expect_vector(inputs.first().ok_or_else(|| anyhow::anyhow!("Mahalanobis sin entrada"))?, &self.id)?;
        if x.is_empty() {
            anyhow::bail!("Mahalanobis '{}': vector de entrada vacío", self.id);
        }
        if self.cfg.target.is_empty() {
            anyhow::bail!("Mahalanobis '{}': target no configurado (len=0) — revisá la config del pipeline", self.id);
        }
        if x.len() != self.cfg.target.len() {
            anyhow::bail!("Mahalanobis '{}': dimensión entrada ({}) != dimensión target ({})",
                self.id, x.len(), self.cfg.target.len());
        }
        let d = self.distance(&x, self.last_p.as_deref());
        self.last_d = Some(d);
        let prev = self.hyst.clone();
        let action = if d > self.cfg.threshold_act { NodeAction::On }
            else if d < self.cfg.threshold_deact { NodeAction::Off }
            else { self.hyst.clone().unwrap_or(NodeAction::Hold) };
        if Some(&action) != prev.as_ref() { self.last_change = Some(Utc::now().to_rfc3339()); }
        self.hyst = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }

    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "mahalanobis".into(),
            data: serde_json::json!({ "hyst": self.hyst, "last_d": self.last_d, "last_change": self.last_change }),
            is_ready: true }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.hyst        = state.data.get("hyst").and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_d      = state.data.get("last_d").and_then(|v| v.as_f64());
        self.last_change = state.data.get("last_change").and_then(|v| v.as_str().map(String::from));
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        let mut m = vec![];
        if let Some(d) = self.last_d { m.push(("d".into(), d)); }
        if let Some(h) = &self.hyst {
            m.push(("decision".into(), match h { NodeAction::On => 1.0, _ => 0.0 }));
        }
        m
    }
}

// ── Hysteresis ────────────────────────────────────────────────────────────────

pub struct HysteresisNode { id: String, cfg: HysteresisConfig, state: Option<NodeAction>, last_change: Option<String> }

impl HysteresisNode {
    pub fn new(id: String, cfg: HysteresisConfig) -> Self {
        Self { id, cfg, state: None, last_change: None }
    }
}

fn reduce(x: &[f64], r: &Reduction) -> f64 {
    match r {
        Reduction::Mean          => x.iter().sum::<f64>() / x.len() as f64,
        Reduction::Min           => x.iter().cloned().fold(f64::INFINITY, f64::min),
        Reduction::Max           => x.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        Reduction::Component { index } => x[*index],
        Reduction::WeightedByP   => x.iter().sum::<f64>() / x.len() as f64, // fallback
        Reduction::CountBelow { threshold, min_count } => {
            if x.iter().filter(|&&v| v < *threshold).count() >= *min_count { 1.0 } else { 0.0 }
        }
        Reduction::CountAbove { threshold, min_count } => {
            if x.iter().filter(|&&v| v > *threshold).count() >= *min_count { 1.0 } else { 0.0 }
        }
    }
}

#[async_trait]
impl NodeInstance for HysteresisNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let x = expect_vector(inputs.first().ok_or_else(|| anyhow::anyhow!("Hysteresis sin entrada"))?, &self.id)?;
        let val = reduce(&x, &self.cfg.reduction);
        let prev = self.state.clone();
        let action = if val < self.cfg.low { self.cfg.action_below_low.clone() }
            else if val > self.cfg.high { self.cfg.action_above_high.clone() }
            else { self.state.clone().unwrap_or(NodeAction::Hold) };
        if Some(&action) != prev.as_ref() { self.last_change = Some(Utc::now().to_rfc3339()); }
        self.state = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "hysteresis".into(),
            data: serde_json::json!({ "state": self.state, "last_change": self.last_change }),
            is_ready: true }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.state       = state.data.get("state").and_then(|v| serde_json::from_value(v.clone()).ok());
        self.last_change = state.data.get("last_change").and_then(|v| v.as_str().map(String::from));
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        if let Some(s) = &self.state {
            vec![("decision".into(), match s { NodeAction::On => 1.0, _ => 0.0 })]
        } else { vec![] }
    }
}

// ── SPRT ──────────────────────────────────────────────────────────────────────

pub struct SprtNode {
    id:     String, cfg: SprtConfig,
    log_a:  f64, log_b: f64, llr: f64,
    last:   Option<NodeAction>, last_change: Option<String>, samples: u64,
}

impl SprtNode {
    pub fn new(id: String, cfg: SprtConfig) -> Self {
        let log_a = ((1.0 - cfg.beta) / cfg.alpha).ln();
        let log_b = (cfg.beta / (1.0 - cfg.alpha)).ln();
        Self { id, cfg, log_a, log_b, llr: 0.0, last: None, last_change: None, samples: 0 }
    }
}

fn gaussian_log_pdf(x: f64, mu: f64, sigma: f64) -> f64 { let z = (x - mu) / sigma; -0.5 * z * z - sigma.ln() }

#[async_trait]
impl NodeInstance for SprtNode {
    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>> {
        let x = expect_vector(inputs.first().ok_or_else(|| anyhow::anyhow!("SPRT sin entrada"))?, &self.id)?;
        let val = reduce(&x, &self.cfg.reduction);
        self.samples += 1;
        self.llr += gaussian_log_pdf(val, self.cfg.mu_H1, self.cfg.sigma)
                  - gaussian_log_pdf(val, self.cfg.mu_H0, self.cfg.sigma);
        let prev = self.last.clone();
        let action = if self.llr >= self.log_a {
            if self.cfg.reset_on_action { self.llr = 0.0; self.samples = 0; }
            NodeAction::On
        } else if self.llr <= self.log_b {
            if self.cfg.reset_on_action { self.llr = 0.0; self.samples = 0; }
            NodeAction::Off
        } else {
            self.last.clone().unwrap_or(NodeAction::Hold)
        };
        if Some(&action) != prev.as_ref() { self.last_change = Some(Utc::now().to_rfc3339()); }
        self.last = Some(action.clone());
        Ok(Some(Signal::Action(action)))
    }
    fn save_state(&self) -> NodeState {
        NodeState { node_id: self.id.clone(), node_type: "sprt".into(),
            data: serde_json::json!({ "llr": self.llr, "last": self.last, "samples": self.samples }),
            is_ready: true }
    }
    fn load_state(&mut self, state: &NodeState) {
        self.llr     = state.data.get("llr").and_then(|v| v.as_f64()).unwrap_or(0.0);
        self.last    = state.data.get("last").and_then(|v| serde_json::from_value(v.clone()).ok());
        self.samples = state.data.get("samples").and_then(|v| v.as_u64()).unwrap_or(0);
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        let mut m = vec![("llr".into(), self.llr)];
        if let Some(l) = &self.last {
            m.push(("decision".into(), match l { NodeAction::On => 1.0, _ => 0.0 }));
        }
        m
    }
}