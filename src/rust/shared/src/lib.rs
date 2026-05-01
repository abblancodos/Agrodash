// shared/src/lib.rs
//
// Schema de configuración basado en nodos y edges (dataflow/FBP).
// Compatible con el modelo de nodos de Svelteflow en el frontend.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Config del proceso ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub version: u32,

    /// Conexiones compartidas por todos los pipelines que las referencien.
    pub shared_connections: Option<SharedConnections>,

    /// Conjunto de pipelines independientes dentro del proceso.
    /// Cada pipeline es un grafo de nodos y edges.
    pub pipelines: Vec<PipelineConfig>,
}

// ── Shared connections ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedConnections {
    pub mqtt: Option<MqttConnection>,
    pub http: Option<HttpConnection>,
}

// ── Pipeline = grafo de nodos y edges ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Identificador único dentro del proceso. Snake_case. Ej: "fila_1"
    pub id:    String,
    /// Nombre legible. Ej: "Fila 1 — Sector Norte"
    pub label: String,
    /// Segundos entre ejecuciones del loop.
    pub loop_interval_seconds: f64,
    /// Nodos del grafo.
    pub nodes: Vec<NodeConfig>,
    /// Edges dirigidos entre nodos.
    pub edges: Vec<EdgeConfig>,
    /// Posiciones de los nodos en el canvas (para Svelteflow).
    #[serde(default)]
    pub node_positions: HashMap<String, NodePosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

// ── Nodo ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// ID único dentro del pipeline. Ej: "src1", "flt1", "dec1", "act1"
    pub id:     String,
    /// Tipo y configuración del nodo.
    #[serde(flatten)]
    pub kind:   NodeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    // ── Fuentes ──────────────────────────────────────────────────────────────
    PostgresSensor(PostgresSensorConfig),

    // ── Filtros ──────────────────────────────────────────────────────────────
    Kalman(KalmanConfig),
    MovingAvg(MovingAvgConfig),
    Ewma(EwmaConfig),
    Lowpass(LowpassConfig),
    Passthrough,

    // ── Combinadores (fan-in) ─────────────────────────────────────────────────
    /// Concatena vectores de múltiples entradas en uno solo.
    Concat,
    /// Promedio ponderado de múltiples entradas.
    WeightedMean { weights: Vec<f64> },

    // ── Decisores ────────────────────────────────────────────────────────────
    Mahalanobis(MahalanobisConfig),
    Hysteresis(HysteresisConfig),
    Sprt(SprtConfig),

    // ── Actuadores ───────────────────────────────────────────────────────────
    MqttActuator(MqttActuatorConfig),
    HttpActuator(HttpActuatorConfig),

    // ── Utilidades ────────────────────────────────────────────────────────────
    /// Registra el valor que pasa por él en process_readings sin modificarlo.
    Logger { tag: String },
    /// Selecciona componentes específicas del vector.
    Select { indices: Vec<usize> },
    /// Aplica una transformación lineal: y = a*x + b
    LinearScale { a: VecOrScalar, b: VecOrScalar },
}

// ── Edge ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub from: String,   // node id
    pub to:   String,   // node id
    /// Puerto de salida (para nodos con múltiples salidas, futuro).
    #[serde(default)]
    pub from_port: Option<String>,
    /// Puerto de entrada (para nodos con múltiples entradas).
    #[serde(default)]
    pub to_port:   Option<String>,
}

// ── Tipos de señal que fluyen entre nodos ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Signal {
    /// Vector de f64 — sale de fuentes y filtros.
    Vector(Vec<f64>),
    /// Decisión binaria — sale de decisores.
    Action(NodeAction),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeAction {
    On,
    Off,
    Hold,
}

// ── Fuentes ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresSensorConfig {
    pub sensors: Vec<SensorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorEntry {
    pub id:    String,
    pub label: String,
}

