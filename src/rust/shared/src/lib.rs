// shared/src/lib.rs
//
// Tipos compartidos entre agrodash-api y agrodash-agent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Config del proceso (multi-pipeline) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub version:   u32,
    pub pipelines: Vec<PipelineConfig>,
}

// ── Pipeline — un agente por pipeline ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Identificador único dentro del proceso. Snake_case. Ej: "fila_1"
    pub id:                    String,
    /// Nombre legible. Ej: "Fila 1 — Sector Norte"
    pub label:                 String,
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
    pub broker_url:     String,
    pub client_id:      String,        // se interpola {process_id}_{pipeline_id}
    pub username:       Option<String>,
    pub password:       Option<String>,
    pub keepalive_secs: Option<u64>,
    pub qos:            Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConnection {
    pub base_url:      String,
    pub timeout_secs:  Option<u64>,
    pub bearer_token:  Option<String>,
    pub extra_headers: Option<HashMap<String, String>>,
}

// ── Source ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    PostgresSensor(PostgresSensorSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresSensorSource {
    pub sensors: Vec<SensorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorEntry {
    pub id:    String,
    pub label: String,
}

// ── Filter ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    #[serde(flatten)]
    pub kind:           FilterKind,
    pub warmup_samples: usize,
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
    pub Q:                     VecOrScalar,
    pub R:                     VecOrScalar,
    pub P0:                    f64,
    pub convergence_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovingAvgParams {
    pub window_n: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EwmaParams {
    pub alpha: VecOrScalar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowpassParams {
    pub tau_seconds: VecOrScalar,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MahalanobisParams {
    pub target:          Vec<f64>,
    pub threshold_act:   f64,
    pub threshold_deact: f64,
    pub use_kalman_P:    bool,
    pub sigma:           Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HysteresisParams {
    pub reduction:         Reduction,
    pub low:               f64,
    pub high:              f64,
    pub action_below_low:  ActuatorAction,
    pub action_above_high: ActuatorAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprtParams {
    pub mu_H0:           f64,
    pub mu_H1:           f64,
    pub sigma:           f64,
    pub alpha:           f64,
    pub beta:            f64,
    pub reduction:       Reduction,
    pub reset_on_action: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reduction {
    Mean,
    Min,
    Max,
    Component { index: usize },
    WeightedByP,
    CountBelow { threshold: f64, min_count: usize },
    CountAbove { threshold: f64, min_count: usize },
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
    pub path_on:  String,
    pub path_off: String,
    pub body_on:  Option<serde_json::Value>,
    pub body_off: Option<serde_json::Value>,
    pub method:   Option<String>,
}

// ── Estado del agente ─────────────────────────────────────────────────────────
// Un agente maneja exactamente un pipeline.
// El API almacena el estado por (process_id, pipeline_id).

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentState {
    pub pipeline_id: String,
    pub filter:      FilterState,
    pub decision:    DecisionState,
    pub actuator:    ActuatorState,
    pub last_raw:    Option<Vec<f64>>,
    pub cycle:       u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterState {
    pub x_hat:     Option<Vec<f64>>,
    pub P_diag:    Option<Vec<f64>>,
    pub n_updates: u64,
    pub is_ready:  bool,
    pub buffer:    Option<Vec<Vec<f64>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecisionState {
    pub hysteresis_state:  Option<ActuatorAction>,
    pub log_likelihood:    Option<f64>,
    pub last_mahalanobis:  Option<f64>,
    pub last_reduced:      Option<f64>,
    pub last_change_at:    Option<String>,
    pub evidence_samples:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActuatorState {
    pub last_action:    Option<ActuatorAction>,
    pub last_action_at: Option<String>,
    pub total_on_secs:  f64,
}

// ── Estado completo del proceso (todos los pipelines) ────────────────────────
// Lo que retorna GET /processes/:id/state

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessFullState {
    pub process_id: String,
    pub pipelines:  Vec<PipelineFullState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineFullState {
    pub pipeline_id:     String,
    pub label:           String,
    pub is_ready:        bool,
    pub override_active: bool,
    pub sensor_labels:   Vec<String>,
    pub filter:          FilterState,
    pub decision:        DecisionState,
    pub actuator:        ActuatorState,
    pub last_raw:        Option<Vec<f64>>,
    pub cycle:           u64,
}

// ── Protocolo socket API ↔ agente ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentCommand {
    GetState,
    GetConfig,
    SetConfig    { config: PipelineConfig },
    Override     { action: ActuatorAction },
    ClearOverride,
    Checkpoint,
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

// ── FullAgentState — lo que retorna get_state al socket ───────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAgentState {
    pub pipeline_id:     String,
    pub cycle:           u64,
    pub is_ready:        bool,
    pub override_active: bool,
    pub filter:          FilterState,
    pub decision:        DecisionState,
    pub actuator:        ActuatorState,
    pub last_raw:        Option<Vec<f64>>,
    pub sensor_labels:   Vec<String>,
}
