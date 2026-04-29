// agent/src/filters/kalman.rs
//
// Kalman de covarianza diagonal — suficiente para sensores independientes.
// Si las componentes estuvieran correlacionadas habría que usar matriz completa,
// pero para filas de suelo independientes la diagonal es correcta y mucho
// más eficiente (O(n) vs O(n²)).
//
// Modelo:
//   x_k = x_{k-1} + w_k         w_k ~ N(0, Q·dt)
//   z_k = x_k + v_k             v_k ~ N(0, R)
//
// Update diagonal:
//   K_i = P_i / (P_i + R_i)
//   x_i = x_i + K_i · (z_i - x_i)
//   P_i = (1 - K_i) · P_i

use agrodash_shared::{FilterState, KalmanParams};
use super::Filter;

pub struct KalmanFilter {
    dim:     usize,
    x:       Vec<f64>,   // estimado actual
    P:       Vec<f64>,   // covarianza diagonal
    Q:       Vec<f64>,   // varianza de proceso
    R:       Vec<f64>,   // varianza de medición
    P_conv:  f64,        // umbral de convergencia
    n:       usize,      // número de updates
    warmup:  usize,
}

impl KalmanFilter {
    pub fn new(dim: usize, params: &KalmanParams, warmup: usize) -> Self {
        let Q = params.Q.expand(dim);
        let R = params.R.expand(dim);
        let P = vec![params.P0; dim];
        let x = vec![0.0; dim];

        Self {
            dim, x, P, Q, R,
            P_conv: params.convergence_threshold,
            n: 0, warmup,
        }
    }

    pub fn x_hat(&self) -> &[f64] { &self.x }
    pub fn P_diag_ref(&self) -> &[f64] { &self.P }

    fn convergido(&self) -> bool {
        self.P.iter().all(|&p| p < self.P_conv)
    }
}

impl Filter for KalmanFilter {
    fn dim(&self) -> usize { self.dim }

    fn update(&mut self, z: &[f64], dt: f64) -> Vec<f64> {
        assert_eq!(z.len(), self.dim, "dimensión de medición incorrecta");

        for i in 0..self.dim {
            // Predicción: propagar covarianza (modelo de paseo aleatorio)
            self.P[i] += self.Q[i] * dt;

            // Update: Kalman gain diagonal
            let K = self.P[i] / (self.P[i] + self.R[i]);

            // Corregir estimado
            self.x[i] += K * (z[i] - self.x[i]);

            // Actualizar covarianza (forma Joseph para estabilidad)
            self.P[i] = (1.0 - K) * self.P[i];
        }

        // Inicializar x con primera medición para convergencia rápida
        if self.n == 0 {
            self.x.copy_from_slice(z);
        }

        self.n += 1;
        self.x.clone()
    }

    fn p_diag(&self) -> Option<Vec<f64>> {
        Some(self.P.clone())
    }

    fn is_ready(&self) -> bool {
        self.n >= self.warmup && self.convergido()
    }

    fn save_state(&self) -> FilterState {
        FilterState {
            x_hat:    Some(self.x.clone()),
            P_diag:   Some(self.P.clone()),
            n_updates: self.n as u64,
            is_ready:  self.is_ready(),
            buffer:    None,
        }
    }

    fn load_state(&mut self, state: &FilterState) {
        if let Some(x) = &state.x_hat {
            if x.len() == self.dim { self.x = x.clone(); }
        }
        if let Some(p) = &state.P_diag {
            if p.len() == self.dim { self.P = p.clone(); }
        }
        self.n = state.n_updates as usize;
    }
}
