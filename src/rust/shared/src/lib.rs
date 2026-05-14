// shared/src/lib.rs — ver comentario completo al final del archivo

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Config del proceso ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub version: u32,
    pub shared_connections: Option<SharedConnections>,
    pub pipelines: Vec<PipelineConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedConnections {
    pub mqtt: Option<MqttConnection>,
    pub http: Option<HttpConnection>,
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub id: String,
    pub label: String,
    pub loop_interval_seconds: f64,
    pub nodes: Vec<NodeConfig>,
    pub edges: Vec<EdgeConfig>,
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
    pub id: String,
    /// Nombre descriptivo libre — solo para display en frontend y logs.
    /// No afecta la lógica del agente. Opcional, retrocompatible.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(flatten)]
    pub kind: NodeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    // Fuentes
    PostgresSensor(PostgresSensorConfig),
    // Filtros
    Kalman(KalmanConfig),
    MovingAvg(MovingAvgConfig),
    Ewma(EwmaConfig),
    Lowpass(LowpassConfig),
    Passthrough,
    // Combinadores
    Concat,
    WeightedMean { weights: Vec<f64> },
    // Decisores
    Mahalanobis(MahalanobisConfig),
    Hysteresis(HysteresisConfig),
    Sprt(SprtConfig),
    // Actuadores
    MqttActuator(MqttActuatorConfig),
    HttpActuator(HttpActuatorConfig),
    // Sanity / Watchdog
    MqttSubscriber(MqttSubscriberConfig),
    Watchdog(WatchdogConfig),
    // Utilidades
    Logger { tag: String },
    Select { indices: Vec<usize> },
    LinearScale { a: VecOrScalar, b: VecOrScalar },
}

// ── Edge ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub from_port: Option<String>,
    #[serde(default)]
    pub to_port: Option<String>,
    /// Tipo semántico: controla el color del edge en el canvas.
    /// Omitido = Data (azul, default).
    #[serde(default)]
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    #[default]
    Data, // azul — flujo de datos normal
    Feedback, // naranja — señal de retroalimentación (MqttSubscriber → Watchdog)
    Decision, // verde — acción de decisor → watchdog/actuador
}

// ── Signal ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Signal {
    Vector(Vec<f64>),
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
    pub id: String,
    pub label: String,
}

