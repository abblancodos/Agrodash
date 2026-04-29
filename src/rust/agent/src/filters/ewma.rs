use agrodash_shared::FilterState;
use super::Filter;

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
            x_hat:     self.x.clone(),
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
