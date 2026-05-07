// agent/src/nodes/watchdog.rs
//
// WatchdogNode — árbitro entre el decisor y el actuador.
//
// Puertos de entrada (identificados por to_port en EdgeConfig):
//   "decision"  Signal::Action del decisor            (siempre requerido)
//   "feedback"  Signal::Action del MqttSubscriber     (requerido en Good)
//   "signal"    Signal::Vector del filtro/decisor     (requerido en Bad)
//
// Salida: Signal::Action al actuador.
//   - Ok/Retrying:      propaga la decisión del decisor (o el override)
//   - Blocked:          emite Hold
//   - PendingUser:      emite Hold, alerta persistente en el frontend
//
// Integración con el sistema de override:
//   ClearWatchdog  → reset() + aplica ClearOverride internamente
//   ConfirmWatchdog → Ugly: confirma la acción pendiente y la propaga
//   Override manual → se propaga si status != Blocked

use super::NodeInstance;
use agrodash_shared::{NodeAction, NodeState, Signal, Trend, WatchdogConfig, WatchdogMode};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::time::{Duration, Instant};

// ── Estado interno ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum WatchdogStatus {
    Ok,
    Retrying,
    Blocked,
    PendingUser,
}

impl WatchdogStatus {
    fn as_str(&self) -> &'static str {
        match self {
            WatchdogStatus::Ok => "ok",
            WatchdogStatus::Retrying => "retrying",
            WatchdogStatus::Blocked => "blocked",
            WatchdogStatus::PendingUser => "pending_user",
        }
    }
}

// ── Nodo ─────────────────────────────────────────────────────────────────────

pub struct WatchdogNode {
    id: String,
    cfg: WatchdogConfig,

    status: WatchdogStatus,
    retry_count: u32,
    last_check_at: Option<Instant>,
    blocked_since: Option<String>,

    // Good mode: último feedback leído
    last_feedback: Option<NodeAction>,

    // Bad mode: valor de la señal al inicio de la ventana de observación
    window_start_val: Option<Vec<f64>>,
    window_start_at: Option<Instant>,

    // Ugly mode: acción pendiente de confirmación del usuario
    pending_action: Option<NodeAction>,

    // La acción que el watchdog le mandó al actuador en el ciclo anterior.
    // Necesaria para saber qué esperamos en feedback/tendencia.
    last_issued_action: Option<NodeAction>,

    // Override manual activo (On/Off). None = auto.
    override_action: Option<NodeAction>,
    // Serializado en save_state para has_watchdog_override()
    // (campo técnico — no se muestra en UI directamente)
}

impl WatchdogNode {
    pub fn new(id: String, cfg: WatchdogConfig) -> Self {
        Self {
            id,
            cfg,
            status: WatchdogStatus::Ok,
            retry_count: 0,
            last_check_at: None,
            blocked_since: None,
            last_feedback: None,
            window_start_val: None,
            window_start_at: None,
            pending_action: None,
            last_issued_action: None,
            override_action: None,
        }
    }

    // ── Reset (ClearWatchdog) ─────────────────────────────────────────────

    pub fn reset(&mut self) {
        self.status = WatchdogStatus::Ok;
        self.retry_count = 0;
        self.last_check_at = None;
        self.blocked_since = None;
        self.last_feedback = None;
        self.window_start_val = None;
        self.window_start_at = None;
        self.pending_action = None;
        self.last_issued_action = None;
        self.override_action = None;
        tracing::info!("[Watchdog:{}] reset — volviendo a modo automático", self.id);
    }

    // ── Override ─────────────────────────────────────────────────────────

    pub fn set_override(&mut self, action: NodeAction) {
        if self.status == WatchdogStatus::Blocked {
            tracing::warn!(
                "[Watchdog:{}] override ignorado — watchdog bloqueado, usá ClearWatchdog primero",
                self.id
            );
            return;
        }
        tracing::info!("[Watchdog:{}] override manual → {:?}", self.id, action);
        self.override_action = Some(action);
    }

    // ── Confirm (Ugly) ────────────────────────────────────────────────────

    pub fn confirm(&mut self) {
        if self.status != WatchdogStatus::PendingUser {
            tracing::warn!(
                "[Watchdog:{}] confirm ignorado — status={}",
                self.id,
                self.status.as_str()
            );
            return;
        }
        if let Some(action) = self.pending_action.take() {
            tracing::info!(
                "[Watchdog:{}] usuario confirmó acción {:?}",
                self.id,
                action
            );
            self.last_issued_action = Some(action);
            self.status = WatchdogStatus::Ok;
        }
    }

