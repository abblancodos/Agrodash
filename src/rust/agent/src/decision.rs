// agent/src/decision.rs

use agrodash_shared::{
    ActuatorAction, DecisionConfig, DecisionKind, DecisionState,
    MahalanobisParams, HysteresisParams, Reduction,
};
use chrono::Utc;

// ── Output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DecisionOutput {
    On,
    Off,
    Hold,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

pub trait Decision: Send {
    /// Evalúa el vector filtrado.
    /// p_diag: diagonal de P del Kalman si está disponible.
    fn evaluate(
        &mut self,
        x_hat:  &[f64],
        p_diag: Option<&[f64]>,
    ) -> DecisionOutput;

    fn state(&self) -> DecisionState;
    fn load_state(&mut self, state: &DecisionState);
}

// ── Factory ───────────────────────────────────────────────────────────────────

pub fn build(cfg: &DecisionConfig) -> Box<dyn Decision> {
    match &cfg.kind {
        DecisionKind::Mahalanobis(p) => Box::new(MahalanobisDecision::new(p.clone())),
        DecisionKind::Hysteresis(p)  => Box::new(HysteresisDecision::new(p.clone())),
        DecisionKind::Sprt(p)        => Box::new(SprtDecision::new(
            p.mu_H0, p.mu_H1, p.sigma, p.alpha, p.beta,
            p.reduction.clone(), p.reset_on_action,
        )),
    }
}

// ── Mahalanobis ───────────────────────────────────────────────────────────────
//
// d²(x) = (x - target)ᵀ · Σ⁻¹ · (x - target)
//
// Con covarianza diagonal Σ = diag(P) o Σ = σ²·I:
//   d²(x) = Σᵢ (x_i - target_i)² / σᵢ²
//
// Histéresis estadística:
//   d > threshold_act   → On
//   d < threshold_deact → Off
//   entre ambos         → Hold

pub struct MahalanobisDecision {
    params:     MahalanobisParams,
    hyst_state: Option<ActuatorAction>,
    last_d:     Option<f64>,
    last_change: Option<String>,
}

impl MahalanobisDecision {
    pub fn new(params: MahalanobisParams) -> Self {
        Self { params, hyst_state: None, last_d: None, last_change: None }
    }

    fn compute_distance(&self, x_hat: &[f64], p_diag: Option<&[f64]>) -> f64 {
        let n = x_hat.len();
        let target = &self.params.target;

        // Σ⁻¹ diagonal — usamos P si está disponible y use_kalman_P=true
        let sigma_inv: Vec<f64> = (0..n).map(|i| {
            if self.params.use_kalman_P {
                if let Some(p) = p_diag {
                    let p_i = p[i].max(1e-12);  // evitar división por cero
                    return 1.0 / p_i;
                }
            }
            let s = self.params.sigma.unwrap_or(1.0).max(1e-12);
            1.0 / (s * s)
        }).collect();

        // d² = Σ (x_i - target_i)² · sigma_inv_i
        let d_sq: f64 = (0..n)
            .map(|i| {
                let diff = x_hat[i] - target[i];
                diff * diff * sigma_inv[i]
            })
            .sum();

        d_sq.sqrt()
    }
}

impl Decision for MahalanobisDecision {
    fn evaluate(&mut self, x_hat: &[f64], p_diag: Option<&[f64]>) -> DecisionOutput {
        let d = self.compute_distance(x_hat, p_diag);
        self.last_d = Some(d);

        let prev = self.hyst_state.clone();

        let output = if d > self.params.threshold_act {
            DecisionOutput::On
        } else if d < self.params.threshold_deact {
            DecisionOutput::Off
        } else {
            // Zona de histéresis — mantener estado anterior
            match &self.hyst_state {
                Some(ActuatorAction::On)  => DecisionOutput::On,
                Some(ActuatorAction::Off) => DecisionOutput::Off,
                None                      => DecisionOutput::Hold,
            }
        };

        // Actualizar estado interno
        let new_state = match &output {
            DecisionOutput::On   => Some(ActuatorAction::On),
            DecisionOutput::Off  => Some(ActuatorAction::Off),
            DecisionOutput::Hold => self.hyst_state.clone(),
        };

        if new_state != prev {
            self.last_change = Some(Utc::now().to_rfc3339());
        }
        self.hyst_state = new_state;

        output
    }

    fn state(&self) -> DecisionState {
        DecisionState {
            hysteresis_state:  self.hyst_state.clone(),
            last_mahalanobis:  self.last_d,
            last_change_at:    self.last_change.clone(),
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &DecisionState) {
        self.hyst_state  = state.hysteresis_state.clone();
        self.last_d      = state.last_mahalanobis;
        self.last_change = state.last_change_at.clone();
    }
}

// ── Hysteresis escalar ────────────────────────────────────────────────────────

pub struct HysteresisDecision {
    params:     HysteresisParams,
    state_act:  Option<ActuatorAction>,
    last_val:   Option<f64>,
    last_change: Option<String>,
}

impl HysteresisDecision {
    pub fn new(params: HysteresisParams) -> Self {
        Self { params, state_act: None, last_val: None, last_change: None }
    }
}

impl Decision for HysteresisDecision {
    fn evaluate(&mut self, x_hat: &[f64], p_diag: Option<&[f64]>) -> DecisionOutput {
        let val = reduce(x_hat, p_diag, &self.params.reduction);
        self.last_val = Some(val);
        let prev = self.state_act.clone();

        let output = if val < self.params.low {
            match &self.params.action_below_low {
                ActuatorAction::On  => DecisionOutput::On,
                ActuatorAction::Off => DecisionOutput::Off,
            }
        } else if val > self.params.high {
            match &self.params.action_above_high {
                ActuatorAction::On  => DecisionOutput::On,
                ActuatorAction::Off => DecisionOutput::Off,
            }
        } else {
            match &self.state_act {
                Some(ActuatorAction::On)  => DecisionOutput::On,
                Some(ActuatorAction::Off) => DecisionOutput::Off,
                None                      => DecisionOutput::Hold,
            }
        };

        let new_state = match &output {
            DecisionOutput::On   => Some(ActuatorAction::On),
            DecisionOutput::Off  => Some(ActuatorAction::Off),
            DecisionOutput::Hold => self.state_act.clone(),
        };
        if new_state != prev { self.last_change = Some(Utc::now().to_rfc3339()); }
        self.state_act = new_state;
        output
    }

