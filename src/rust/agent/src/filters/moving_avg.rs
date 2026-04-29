// agent/src/filters/moving_avg.rs
use agrodash_shared::FilterState;
use super::Filter;
use std::collections::VecDeque;

pub struct MovingAvgFilter {
    dim:    usize,
    window: usize,
    warmup: usize,
    buf:    VecDeque<Vec<f64>>,
    n:      usize,
}

impl MovingAvgFilter {
    pub fn new(dim: usize, window: usize, warmup: usize) -> Self {
        Self { dim, window, warmup, buf: VecDeque::with_capacity(window), n: 0 }
    }
}

impl Filter for MovingAvgFilter {
    fn dim(&self) -> usize { self.dim }

    fn update(&mut self, z: &[f64], _dt: f64) -> Vec<f64> {
        if self.buf.len() == self.window { self.buf.pop_front(); }
        self.buf.push_back(z.to_vec());
        self.n += 1;

        let mut mean = vec![0.0f64; self.dim];
        for sample in &self.buf {
            for (i, &v) in sample.iter().enumerate() { mean[i] += v; }
        }
        let n = self.buf.len() as f64;
        mean.iter_mut().for_each(|v| *v /= n);
        mean
    }

    fn is_ready(&self) -> bool {
        self.n >= self.warmup && self.buf.len() == self.window
    }

    fn save_state(&self) -> FilterState {
        FilterState {
            x_hat:    Some(self.buf.back().cloned().unwrap_or_default()),
            n_updates: self.n as u64,
            is_ready:  self.is_ready(),
            buffer:   Some(self.buf.iter().cloned().collect()),
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &FilterState) {
        self.n = state.n_updates as usize;
        if let Some(buf) = &state.buffer {
            self.buf = buf.iter().cloned().collect();
        }
    }
}


// ── EWMA ──────────────────────────────────────────────────────────────────────
// agent/src/filters/ewma.rs — incluido aquí para brevedad

pub struct EwmaFilter {
    dim:    usize,
    alpha:  Vec<f64>,
    warmup: usize,
    x:      Option<Vec<f64>>,
    n:      usize,
}

impl EwmaFilter {
    pub fn new(dim: usize, alpha: Vec<f64>, warmup: usize) -> Self {
        Self { dim, alpha, warmup, x: None, n: 0 }
    }
}

impl Filter for EwmaFilter {
    fn dim(&self) -> usize { self.dim }

    fn update(&mut self, z: &[f64], _dt: f64) -> Vec<f64> {
        let x = self.x.get_or_insert_with(|| z.to_vec());
        for i in 0..self.dim {
            x[i] = self.alpha[i] * z[i] + (1.0 - self.alpha[i]) * x[i];
        }
        self.n += 1;
        x.clone()
    }

    fn is_ready(&self) -> bool { self.n >= self.warmup }

    fn save_state(&self) -> FilterState {
        FilterState {
            x_hat:    self.x.clone(),
            n_updates: self.n as u64,
            is_ready:  self.is_ready(),
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &FilterState) {
        self.x = state.x_hat.clone();
        self.n = state.n_updates as usize;
    }
}


// ── Lowpass (RC discreto) ─────────────────────────────────────────────────────
// agent/src/filters/lowpass.rs — incluido aquí para brevedad
//
// Implementación discreta del filtro paso bajo de primer orden:
//   α = dt / (τ + dt)
//   y_k = α · x_k + (1 - α) · y_{k-1}

pub struct LowpassFilter {
    dim:    usize,
    tau:    Vec<f64>,    // constante de tiempo RC por componente
    warmup: usize,
    y:      Option<Vec<f64>>,
    n:      usize,
}

impl LowpassFilter {
    pub fn new(dim: usize, tau: Vec<f64>, warmup: usize) -> Self {
        Self { dim, tau, warmup, y: None, n: 0 }
    }
}

impl Filter for LowpassFilter {
    fn dim(&self) -> usize { self.dim }

    fn update(&mut self, z: &[f64], dt: f64) -> Vec<f64> {
        let y = self.y.get_or_insert_with(|| z.to_vec());
        for i in 0..self.dim {
            let alpha = dt / (self.tau[i] + dt);
            y[i] = alpha * z[i] + (1.0 - alpha) * y[i];
        }
        self.n += 1;
        y.clone()
    }

    fn is_ready(&self) -> bool { self.n >= self.warmup }

    fn save_state(&self) -> FilterState {
        FilterState {
            x_hat:    self.y.clone(),
            n_updates: self.n as u64,
            is_ready:  self.is_ready(),
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &FilterState) {
        self.y = state.x_hat.clone();
        self.n = state.n_updates as usize;
    }
}
