// shared/src/tests.rs
//
// Tests de round-trip de serialización para todos los tipos de config.
// Declarado desde lib.rs con:
//   #[cfg(test)]
//   mod tests;
//
// Por qué esto importa: todos estos tipos se serializan a JSONB en PostgreSQL.
// Si deserialize(serialize(x)) != x, hay un bug silencioso que solo aparece
// al reiniciar el agente o recargar la config — nunca en runtime normal.
//
// Regla para campos nuevos: SIEMPRE agregar #[serde(default)] si el campo
// es opcional, y agregar el test de compatibilidad hacia atrás acá.
//
// Cómo correr solo estos tests:
//   cargo test -p agrodash-shared -- --nocapture

use super::*;

// ── Macro de round-trip ───────────────────────────────────────────────────────
//
// Verifica que serialize → deserialize → serialize produce el mismo JSON.
// Si falla en el segundo deserialize: falta #[serde(default)] en algún campo.
// Si falla en la comparación de JSON: la serialización no es simétrica.

macro_rules! test_rt {
    ($nombre:ident, $tipo:ty, $valor:expr) => {
        #[test]
        fn $nombre() {
            let original = $valor;
            let json = serde_json::to_string(&original)
                .expect("serialize falló");
            let parsed: $tipo = serde_json::from_str(&json)
                .expect("deserialize falló — ¿falta #[serde(default)]?");
            let json2 = serde_json::to_string(&parsed)
                .expect("re-serialize falló");
            assert_eq!(
                json, json2,
                "round-trip produjo JSON diferente — serialización no simétrica"
            );
        }
    };
}

// ═════════════════════════════════════════════════════════════════════════════
// WatchdogConfig / WatchdogMode — los tres modos
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(watchdog_good_rt, WatchdogConfig, WatchdogConfig {
    actuator_id: "bomba_1".into(),
    mode: WatchdogMode::Good,
    action_timeout_secs: 30,
    max_retries: 3,
});

test_rt!(watchdog_bad_rt, WatchdogConfig, WatchdogConfig {
    actuator_id: "bomba_1".into(),
    mode: WatchdogMode::Bad {
        expected_on_trend:  Trend::Ascending,
        expected_off_trend: Trend::Descending,
        min_change_pct:     0.05,
        component:          Some(0),
    },
    action_timeout_secs: 60,
    max_retries: 2,
});

test_rt!(watchdog_bad_sin_component_rt, WatchdogConfig, WatchdogConfig {
    actuator_id: "bomba_2".into(),
    mode: WatchdogMode::Bad {
        expected_on_trend:  Trend::Ascending,
        expected_off_trend: Trend::Stable,
        min_change_pct:     0.1,
        component:          None, // norma L2
    },
    action_timeout_secs: 45,
    max_retries: 1,
});

test_rt!(watchdog_ugly_rt, WatchdogConfig, WatchdogConfig {
    actuator_id: "valvula_2".into(),
    mode: WatchdogMode::Ugly { notify_message: Some("verificar presión manual".into()) },
    action_timeout_secs: 0,
    max_retries: 0,
});

test_rt!(watchdog_ugly_sin_mensaje_rt, WatchdogConfig, WatchdogConfig {
    actuator_id: "valvula_2".into(),
    mode: WatchdogMode::Ugly { notify_message: None },
    action_timeout_secs: 0,
    max_retries: 0,
});

// ═════════════════════════════════════════════════════════════════════════════
// EdgeConfig / EdgeKind
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(edge_data_rt, EdgeConfig, EdgeConfig {
    from: "filtro".into(),
    to:   "decisor".into(),
    from_port: None,
    to_port:   None,
    kind:      EdgeKind::Data,
});

test_rt!(edge_feedback_rt, EdgeConfig, EdgeConfig {
    from: "sub".into(),
    to:   "wd".into(),
    from_port: None,
    to_port:   Some("feedback".into()),
    kind:      EdgeKind::Feedback,
});

test_rt!(edge_decision_rt, EdgeConfig, EdgeConfig {
    from: "dec".into(),
    to:   "wd".into(),
    from_port: None,
    to_port:   Some("decision".into()),
    kind:      EdgeKind::Decision,
});

// ═════════════════════════════════════════════════════════════════════════════
// MqttSubscriberConfig
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(mqtt_subscriber_rt, MqttSubscriberConfig, MqttSubscriberConfig {
    connection:       ConnectionRef::Named("shared".into()),
    topic:            "sensor/estado".into(),
    payload_on:       "ON".into(),
    payload_off:      "OFF".into(),
    stale_after_secs: Some(120),
});

