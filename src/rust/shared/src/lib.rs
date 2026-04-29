// shared/src/lib.rs
//
// Tipos compartidos entre agrodash-api y agrodash-agent.
// Serializable a JSON para el protocolo del socket y para PostgreSQL JSONB.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Config del proceso ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub version:               u32,
    pub loop_interval_seconds: f64,
    pub connections:           Connections,
    pub source:                SourceConfig,
    pub filter:                FilterConfig,
    pub decision:              DecisionConfig,
    pub actuator:              ActuatorConfig,
}

// ── Connections ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connections {
    pub mqtt: Option<MqttConnection>,
    pub http: Option<HttpConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConnection {
    pub broker_url: String,              // mqtt://host:port
    pub client_id:  String,              // se interpola {process_id}
    pub username:   Option<String>,
    pub password:   Option<String>,
    pub keepalive_secs: Option<u64>,     // default 30
    pub qos:        Option<u8>,          // 0 | 1 | 2, default 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConnection {
    pub base_url:        String,
    pub timeout_secs:    Option<u64>,
    pub bearer_token:    Option<String>,
    pub extra_headers:   Option<HashMap<String, String>>,
}

// ── Source ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    PostgresSensor(PostgresSensorSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresSensorSource {
    /// Cada entrada es un sensor. El orden define el índice en el vector.
    pub sensors: Vec<SensorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorEntry {
    pub id:    String,    // UUID del sensor en la tabla sensors
    pub label: String,    // nombre legible, para logs y UI
}