    fn state(&self) -> DecisionState {
        DecisionState {
            hysteresis_state: self.state_act.clone(),
            last_reduced:     self.last_val,
            last_change_at:   self.last_change.clone(),
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &DecisionState) {
        self.state_act   = state.hysteresis_state.clone();
        self.last_change = state.last_change_at.clone();
    }
}

// ── SPRT ──────────────────────────────────────────────────────────────────────

pub struct SprtDecision {
    mu_H0:    f64,
    mu_H1:    f64,
    sigma:    f64,
    log_A:    f64,   // log(1-beta / alpha)
    log_B:    f64,   // log(beta / (1-alpha))
    llr:      f64,   // log-likelihood ratio acumulado
    reduction: Reduction,
    reset_on_action: bool,
    last_action: Option<ActuatorAction>,
    last_change: Option<String>,
    samples:  u64,
}

impl SprtDecision {
    pub fn new(
        mu_H0: f64, mu_H1: f64, sigma: f64,
        alpha: f64, beta: f64,
        reduction: Reduction,
        reset_on_action: bool,
    ) -> Self {
        Self {
            mu_H0, mu_H1, sigma,
            log_A: ((1.0 - beta) / alpha).ln(),
            log_B: (beta / (1.0 - alpha)).ln(),
            llr: 0.0, reduction, reset_on_action,
            last_action: None, last_change: None, samples: 0,
        }
    }
}

impl Decision for SprtDecision {
    fn evaluate(&mut self, x_hat: &[f64], p_diag: Option<&[f64]>) -> DecisionOutput {
        let x = reduce(x_hat, p_diag, &self.reduction);
        self.samples += 1;

        // Incremento del log-likelihood ratio
        let num   = gaussian_log_pdf(x, self.mu_H1, self.sigma);
        let denom = gaussian_log_pdf(x, self.mu_H0, self.sigma);
        self.llr += num - denom;

        let prev = self.last_action.clone();
        let output = if self.llr >= self.log_A {
            if self.reset_on_action { self.llr = 0.0; self.samples = 0; }
            DecisionOutput::On
        } else if self.llr <= self.log_B {
            if self.reset_on_action { self.llr = 0.0; self.samples = 0; }
            DecisionOutput::Off
        } else {
            DecisionOutput::Hold
        };

        let new = match &output {
            DecisionOutput::On   => Some(ActuatorAction::On),
            DecisionOutput::Off  => Some(ActuatorAction::Off),
            DecisionOutput::Hold => self.last_action.clone(),
        };
        if new != prev { self.last_change = Some(Utc::now().to_rfc3339()); }
        self.last_action = new;
        output
    }

    fn state(&self) -> DecisionState {
        DecisionState {
            hysteresis_state: self.last_action.clone(),
            log_likelihood:   Some(self.llr),
            last_change_at:   self.last_change.clone(),
            evidence_samples: self.samples,
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &DecisionState) {
        self.llr         = state.log_likelihood.unwrap_or(0.0);
        self.last_action = state.hysteresis_state.clone();
        self.last_change = state.last_change_at.clone();
        self.samples     = state.evidence_samples;
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn reduce(x: &[f64], p_diag: Option<&[f64]>, r: &Reduction) -> f64 {
    match r {
        Reduction::Mean => x.iter().sum::<f64>() / x.len() as f64,
        Reduction::Min  => x.iter().cloned().fold(f64::INFINITY, f64::min),
        Reduction::Max  => x.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        Reduction::Component { index } => x[*index],
        Reduction::WeightedByP => {
            if let Some(p) = p_diag {
                let weights: Vec<f64> = p.iter().map(|&pi| 1.0 / pi.max(1e-12)).collect();
                let wsum: f64 = weights.iter().sum();
                x.iter().zip(weights.iter()).map(|(xi, wi)| xi * wi).sum::<f64>() / wsum
            } else {
                x.iter().sum::<f64>() / x.len() as f64
            }
        }
        Reduction::CountBelow { threshold, min_count } => {
            let count = x.iter().filter(|&&xi| xi < *threshold).count();
            if count >= *min_count { 1.0 } else { 0.0 }
        }
        Reduction::CountAbove { threshold, min_count } => {
            let count = x.iter().filter(|&&xi| xi > *threshold).count();
            if count >= *min_count { 1.0 } else { 0.0 }
        }
    }
}

fn gaussian_log_pdf(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    -0.5 * z * z - sigma.ln()
}