test_rt!(mqtt_subscriber_sin_stale_rt, MqttSubscriberConfig, MqttSubscriberConfig {
    connection:       ConnectionRef::Named("shared".into()),
    topic:            "sensor/estado".into(),
    payload_on:       "1".into(),
    payload_off:      "0".into(),
    stale_after_secs: None,
});

// ═════════════════════════════════════════════════════════════════════════════
// HysteresisConfig
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(hysteresis_rt, HysteresisConfig, HysteresisConfig {
    reduction:         Reduction::Mean,
    low:               30.0,
    high:              60.0,
    action_below_low:  NodeAction::On,
    action_above_high: NodeAction::Off,
    trend:             None,
});

test_rt!(hysteresis_con_trend_rt, HysteresisConfig, HysteresisConfig {
    reduction:         Reduction::Mean,
    low:               20.0,
    high:              80.0,
    action_below_low:  NodeAction::On,
    action_above_high: NodeAction::Off,
    trend:             Some(TrendConfig { window_n: Some(10), noise_floor: 0.01 }),
});

// ═════════════════════════════════════════════════════════════════════════════
// SprtConfig
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(sprt_rt, SprtConfig, SprtConfig {
    mu_H0:           30.0,
    mu_H1:           70.0,
    sigma:            5.0,
    alpha:            0.05,
    beta:             0.05,
    reduction:        Reduction::Mean,
    reset_on_action:  true,
    trend:            None,
});

// ═════════════════════════════════════════════════════════════════════════════
// KalmanConfig / EwmaConfig / MovingAvgConfig / LowpassConfig
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(kalman_rt, KalmanConfig, KalmanConfig {
    Q:                     VecOrScalar::Scalar(0.01),
    R:                     VecOrScalar::Scalar(0.1),
    P0:                    1.0,
    convergence_threshold: 0.01,
    warmup_samples:        10,
});

test_rt!(kalman_vec_rt, KalmanConfig, KalmanConfig {
    Q:                     VecOrScalar::Vec(vec![0.01, 0.02, 0.01]),
    R:                     VecOrScalar::Vec(vec![0.1, 0.1, 0.2]),
    P0:                    1.0,
    convergence_threshold: 0.005,
    warmup_samples:        20,
});

test_rt!(ewma_rt, EwmaConfig, EwmaConfig {
    alpha:          VecOrScalar::Scalar(0.3),
    warmup_samples: 5,
});

test_rt!(moving_avg_rt, MovingAvgConfig, MovingAvgConfig {
    window_n:       10,
    warmup_samples: 10,
});

test_rt!(lowpass_rt, LowpassConfig, LowpassConfig {
    tau_seconds:    VecOrScalar::Scalar(2.0),
    warmup_samples: 5,
});

// ═════════════════════════════════════════════════════════════════════════════
// Reduction — todas las variantes
// ═════════════════════════════════════════════════════════════════════════════

test_rt!(reduction_mean_rt,    Reduction, Reduction::Mean);
test_rt!(reduction_min_rt,     Reduction, Reduction::Min);
test_rt!(reduction_max_rt,     Reduction, Reduction::Max);
test_rt!(reduction_component_rt, Reduction, Reduction::Component { index: 2 });
test_rt!(reduction_weighted_rt,  Reduction, Reduction::WeightedByP);
test_rt!(reduction_count_below_rt, Reduction, Reduction::CountBelow { threshold: 50.0, min_count: 3 });
test_rt!(reduction_count_above_rt, Reduction, Reduction::CountAbove { threshold: 80.0, min_count: 2 });

// ═════════════════════════════════════════════════════════════════════════════
// AgentCommand — protocolo NOTIFY
//
// Si el tag de serde cambia, el agente ignora el comando silenciosamente.
// Estos tests son la especificación del protocolo.
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn agent_commands_tags_correctos() {
    let casos: &[(AgentCommand, &str)] = &[
        (AgentCommand::Stop,                                  "stop"),
        (AgentCommand::Checkpoint,                            "checkpoint"),
        (AgentCommand::Reload,                                "reload"),
        (AgentCommand::SelfTest,                              "self_test"),
        (AgentCommand::ClearWatchdog   { actuator_id: None }, "clear_watchdog"),
        (AgentCommand::ConfirmWatchdog { actuator_id: None }, "confirm_watchdog"),
    ];

    for (cmd, tag) in casos {
        let json = serde_json::to_string(cmd).unwrap();
        assert!(
            json.contains(&format!("\"cmd\":\"{tag}\"")),
            "{tag} no serializa con tag correcto: {json}"
        );
    }
}