// ── Filtros ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct KalmanConfig {
    pub Q: VecOrScalar,
    pub R: VecOrScalar,
    pub P0: f64,
    pub convergence_threshold: f64,
    pub warmup_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovingAvgConfig {
    pub window_n: usize,
    pub warmup_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EwmaConfig {
    pub alpha: VecOrScalar,
    pub warmup_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowpassConfig {
    pub tau_seconds: VecOrScalar,
    pub warmup_samples: usize,
}

// ── TrendBuffer (compartido por decisores) ────────────────────────────────────

/// Config de tendencia opcional en cualquier decisor.
/// Permite que el Watchdog en modo Bad evalúe si el actuador tuvo efecto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendConfig {
    /// Ventana de muestras para la derivada discreta.
    /// None → usa warmup_samples del filtro upstream (best-effort, el decisor
    /// simplemente acumula todas las muestras disponibles hasta que tenga window_n).
    pub window_n: Option<usize>,
    /// Delta mínimo absoluto por dimensión para no tratar como ruido.
    #[serde(default = "default_noise_floor")]
    pub noise_floor: f64,
}

fn default_noise_floor() -> f64 {
    1e-5
}

// ── Decisores ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct MahalanobisConfig {
    pub target: Vec<f64>,
    pub threshold_act: f64,
    pub threshold_deact: f64,
    pub use_kalman_P: bool,
    pub sigma: Option<f64>,
    /// Tracking de tendencia — requerido para Watchdog en modo Bad.
    #[serde(default)]
    pub trend: Option<TrendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HysteresisConfig {
    pub reduction: Reduction,
    pub low: f64,
    pub high: f64,
    pub action_below_low: NodeAction,
    pub action_above_high: NodeAction,
    #[serde(default)]
    pub trend: Option<TrendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct SprtConfig {
    pub mu_H0: f64,
    pub mu_H1: f64,
    pub sigma: f64,
    pub alpha: f64,
    pub beta: f64,
    pub reduction: Reduction,
    pub reset_on_action: bool,
    #[serde(default)]
    pub trend: Option<TrendConfig>,
}

// ── Actuadores ────────────────────────────────────────────────────────────────

/// Una etapa de confirmación en el pipeline de comunicación de un actuador.
///
/// Placeholders en match_prefix y error_prefix:
///   {action}  → "ON" o "OFF"
///   {payload} → payload completo enviado (ej. "on,5")
///   {valve}   → parte después de la coma en payload_on (ej. "5")
///
/// Ejemplo para BioCarbón:
///   { "name": "Gateway",      "match_prefix": "MQTT_RECIBIDO:{action},{valve}", "timeout_secs": 3  }
///   { "name": "LoRa",         "match_prefix": "LORA_ENVIANDO:{action},{valve}", "timeout_secs": 8  }
///   { "name": "Concentrador", "match_prefix": "CONCENTRADOR:Relay{valve}",      "timeout_secs": 15, "terminal": true }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckStage {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub match_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error_prefix: Option<String>,
    pub timeout_secs: f64,
    #[serde(default)]
    pub terminal: bool,
}

impl AckStage {
    fn interpolate(pattern: &str, action: &str, payload: &str) -> String {
        let valve = payload.split_once(',').map(|(_, v)| v).unwrap_or(payload);
        pattern
            .replace("{action}", &action.to_uppercase())
            .replace("{payload}", payload)
            .replace("{valve}", valve)
    }

    pub fn matches(&self, msg: &str, action: &str, payload: &str) -> bool {
        self.match_prefix
            .as_deref()
            .map(|p| msg.starts_with(&Self::interpolate(p, action, payload)))
            .unwrap_or(false)
    }

    pub fn is_error(&self, msg: &str, action: &str, payload: &str) -> bool {
        self.error_prefix
            .as_deref()
            .map(|p| msg.starts_with(&Self::interpolate(p, action, payload)))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttActuatorConfig {
    pub connection: ConnectionRef,
    pub topic: String,
    pub payload_on: String,
    pub payload_off: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retain: Option<bool>,
    /// Nombre corto del actuador — redundante con NodeConfig.label pero
    /// accesible desde el config del nodo sin buscar en NodeConfig.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    /// Topic MQTT donde llegan las confirmaciones de etapas.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ack_topic: Option<String>,
    /// Etapas de confirmación en orden. Requiere ack_topic.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stages: Option<Vec<AckStage>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpActuatorConfig {
    pub connection: ConnectionRef,
    pub path_on: String,
    pub path_off: String,
    pub body_on: Option<serde_json::Value>,
    pub body_off: Option<serde_json::Value>,
    pub method: Option<String>,
}

// ── MqttSubscriber ────────────────────────────────────────────────────────────

/// Nodo fuente de feedback. Suscribe un topic MQTT y emite Signal::Action.
/// Conectar al puerto "feedback" del Watchdog con EdgeKind::Feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttSubscriberConfig {
    pub connection: ConnectionRef,
    pub topic: String,
    pub payload_on: String,
    pub payload_off: String,
    /// Segundos sin mensaje antes de considerar la lectura obsoleta.
    /// None = nunca expira (mantiene último valor).
    #[serde(default)]
    pub stale_after_secs: Option<u64>,
}

// ── Watchdog ──────────────────────────────────────────────────────────────────

/// Árbitro entre decisor y actuador. Tres niveles de sanity check.
///
/// Puertos de entrada (identificados por `to_port` en EdgeConfig):
///   "decision"  — Signal::Action del decisor       (requerido)
///   "feedback"  — Signal::Action del MqttSubscriber (requerido en Good)
///   "signal"    — Signal::Vector del filtro          (requerido en Bad)
///
/// Salida: Signal::Action al actuador (Hold si bloqueado/pendiente).
///
/// Gestión de overrides:
///   ClearWatchdog  → resetea retry_count + status, luego aplica ClearOverride.
///   ConfirmWatchdog → Ugly: confirma la acción pendiente y la propaga al actuador.
///   Override manual → respetado mientras status != Blocked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    /// ID del actuador que este watchdog protege.
    pub actuator_id: String,
    pub mode: WatchdogMode,
    /// Segundos que el watchdog espera antes de evaluar / hacer retry.
    #[serde(default = "default_action_timeout")]
    pub action_timeout_secs: u64,
    /// Máximo de reintentos antes de bloquear (Good y Bad).
    /// Ugly siempre es 0.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_action_timeout() -> u64 {
    30
}
fn default_max_retries() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum WatchdogMode {
    /// Full autopilot. Requiere MqttSubscriber en puerto "feedback".
    Good,

    /// Inferencia por tendencia de la señal filtrada.
    /// Requiere Signal::Vector en el puerto "signal".
    Bad {
        expected_on_trend: Trend,
        expected_off_trend: Trend,
        /// Cambio mínimo porcentual en la ventana (0.0–1.0). Ej: 0.05 = 5%.
        min_change_pct: f64,
        /// Componente del vector a evaluar. None = norma L2 del vector.
        #[serde(default)]
        component: Option<usize>,
    },

    /// Manual aumentado. Emite alertas persistentes y espera confirmación.
    Ugly {
        #[serde(default)]
        notify_message: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Ascending,
    Descending,
    Stable,
}

// Estado del Watchdog serializado en NodeState.data:
// {
//   "mode":              "good" | "bad" | "ugly",
//   "status":            "ok" | "retrying" | "blocked" | "pending_user",
//   "retry_count":       u32,
//   "last_check_at":     Option<String ISO>,
//   "last_feedback":     Option<"on" | "off" | "stale">,   ← Good
//   "blocked_since":     Option<String ISO>,
//   "pending_action":    Option<"on" | "off">,             ← Ugly
//   "window_start_val":  Option<Vec<f64>>,                 ← Bad
//   "window_start_at":   Option<String ISO>,               ← Bad
//   "trend":             Option<Vec<f64>>,                 ← Bad: derivada actual
// }

// ── Conexiones ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConnectionRef {
    Named(String),
    Inline(Box<InlineConnection>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineConnection {
    pub mqtt: Option<MqttConnection>,
    pub http: Option<HttpConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConnection {
    pub broker_url: String,
    pub client_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub keepalive_secs: Option<u64>,
    pub qos: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConnection {
    pub base_url: String,
    pub timeout_secs: Option<u64>,
    pub bearer_token: Option<String>,
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
            VecOrScalar::Vec(v) => v.clone(),
        }
    }
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

// ── Estado del agente ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentState {
    pub pipeline_id: String,
    pub node_states: HashMap<String, NodeState>,
    pub last_signals: HashMap<String, SignalSnapshot>,
    pub cycle: u64,
    pub is_ready: bool,
    pub override_active: bool,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeState {
    pub node_id: String,
    pub node_type: String,
    pub data: serde_json::Value,
    pub is_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshot {
    pub node_id: String,
    pub signal: Signal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAgentState {
    pub pipeline_id: String,
    pub label: String,
    pub cycle: u64,
    pub is_ready: bool,
    pub override_active: bool,
    pub node_states: HashMap<String, NodeState>,
    pub last_signals: HashMap<String, SignalSnapshot>,
}

// ── Protocolo NOTIFY / AgentCommand ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentCommand {
    Stop,

    Override {
        action: NodeAction,
        actuator_id: Option<String>,
    },

    /// Resetea el watchdog (retry_count=0, status=Ok) y aplica ClearOverride.
    /// Si no hay Watchdog en el pipeline, actúa igual que el antiguo ClearOverride.
    ClearWatchdog {
        actuator_id: Option<String>,
    },

    /// Ugly mode: el usuario confirma que el actuador está en el estado esperado.
    ConfirmWatchdog {
        actuator_id: Option<String>,
    },

    Reload,
    Checkpoint,
    SelfTest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCmdResult {
    pub pipeline_id: String,
    pub cmd: String,
    pub ok: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub ts: String,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
