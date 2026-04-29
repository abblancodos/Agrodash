// agent/src/filters/mod.rs

use agrodash_shared::{
    FilterConfig, FilterKind, FilterState,
};

pub mod kalman;
pub mod moving_avg;
pub mod ewma;
pub mod lowpass;

pub use kalman::KalmanFilter;
pub use moving_avg::MovingAvgFilter;
pub use ewma::EwmaFilter;
pub use lowpass::LowpassFilter;

// ── Trait ─────────────────────────────────────────────────────────────────────

pub trait Filter: Send {
    /// Dimensión del vector de entrada/salida.
    fn dim(&self) -> usize;

    /// Actualizar con nueva medición z. dt en segundos desde la última llamada.
    /// Retorna el vector filtrado.
    fn update(&mut self, z: &[f64], dt: f64) -> Vec<f64>;

    /// Retorna la diagonal de P si el filtro es Kalman, None si no.
    fn p_diag(&self) -> Option<Vec<f64>> { None }

    /// Si el filtro ya completó el warmup.
    fn is_ready(&self) -> bool;

    /// Serializar estado para persistir en DB.
    fn save_state(&self) -> FilterState;

    /// Restaurar estado desde DB (warmstart).
    fn load_state(&mut self, state: &FilterState);
}

// ── Factory ───────────────────────────────────────────────────────────────────

pub fn build(cfg: &FilterConfig, dim: usize) -> Box<dyn Filter> {
    match &cfg.kind {
        FilterKind::Kalman(p) => {
            let f = KalmanFilter::new(dim, p, cfg.warmup_samples);
            Box::new(f)
        }
        FilterKind::MovingAvg(p) => {
            Box::new(MovingAvgFilter::new(dim, p.window_n, cfg.warmup_samples))
        }
        FilterKind::Ewma(p) => {
            let alpha = p.alpha.expand(dim);
            Box::new(EwmaFilter::new(dim, alpha, cfg.warmup_samples))
        }
        FilterKind::Lowpass(p) => {
            let tau = p.tau_seconds.expand(dim);
            Box::new(LowpassFilter::new(dim, tau, cfg.warmup_samples))
        }
        FilterKind::Passthrough => {
            Box::new(PassthroughFilter { dim, n: 0, warmup: cfg.warmup_samples })
        }
    }
}

// ── Passthrough ───────────────────────────────────────────────────────────────

pub struct PassthroughFilter {
    dim:    usize,
    n:      usize,
    warmup: usize,
}

impl Filter for PassthroughFilter {
    fn dim(&self) -> usize { self.dim }

    fn update(&mut self, z: &[f64], _dt: f64) -> Vec<f64> {
        self.n += 1;
        z.to_vec()
    }

    fn is_ready(&self) -> bool { self.n >= self.warmup.max(1) }

    fn save_state(&self) -> FilterState {
        FilterState {
            n_updates: self.n as u64,
            is_ready:  self.is_ready(),
            ..Default::default()
        }
    }

    fn load_state(&mut self, state: &FilterState) {
        self.n = state.n_updates as usize;
    }
}
