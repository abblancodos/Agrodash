use agrodash_shared::FilterState;
use super::Filter;

pub struct LowpassFilter {
    dim:    usize,
    tau:    Vec<f64>,
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
            x_hat:     self.y.clone(),
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