#[test]
fn agent_commands_deserializan_payloads_notify() {
    let payloads = [
        r#"{"cmd":"stop"}"#,
        r#"{"cmd":"checkpoint"}"#,
        r#"{"cmd":"reload"}"#,
        r#"{"cmd":"self_test"}"#,
        r#"{"cmd":"clear_watchdog","actuator_id":null}"#,
        r#"{"cmd":"confirm_watchdog","actuator_id":"bomba_1"}"#,
        r#"{"cmd":"override","action":"on","actuator_id":null}"#,
    ];

    for payload in &payloads {
        let result: Result<AgentCommand, _> = serde_json::from_str(payload);
        assert!(
            result.is_ok(),
            "payload NOTIFY no deserializa: {payload}\nError: {:?}",
            result.err()
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// VecOrScalar
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn vecorscalar_scalar_expande() {
    let s = VecOrScalar::Scalar(0.5);
    assert_eq!(s.expand(3), vec![0.5, 0.5, 0.5]);
    assert_eq!(s.expand(0), Vec::<f64>::new());
}

#[test]
fn vecorscalar_vec_devuelve_clon() {
    let v = VecOrScalar::Vec(vec![1.0, 2.0, 3.0]);
    assert_eq!(v.expand(3), vec![1.0, 2.0, 3.0]);
}

// ═════════════════════════════════════════════════════════════════════════════
// Compatibilidad hacia atrás — NO BORRAR
//
// Cada test acá documenta un formato antiguo que debe seguir funcionando.
// Los configs existentes en la DB no tienen los campos nuevos.
// ═════════════════════════════════════════════════════════════════════════════

// HysteresisConfig antes de agregar el campo `trend`.
#[test]
fn hysteresis_antigua_sin_trend_funciona() {
    let json = r#"{
        "reduction": {"type": "mean"},
        "low": 30.0, "high": 60.0,
        "action_below_low": "on",
        "action_above_high": "off"
    }"#;
    let cfg: HysteresisConfig = serde_json::from_str(json)
        .expect("HysteresisConfig antigua sin 'trend' no deserializa — \
                 agregar #[serde(default)] al campo 'trend'");
    assert!(cfg.trend.is_none());
}

// EdgeConfig antes de agregar el campo `kind`.
#[test]
fn edge_antiguo_sin_kind_es_data() {
    let json = r#"{"from": "a", "to": "b"}"#;
    let edge: EdgeConfig = serde_json::from_str(json)
        .expect("EdgeConfig sin 'kind' no deserializa");
    assert_eq!(
        edge.kind,
        EdgeKind::Data,
        "EdgeKind default no es Data — rompe todos los pipelines existentes"
    );
}

// SprtConfig antes de agregar el campo `trend`.
#[test]
fn sprt_antiguo_sin_trend_funciona() {
    let json = r#"{
        "mu_H0": 30.0, "mu_H1": 70.0, "sigma": 5.0,
        "alpha": 0.05, "beta": 0.05,
        "reduction": {"type": "mean"},
        "reset_on_action": true
    }"#;
    let _cfg: SprtConfig = serde_json::from_str(json)
        .expect("SprtConfig antigua sin 'trend' no deserializa");
}

// WatchdogConfig antes de agregar action_timeout_secs y max_retries con defaults.
#[test]
fn watchdog_antiguo_sin_timeout_ni_retries_usa_defaults() {
    let json = r#"{
        "actuator_id": "bomba_1",
        "mode": {"level": "good"}
    }"#;
    let cfg: WatchdogConfig = serde_json::from_str(json)
        .expect("WatchdogConfig sin timeout/retries no deserializa — \
                 verificar #[serde(default)] en action_timeout_secs y max_retries");
    assert_eq!(cfg.action_timeout_secs, 30, "default de action_timeout_secs debe ser 30");
    assert_eq!(cfg.max_retries, 3, "default de max_retries debe ser 3");
}

// MqttSubscriberConfig antes de agregar stale_after_secs.
#[test]
fn mqtt_subscriber_antiguo_sin_stale_funciona() {
    let json = r#"{
        "connection": "shared",
        "topic": "sensor/estado",
        "payload_on": "ON",
        "payload_off": "OFF"
    }"#;
    let cfg: MqttSubscriberConfig = serde_json::from_str(json)
        .expect("MqttSubscriberConfig sin 'stale_after_secs' no deserializa");
    assert!(cfg.stale_after_secs.is_none());
}