    // ── Timeout helper ────────────────────────────────────────────────────

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.cfg.action_timeout_secs)
    }

    fn elapsed_since_check(&self) -> Option<Duration> {
        self.last_check_at.map(|t| t.elapsed())
    }

    // ── Lógica principal ──────────────────────────────────────────────────

    fn evaluate(
        &mut self,
        decision: &NodeAction,
        feedback: Option<&NodeAction>, // Good
        signal: Option<&Vec<f64>>,     // Bad
    ) -> NodeAction {
        // Si hay override activo y no está bloqueado, úsalo directo.
        if let Some(ov) = &self.override_action.clone() {
            return ov.clone();
        }

        // Si está bloqueado, nunca actúa.
        if self.status == WatchdogStatus::Blocked {
            return NodeAction::Hold;
        }

        // Si está esperando confirmación del usuario (Ugly), Hold.
        if self.status == WatchdogStatus::PendingUser {
            return NodeAction::Hold;
        }

        match &self.cfg.mode {
            WatchdogMode::Good => self.eval_good(decision, feedback),
            WatchdogMode::Bad { .. } => self.eval_bad(decision, signal),
            WatchdogMode::Ugly { .. } => self.eval_ugly(decision),
        }
    }

    fn eval_good(&mut self, decision: &NodeAction, feedback: Option<&NodeAction>) -> NodeAction {
        // Primero propagamos la decisión del decisor.
        let action = decision.clone();

        // Si la decisión es Hold, no hay nada que verificar.
        if action == NodeAction::Hold {
            return action;
        }

        let feedback = match feedback {
            Some(f) => f,
            None => {
                // Sin feedback disponible — emitimos la acción pero no verificamos.
                tracing::debug!(
                    "[Watchdog:{}] Good sin feedback disponible este ciclo",
                    self.id
                );
                return action;
            }
        };

        // Si el feedback es Hold, el MqttSubscriber no tiene datos frescos (stale).
        // Esperamos hasta el timeout antes de retry.
        if *feedback == NodeAction::Hold {
            if let Some(elapsed) = self.elapsed_since_check() {
                if elapsed > self.timeout() {
                    return self.retry_or_block(&action, "feedback stale (sin datos MQTT frescos)");
                }
            } else {
                self.last_check_at = Some(Instant::now());
            }
            return action;
        }

        // Feedback fresco — comparar con la acción emitida.
        self.last_feedback = Some(feedback.clone());

        if let Some(last) = &self.last_issued_action {
            if feedback == last {
                // Actuador en el estado correcto — reset retries.
                if self.status == WatchdogStatus::Retrying {
                    tracing::info!(
                        "[Watchdog:{}] Good — feedback OK, retries reseteados",
                        self.id
                    );
                    self.retry_count = 0;
                    self.status = WatchdogStatus::Ok;
                    self.last_check_at = None;
                }
            } else if *last != NodeAction::Hold {
                // Feedback discrepa con lo que emitimos — ¿esperamos timeout?
                match self.elapsed_since_check() {
                    None => {
                        self.last_check_at = Some(Instant::now());
                    }
                    Some(elapsed) if elapsed > self.timeout() => {
                        return self.retry_or_block(
                            &action,
                            &format!("feedback={:?} pero se esperaba {:?}", feedback, last),
                        );
                    }
                    _ => {}
                }
            }
        } else {
            // Primera vez que emitimos una acción — marcamos el inicio.
            self.last_check_at = Some(Instant::now());
        }

        action
    }

    fn eval_bad(&mut self, decision: &NodeAction, signal: Option<&Vec<f64>>) -> NodeAction {
        let action = decision.clone();
        if action == NodeAction::Hold {
            return action;
        }

        let signal = match signal {
            Some(s) => s,
            None => {
                tracing::debug!("[Watchdog:{}] Bad sin señal disponible este ciclo", self.id);
                return action;
            }
        };

        // Extraemos la sub-señal a evaluar (componente o norma).
        let val = self.extract_val(signal);

        // Si acaba de cambiar la acción, iniciamos la ventana de observación.
        let action_changed = self.last_issued_action.as_ref() != Some(&action);
        if action_changed || self.window_start_val.is_none() {
            self.window_start_val = Some(signal.clone());
            self.window_start_at = Some(Instant::now());
        }

        // Esperamos que se cumpla el timeout de la ventana antes de evaluar.
        let elapsed = self
            .window_start_at
            .map(|t| t.elapsed())
            .unwrap_or_default();
        if elapsed < self.timeout() {
            return action;
        }

        // Evaluamos tendencia.
        let start_val = match &self.window_start_val {
            Some(v) => self.extract_val(v),
            None => return action,
        };

        if start_val.abs() < 1e-12 {
            // Valor inicial demasiado pequeño para calcular % — evaluamos delta absoluto.
            let delta = (val - start_val).abs();
            if delta < 1e-4 {
                return self
                    .retry_or_block(&action, "señal no cambió en la ventana (valor base ≈ 0)");
            }
        }

        let WatchdogMode::Bad {
            expected_on_trend,
            expected_off_trend,
            min_change_pct,
            ..
        } = &self.cfg.mode
        else {
            return action;
        };

        let expected_trend = match &action {
            NodeAction::On => expected_on_trend,
            NodeAction::Off => expected_off_trend,
            NodeAction::Hold => return action,
        };

        let change_pct = (val - start_val) / start_val.abs();
        let ok = match expected_trend {
            Trend::Ascending => change_pct >= *min_change_pct,
            Trend::Descending => change_pct <= -*min_change_pct,
            Trend::Stable => change_pct.abs() < *min_change_pct,
        };

        if ok {
            if self.status == WatchdogStatus::Retrying {
                tracing::info!(
                    "[Watchdog:{}] Bad — tendencia OK ({:+.3}%), retries reseteados",
                    self.id,
                    change_pct * 100.0
                );
                self.retry_count = 0;
                self.status = WatchdogStatus::Ok;
                self.window_start_val = None;
                self.window_start_at = None;
            }
        } else {
            let msg = format!(
                "tendencia {:+.3}% no cumple {:?} (mínimo {:.1}%)",
                change_pct * 100.0,
                expected_trend,
                min_change_pct * 100.0
            );
            // Reiniciamos la ventana para el próximo intento.
            self.window_start_val = Some(signal.clone());
            self.window_start_at = Some(Instant::now());
            return self.retry_or_block(&action, &msg);
        }

        action
    }

    fn eval_ugly(&mut self, decision: &NodeAction) -> NodeAction {
        let action = decision.clone();
        if action == NodeAction::Hold {
            return action;
        }

        // Si la acción cambió respecto a lo que emitimos antes → pedir confirmación.
        let action_changed = self.last_issued_action.as_ref() != Some(&action);
        if action_changed {
            tracing::warn!(
                "[Watchdog:{}] Ugly — acción {:?} pendiente de confirmación del usuario",
                self.id,
                action
            );
            self.status = WatchdogStatus::PendingUser;
            self.pending_action = Some(action.clone());
            // En Ugly no actuamos hasta confirmar — emitimos Hold.
            return NodeAction::Hold;
        }

        // Si ya confirmamos esta acción, la propagamos normalmente.
        action
    }

    // ── Retry / Block helper ──────────────────────────────────────────────

    fn retry_or_block(&mut self, action: &NodeAction, reason: &str) -> NodeAction {
        self.retry_count += 1;
        tracing::warn!(
            "[Watchdog:{}] fallo #{}/{} — {} — acción={:?}",
            self.id,
            self.retry_count,
            self.cfg.max_retries,
            reason,
            action
        );
        if self.retry_count > self.cfg.max_retries {
            if self.status != WatchdogStatus::Blocked {
                self.status = WatchdogStatus::Blocked;
                self.blocked_since = Some(Utc::now().to_rfc3339());
                tracing::error!(
                    "[Watchdog:{}] BLOQUEADO tras {} retries — requiere ClearWatchdog",
                    self.id,
                    self.cfg.max_retries
                );
            }
            return NodeAction::Hold;
        }
        self.status = WatchdogStatus::Retrying;
        self.last_check_at = Some(Instant::now()); // reset timer para el siguiente intento
        action.clone()
    }

    // ── Extractor de valor escalar del vector de señal ────────────────────

    fn extract_val(&self, v: &[f64]) -> f64 {
        let component = match &self.cfg.mode {
            WatchdogMode::Bad { component, .. } => *component,
            _ => None,
        };
        match component {
            Some(i) if i < v.len() => v[i],
            _ => {
                // Norma L2
                v.iter().map(|x| x * x).sum::<f64>().sqrt()
            }
        }
    }
}