// ── Filtros ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct KalmanConfig {
    pub Q:                     VecOrScalar,
    pub R:                     VecOrScalar,
    pub P0:                    f64,
    pub convergence_threshold: f64,
    pub warmup_samples:        usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovingAvgConfig {
    pub window_n:       usize,
    pub warmup_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EwmaConfig {
    pub alpha:          VecOrScalar,
    pub warmup_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowpassConfig {
    pub tau_seconds:    VecOrScalar,
    pub warmup_samples: usize,
}

// ── Decisores ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct MahalanobisConfig {
    pub target:          Vec<f64>,
    pub threshold_act:   f64,
    pub threshold_deact: f64,
    pub use_kalman_P:    bool,
    pub sigma:           Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HysteresisConfig {
    pub reduction:         Reduction,
    pub low:               f64,
    pub high:              f64,
    pub action_below_low:  NodeAction,
    pub action_above_high: NodeAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct SprtConfig {
    pub mu_H0:           f64,
    pub mu_H1:           f64,
    pub sigma:           f64,
    pub alpha:           f64,
    pub beta:            f64,
    pub reduction:       Reduction,
    pub reset_on_action: bool,
}

// ── Actuadores ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttActuatorConfig {
    /// "shared" → usar shared_connections.mqtt
    /// "pipeline:<id>" → usar conexiones del pipeline indicado
    /// Omitido → conexión propia definida en connection
    pub connection:  ConnectionRef,
    pub topic:       String,
    pub payload_on:  String,
    pub payload_off: String,
    pub retain:      Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpActuatorConfig {
    pub connection: ConnectionRef,
    pub path_on:    String,
    pub path_off:   String,
    pub body_on:    Option<serde_json::Value>,
    pub body_off:   Option<serde_json::Value>,
    pub method:     Option<String>,
}

/// Referencia a una conexión — compartida, de otro pipeline, o propia.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConnectionRef {
    /// "shared" → usar shared_connections del proceso
    Named(String),
    /// Conexión propia inline
    Inline(InlineConnection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineConnection {
    pub mqtt: Option<MqttConnection>,
    pub http: Option<HttpConnection>,
}

// ── Conexiones ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConnection {
    pub broker_url:     String,
    /// Se interpola: {process_id}_{pipeline_id}
    pub client_id:      String,
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

// ── Helpers ───────────────────────────────────────────────────────────────────

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reduction {
    Mean,
    Min,
    Max,
    Component   { index: usize },
    WeightedByP,
    CountBelow  { threshold: f64, min_count: usize },
    CountAbove  { threshold: f64, min_count: usize },
}

// ── Estado del agente ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentState {
    pub pipeline_id:  String,
    /// Estado por nodo — keyed por node_id
    pub node_states:  HashMap<String, NodeState>,
    pub last_signals: HashMap<String, SignalSnapshot>,
    pub cycle:        u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeState {
    pub node_id:   String,
    pub node_type: String,
    /// Estado interno serializado (x_hat, P, buffer, etc.)
    pub data:      serde_json::Value,
    pub is_ready:  bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshot {
    pub node_id:  String,
    /// El valor de la señal en el último ciclo
    pub signal:   Signal,
}

// ── Estado completo — lo que retorna get_state ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAgentState {
    pub pipeline_id:     String,
    pub label:           String,
    pub cycle:           u64,
    pub is_ready:        bool,
    pub override_active: bool,
    pub node_states:     HashMap<String, NodeState>,
    pub last_signals:    HashMap<String, SignalSnapshot>,
}

// ── Protocolo socket ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentCommand {
    GetState,
    GetConfig,
    /// Recarga el ProcessConfig completo y reconstruye el grafo del pipeline.
    SetConfig    { config: ProcessConfig },
    Override     { action: NodeAction },
    ClearOverride,
    Checkpoint,
    Stop,
    /// Ejecuta un ciclo dry-run (sin actuadores) y reporta el estado de cada nodo.
    /// Usado por el self-test de infraestructura.
    SelfTest,
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
        Self { ok: true, data: Some(serde_json::to_value(data).unwrap_or_default()), error: None }
    }
    pub fn err(msg: impl ToString) -> Self {
        Self { ok: false, data: None, error: Some(msg.to_string()) }
    }
}