// ── Filter ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    #[serde(flatten)]
    pub kind:            FilterKind,
    /// Muestras mínimas antes de que is_ready() → true.
    /// Para Kalman además se chequea convergencia de P.
    pub warmup_samples:  usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilterKind {
    Kalman(KalmanParams),
    MovingAvg(MovingAvgParams),
    Ewma(EwmaParams),
    Lowpass(LowpassParams),
    Passthrough,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KalmanParams {
    /// Varianza de proceso por componente.
    /// Escalar → mismo valor para todas; vec → uno por componente.
    pub Q:                     VecOrScalar,
    /// Varianza de medición por componente.
    pub R:                     VecOrScalar,
    /// Covarianza inicial (escalar, se aplica a la diagonal).
    pub P0:                    f64,
    /// P cae bajo este umbral → convergido.
    pub convergence_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovingAvgParams {
    pub window_n: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EwmaParams {
    /// Factor de suavizado [0,1]. Escalar → mismo para todas las componentes.
    pub alpha: VecOrScalar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowpassParams {
    /// Constante de tiempo RC en segundos.
    pub tau_seconds: VecOrScalar,
}

/// Permite especificar un valor escalar o uno por componente.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VecOrScalar {
    Scalar(f64),
    Vec(Vec<f64>),
}

impl VecOrScalar {
    pub fn expand(&self, n: usize) -> Vec<f64> {
        match self {
            VecOrScalar::Scalar(v) => vec![*v; n],
            VecOrScalar::Vec(v)    => v.clone(),
        }
    }
}

// ── Decision ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionConfig {
    #[serde(flatten)]
    pub kind: DecisionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionKind {
    Mahalanobis(MahalanobisParams),
    Hysteresis(HysteresisParams),
    Sprt(SprtParams),
}

/// Decisión basada en distancia de Mahalanobis al vector objetivo.
/// Usa P del filtro Kalman si está disponible; si no, identidad escalada.
///
///   d² = (x - target)ᵀ · Σ⁻¹ · (x - target)
///
/// d > threshold_act   → ActuatorAction::On  (lejos del objetivo)
/// d < threshold_deact → ActuatorAction::Off (cerca del objetivo)
/// entre ambos         → Hold (histéresis estadística)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MahalanobisParams {
    /// Vector objetivo (punto "húmedo suficiente").
    pub target:          Vec<f64>,
    /// Distancia para activar el actuador.
    pub threshold_act:   f64,
    /// Distancia para desactivar. Debe ser < threshold_act.
    pub threshold_deact: f64,
    /// Si true, usa P del Kalman como Σ. Si false, usa identidad × sigma².
    pub use_kalman_P:    bool,
    /// Solo relevante si use_kalman_P = false.
    pub sigma:           Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HysteresisParams {
    pub reduction:        Reduction,
    pub low:              f64,
    pub high:             f64,
    pub action_below_low:  ActuatorAction,
    pub action_above_high: ActuatorAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprtParams {
    pub mu_H0:            f64,
    pub mu_H1:            f64,
    pub sigma:            f64,
    pub alpha:            f64,
    pub beta:             f64,
    pub reduction:        Reduction,
    pub reset_on_action:  bool,
}

/// Cómo reducir un vector a un escalar para la decisión.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reduction {
    Mean,
    Min,
    Max,
    /// Componente específica (índice 0-based).
    Component { index: usize },
    /// Promedio ponderado por 1/P_ii (requiere Kalman).
    WeightedByP,
    /// Cuántas componentes están por debajo de threshold.
    CountBelow {
        threshold: f64,
        /// Activar si count >= n.
        min_count: usize,
    },
    CountAbove {
        threshold: f64,
        min_count: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActuatorAction {
    On,
    Off,
}

// ── Actuator ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActuatorConfig {
    #[serde(flatten)]
    pub kind: ActuatorKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActuatorKind {
    Mqtt(MqttActuatorParams),
    Http(HttpActuatorParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttActuatorParams {
    pub topic:       String,
    pub payload_on:  String,
    pub payload_off: String,
    pub retain:      Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpActuatorParams {
    pub path_on:     String,   // POST a base_url + path_on
    pub path_off:    String,
    pub body_on:     Option<serde_json::Value>,
    pub body_off:    Option<serde_json::Value>,
    pub method:      Option<String>,  // default POST
}

// ── Estado del agente (persiste en DB, warmstart) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentState {
    pub filter:   FilterState,
    pub decision: DecisionState,
    pub actuator: ActuatorState,
    pub last_raw: Option<Vec<f64>>,
    pub cycle:    u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterState {
    /// Estimado actual (x_hat para Kalman, valor filtrado para los otros).
    pub x_hat:       Option<Vec<f64>>,
    /// Diagonal de P (covarianza). Solo Kalman.
    pub P_diag:      Option<Vec<f64>>,
    /// Número de updates aplicados.
    pub n_updates:   u64,
    /// Si el filtro ya superó el warmup.
    pub is_ready:    bool,
    /// Para moving avg y EWMA: buffer circular serializado.
    pub buffer:      Option<Vec<Vec<f64>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecisionState {
    /// Estado actual de la histéresis / SPRT.
    pub hysteresis_state:    Option<ActuatorAction>,
    /// Log-likelihood ratio acumulado (SPRT).
    pub log_likelihood:      Option<f64>,
    /// Última distancia Mahalanobis calculada.
    pub last_mahalanobis:    Option<f64>,
    /// Valor reducido del último ciclo.
    pub last_reduced:        Option<f64>,
    /// Cuándo cambió de estado por última vez.
    pub last_change_at:      Option<String>,
    pub evidence_samples:    u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActuatorState {
    pub last_action:    Option<ActuatorAction>,
    pub last_action_at: Option<String>,
    pub total_on_secs:  f64,
}

// ── Protocolo del socket API ↔ agente ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentCommand {
    GetState,
    GetConfig,
    SetConfig    { config: ProcessConfig },
    Override     { action: ActuatorAction },
    ClearOverride,
    Checkpoint,   // forzar write de estado a DB
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub ok:    bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data:  Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AgentResponse {
    pub fn ok(data: impl Serialize) -> Self {
        Self {
            ok:    true,
            data:  Some(serde_json::to_value(data).unwrap_or_default()),
            error: None,
        }
    }
    pub fn err(msg: impl ToString) -> Self {
        Self { ok: false, data: None, error: Some(msg.to_string()) }
    }
}

/// Estado completo que retorna get_state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAgentState {
    pub process_id:     String,
    pub cycle:          u64,
    pub is_ready:       bool,
    pub override_active: bool,
    pub filter:         FilterState,
    pub decision:       DecisionState,
    pub actuator:       ActuatorState,
    pub last_raw:       Option<Vec<f64>>,
    pub sensor_labels:  Vec<String>,
}