// ── NodeInstance ──────────────────────────────────────────────────────────────

#[async_trait]
impl NodeInstance for WatchdogNode {
    async fn execute(
        &mut self,
        inputs: Vec<Signal>,
        _dt: f64,
        _pool: &PgPool,
    ) -> Result<Option<Signal>> {
        // Clasificamos los inputs por puerto.
        // El agente los entrega en el orden en que están los edges en el grafo,
        // pero nosotros los distinguimos por Signal type y orden de llegada:
        //   - primer Signal::Action  → "decision"
        //   - segundo Signal::Action → "feedback" (Good)
        //   - primer Signal::Vector  → "signal"   (Bad)
        //
        // El runner del agente pasa los inputs según to_port cuando está disponible.
        // Si no hay to_port, usamos el orden de llegada + tipo de señal.

        let mut decision: Option<NodeAction> = None;
        let mut feedback: Option<NodeAction> = None;
        let mut signal: Option<Vec<f64>> = None;

        for s in &inputs {
            match s {
                Signal::Action(a) => {
                    if decision.is_none() {
                        decision = Some(a.clone());
                    } else if feedback.is_none() {
                        feedback = Some(a.clone());
                    }
                }
                Signal::Vector(v) => {
                    if signal.is_none() {
                        signal = Some(v.clone());
                    }
                }
            }
        }

        let decision = match decision {
            Some(d) => d,
            None => {
                // Sin decisión → Hold. No es un error — el pipeline puede estar
                // en warmup y el decisor aún no emitió ninguna acción.
                return Ok(Some(Signal::Action(NodeAction::Hold)));
            }
        };

        let output = self.evaluate(&decision, feedback.as_ref(), signal.as_ref());
        self.last_issued_action = Some(output.clone());
        Ok(Some(Signal::Action(output)))
    }

    fn save_state(&self) -> NodeState {
        let mode_str = match &self.cfg.mode {
            WatchdogMode::Good => "good",
            WatchdogMode::Bad { .. } => "bad",
            WatchdogMode::Ugly { .. } => "ugly",
        };

        NodeState {
            node_id: self.id.clone(),
            node_type: "watchdog".into(),
            data: serde_json::json!({
                "mode":             mode_str,
                "status":           self.status.as_str(),
                "retry_count":      self.retry_count,
                "last_feedback":    self.last_feedback,
                "blocked_since":    self.blocked_since,
                "pending_action":   self.pending_action,
                "last_issued_action": self.last_issued_action,
                // Bad mode: valor al inicio de la ventana (para el frontend)
                "window_start_val": self.window_start_val,
                // Ugly: mensaje de notificación
                "notify_message":   match &self.cfg.mode {
                    WatchdogMode::Ugly { notify_message } => notify_message.clone(),
                    _ => None,
                },
                "actuator_id": self.cfg.actuator_id,
                // Flag técnico para has_watchdog_override()
                "override_action": self.override_action,
            }),
            is_ready: true,
        }
    }

    fn load_state(&mut self, state: &NodeState) {
        // Restauramos solo lo que es seguro: retry_count y blocked_since.
        // El status se recalcula en el primer ciclo para no quedar en un estado
        // de bloqueo incorrecto si el problema se resolvió mientras el agente
        // estuvo apagado.
        self.retry_count = state
            .data
            .get("retry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        self.blocked_since = state
            .data
            .get("blocked_since")
            .and_then(|v| v.as_str().map(String::from));
        // Si estaba bloqueado, lo mantenemos para que el usuario tenga que hacer
        // ClearWatchdog explícitamente.
        if state.data.get("status").and_then(|v| v.as_str()) == Some("blocked") {
            self.status = WatchdogStatus::Blocked;
        }
        // Ugly: restauramos la acción pendiente si estaba esperando confirmación.
        if state.data.get("status").and_then(|v| v.as_str()) == Some("pending_user") {
            self.status = WatchdogStatus::PendingUser;
            self.pending_action = state
                .data
                .get("pending_action")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
        }
    }

    fn is_actuator(&self) -> bool {
        false
    }

    // ── Implementaciones del trait NodeInstance para watchdog control ──────────

    fn watchdog_reset(&mut self) {
        self.reset();
    }

    fn watchdog_confirm(&mut self) {
        self.confirm();
    }

    fn watchdog_set_override(&mut self, action: NodeAction) {
        self.set_override(action);
    }

    fn has_watchdog_override(&self) -> bool {
        self.override_action.is_some()
    }

    fn metrics(&self) -> Vec<(String, f64)> {
        vec![
            ("retry_count".into(), self.retry_count as f64),
            (
                "blocked".into(),
                if self.status == WatchdogStatus::Blocked {
                    1.0
                } else {
                    0.0
                },
            ),
            (
                "pending".into(),
                if self.status == WatchdogStatus::PendingUser {
                    1.0
                } else {
                    0.0
                },
            ),
        ]
    }
}

#[cfg(test)]
#[path = "watchdog_tests.rs"]
mod watchdog_tests;
