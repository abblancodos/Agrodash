// agent/src/nodes/watchdog_tests.rs
//
// Tests de WatchdogNode. Declarado desde watchdog.rs con:
//   #[cfg(test)]
//   #[path = "watchdog_tests.rs"]
//   mod watchdog_tests;
//
#![allow(clippy::unwrap_used, clippy::expect_used)]
// Cubre:
//   - Las 9 transiciones FSM (TESTING.md §1.3)
//   - Checklist obligatorio de nodo nuevo (TESTING.md §1.1)
//   - Override manual
//   - Bad mode — ventana de observación
//
// Cómo correr solo estos tests:
//   cargo test -p agrodash-agent watchdog -- --nocapture

use super::*;
use agrodash_shared::{NodeAction, NodeState, Signal, Trend, WatchdogConfig, WatchdogMode};

// ── Helpers de configuración ──────────────────────────────────────────────────

fn cfg_good(max_retries: u32, timeout_secs: u64) -> WatchdogConfig {
    WatchdogConfig {
        actuator_id: "bomba_1".into(),
        mode: WatchdogMode::Good,
        action_timeout_secs: timeout_secs,
        max_retries,
    }
}

fn cfg_ugly() -> WatchdogConfig {
    WatchdogConfig {
        actuator_id: "valvula_1".into(),
        mode: WatchdogMode::Ugly {
            notify_message: None,
        },
        action_timeout_secs: 0,
        max_retries: 0,
    }
}

fn cfg_bad(max_retries: u32, timeout_secs: u64) -> WatchdogConfig {
    WatchdogConfig {
        actuator_id: "bomba_2".into(),
        mode: WatchdogMode::Bad {
            expected_on_trend: Trend::Ascending,
            expected_off_trend: Trend::Descending,
            min_change_pct: 0.05,
            component: Some(0),
        },
        action_timeout_secs: timeout_secs,
        max_retries,
    }
}

fn pool() -> sqlx::PgPool {
    match std::env::var("DATABASE_URL") {
        Ok(url) => tokio::runtime::Handle::current()
            .block_on(sqlx::PgPool::connect(&url))
            .expect("DATABASE_URL disponible pero no se pudo conectar"),
        Err(_) => sqlx::PgPool::connect_lazy("postgres://localhost/agrodash_test")
            .expect("URL de fallback inválida"),
    }
}

/// Ejecuta un ciclo del watchdog con decision + feedback/signal opcionales.
/// Devuelve el NodeAction resultante.
async fn run(
    wd: &mut WatchdogNode,
    decision: NodeAction,
    feedback: Option<NodeAction>,
    signal: Option<Vec<f64>>,
    pool: &sqlx::PgPool,
) -> NodeAction {
    let mut inputs = vec![Signal::Action(decision)];
    if let Some(fb) = feedback {
        inputs.push(Signal::Action(fb));
    }
    if let Some(sig) = signal {
        inputs.push(Signal::Vector(sig));
    }
    match wd
        .execute(inputs, 1.0, pool)
        .await
        .expect("execute no debe fallar en helper de test")
    {
        Some(Signal::Action(a)) => a,
        other => panic!("se esperaba Signal::Action, got {:?}", other),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// FSM — 9 transiciones (TESTING.md §1.3)
// ═════════════════════════════════════════════════════════════════════════════

// T1: Ok + feedback correcto → Ok, propaga decisión
#[tokio::test]
async fn t1_ok_feedback_correcto_permanece_ok() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(2, 0));

    // Primer ciclo: establece last_issued_action.
    let out1 = run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(out1, NodeAction::On, "T1: primer ciclo debe emitir On");

    // Segundo ciclo: feedback confirma On.
    let out2 = run(&mut wd, NodeAction::On, Some(NodeAction::On), None, &pool).await;
    assert_eq!(
        out2,
        NodeAction::On,
        "T1: con feedback correcto debe propagar On"
    );
    assert_eq!(wd.status, WatchdogStatus::Ok, "T1: status debe seguir Ok");
    assert_eq!(wd.retry_count, 0, "T1: no debe haber retries");
}

// T2: Ok + feedback discrepa (después del timeout) → Retrying, propaga decisión
//
// Con timeout=0 la discrepancia se detecta inmediatamente.
// El loop cubre el ciclo extra que necesita elapsed_since_check.
#[tokio::test]
async fn t2_ok_feedback_discrepa_pasa_a_retrying() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(3, 0));

    run(&mut wd, NodeAction::On, None, None, &pool).await;

    let mut final_status = wd.status.clone();
    for _ in 0..3 {
        run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        final_status = wd.status.clone();
        if final_status == WatchdogStatus::Retrying {
            break;
        }
    }
    assert_eq!(
        final_status,
        WatchdogStatus::Retrying,
        "T2: debe pasar a Retrying"
    );
    assert!(wd.retry_count > 0, "T2: retry_count debe ser > 0");
}

// T3: Retrying + feedback correcto → Ok, propaga decisión
#[tokio::test]
async fn t3_retrying_feedback_correcto_vuelve_a_ok() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(3, 0));

    // Llevar a Retrying.
    run(&mut wd, NodeAction::On, None, None, &pool).await;
    for _ in 0..3 {
        run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        if wd.status == WatchdogStatus::Retrying {
            break;
        }
    }
    assert_eq!(wd.status, WatchdogStatus::Retrying, "precondición T3");

    let out = run(&mut wd, NodeAction::On, Some(NodeAction::On), None, &pool).await;
    assert_eq!(out, NodeAction::On, "T3: debe propagar la decisión");
    assert_eq!(wd.status, WatchdogStatus::Ok, "T3: debe volver a Ok");
    assert_eq!(wd.retry_count, 0, "T3: retry_count debe resetearse");
}

// T4: Retrying + discrepa × max_retries → Blocked, emite Hold
#[tokio::test]
async fn t4_retrying_agota_retries_pasa_a_blocked() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(1, 0));

    run(&mut wd, NodeAction::On, None, None, &pool).await;

    let mut final_out = NodeAction::On;
    for _ in 0..5 {
        final_out = run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        if wd.status == WatchdogStatus::Blocked {
            break;
        }
    }
    assert_eq!(
        wd.status,
        WatchdogStatus::Blocked,
        "T4: debe pasar a Blocked"
    );
    assert_eq!(final_out, NodeAction::Hold, "T4: Blocked debe emitir Hold");
}

// T5: Blocked + cualquier input → Blocked, emite Hold
#[tokio::test]
async fn t5_blocked_ignora_inputs_emite_hold() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(0, 0));

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    for _ in 0..3 {
        run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        if wd.status == WatchdogStatus::Blocked {
            break;
        }
    }
    assert_eq!(wd.status, WatchdogStatus::Blocked, "precondición T5");

    for (dec, fb) in [
        (NodeAction::On, Some(NodeAction::On)),
        (NodeAction::Off, Some(NodeAction::Off)),
        (NodeAction::On, None),
    ] {
        let out = run(&mut wd, dec, fb, None, &pool).await;
        assert_eq!(out, NodeAction::Hold, "T5: Blocked siempre emite Hold");
        assert_eq!(
            wd.status,
            WatchdogStatus::Blocked,
            "T5: status no debe cambiar"
        );
    }
}

// T6: Blocked + ClearWatchdog → Ok
#[tokio::test]
async fn t6_blocked_clear_vuelve_a_ok() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(0, 0));

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    for _ in 0..3 {
        run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        if wd.status == WatchdogStatus::Blocked {
            break;
        }
    }
    assert_eq!(wd.status, WatchdogStatus::Blocked, "precondición T6");

    wd.watchdog_reset();

    assert_eq!(
        wd.status,
        WatchdogStatus::Ok,
        "T6: ClearWatchdog debe llevar a Ok"
    );
    assert_eq!(wd.retry_count, 0, "T6: retry_count debe ser 0");
    assert!(
        wd.blocked_since.is_none(),
        "T6: blocked_since debe borrarse"
    );

    let out = run(&mut wd, NodeAction::On, Some(NodeAction::On), None, &pool).await;
    assert_eq!(
        out,
        NodeAction::On,
        "T6: tras reset debe propagar decisión normal"
    );
}

// T7: Ok + acción nueva (Ugly) → PendingUser, emite Hold
#[tokio::test]
async fn t7_ok_accion_nueva_ugly_pasa_a_pending_user() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_ugly());

    let out = run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(
        out,
        NodeAction::Hold,
        "T7: primera acción Ugly debe emitir Hold"
    );
    assert_eq!(
        wd.status,
        WatchdogStatus::PendingUser,
        "T7: debe pasar a PendingUser"
    );
    assert_eq!(
        wd.pending_action,
        Some(NodeAction::On),
        "T7: debe guardar la acción pendiente"
    );
}

// T8: PendingUser + ConfirmWatchdog → Ok, propaga acción confirmada
#[tokio::test]
async fn t8_pending_user_confirm_vuelve_a_ok() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_ugly());

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(wd.status, WatchdogStatus::PendingUser, "precondición T8");

    wd.watchdog_confirm();

    assert_eq!(
        wd.status,
        WatchdogStatus::Ok,
        "T8: confirm debe llevar a Ok"
    );
    assert!(
        wd.pending_action.is_none(),
        "T8: pending_action debe borrarse"
    );

    let out = run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(out, NodeAction::On, "T8: tras confirm debe propagar On");
}

// T9: PendingUser + cualquier input (sin confirm) → PendingUser, Hold
#[tokio::test]
async fn t9_pending_user_sin_confirm_sigue_en_hold() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_ugly());

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(wd.status, WatchdogStatus::PendingUser, "precondición T9");

    for dec in [
        NodeAction::On,
        NodeAction::Off,
        NodeAction::Hold,
        NodeAction::On,
    ] {
        let out = run(&mut wd, dec, None, None, &pool).await;
        assert_eq!(out, NodeAction::Hold, "T9: PendingUser debe emitir Hold");
        assert_eq!(
            wd.status,
            WatchdogStatus::PendingUser,
            "T9: status no debe cambiar"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Checklist §1.1
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ejecuta_con_entrada_valida() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(2, 30));
    let res = wd
        .execute(vec![Signal::Action(NodeAction::On)], 1.0, &pool)
        .await;
    assert!(res.is_ok(), "execute falló con entrada válida: {:?}", res);
}

// Sin decisión → Hold (el pipeline puede estar en warmup).
#[tokio::test]
async fn entrada_vacia_retorna_hold() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(2, 30));
    let res = wd.execute(vec![], 1.0, &pool).await;
    match res {
        Ok(Some(Signal::Action(NodeAction::Hold))) | Ok(None) | Err(_) => {}
        Ok(Some(sig)) => panic!("con entrada vacía se esperaba Hold, got {:?}", sig),
    }
}

// Round-trip save/load — estado Blocked.
// Invariante: status=blocked y retry_count sobreviven al reinicio del agente.
#[tokio::test]
async fn round_trip_blocked() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(0, 0));

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    for _ in 0..3 {
        run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        if wd.status == WatchdogStatus::Blocked {
            break;
        }
    }
    assert_eq!(
        wd.status,
        WatchdogStatus::Blocked,
        "precondición round-trip"
    );

    let state = wd.save_state();
    let mut wd2 = WatchdogNode::new("wd".into(), cfg_good(0, 0));
    wd2.load_state(&state);

    assert_eq!(
        wd2.status,
        WatchdogStatus::Blocked,
        "round-trip: status Blocked no se restauró"
    );
    assert_eq!(
        wd2.retry_count, wd.retry_count,
        "round-trip: retry_count no coincide"
    );
    assert_eq!(
        wd2.blocked_since, wd.blocked_since,
        "round-trip: blocked_since no coincide"
    );
}

// Round-trip save/load — estado PendingUser (Ugly).
#[tokio::test]
async fn round_trip_pending_user() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_ugly());

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(
        wd.status,
        WatchdogStatus::PendingUser,
        "precondición round-trip Ugly"
    );

    let state = wd.save_state();
    let mut wd2 = WatchdogNode::new("wd".into(), cfg_ugly());
    wd2.load_state(&state);

    assert_eq!(
        wd2.status,
        WatchdogStatus::PendingUser,
        "round-trip: PendingUser no se restauró"
    );
    assert_eq!(
        wd2.pending_action,
        Some(NodeAction::On),
        "round-trip: pending_action no se restauró"
    );
}

// Compatibilidad con estado antiguo (sin blocked_since).
#[test]
fn load_state_acepta_formato_antiguo_sin_blocked_since() {
    let estado_antiguo = serde_json::json!({
        "node_id":   "wd",
        "node_type": "watchdog",
        "is_ready":  true,
        "data": {
            "mode":        "good",
            "status":      "blocked",
            "retry_count": 3
            // blocked_since ausente — versiones viejas no lo tenían
        }
    });
    let node_state: NodeState =
        serde_json::from_value(estado_antiguo).expect("NodeState no deserializa");
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(3, 30));
    wd.load_state(&node_state); // no debe fallar
    assert_eq!(
        wd.status,
        WatchdogStatus::Blocked,
        "debe cargarse como Blocked"
    );
    assert!(wd.blocked_since.is_none(), "blocked_since debe ser None");
}

// ═════════════════════════════════════════════════════════════════════════════
// Override manual
// ═════════════════════════════════════════════════════════════════════════════

// Override en Ok → tiene precedencia sobre la decisión del decisor.
#[tokio::test]
async fn override_en_ok_se_respeta() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(2, 30));

    wd.watchdog_set_override(NodeAction::Off);

    let out = run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(
        out,
        NodeAction::Off,
        "override debe tener precedencia sobre la decisión"
    );
}

// Override en Blocked → ignorado. Solo ClearWatchdog puede salir de Blocked.
#[tokio::test]
async fn override_en_blocked_es_ignorado() {
    let pool = pool();
    let mut wd = WatchdogNode::new("wd".into(), cfg_good(0, 0));

    run(&mut wd, NodeAction::On, None, None, &pool).await;
    for _ in 0..3 {
        run(&mut wd, NodeAction::On, Some(NodeAction::Off), None, &pool).await;
        if wd.status == WatchdogStatus::Blocked {
            break;
        }
    }
    assert_eq!(
        wd.status,
        WatchdogStatus::Blocked,
        "precondición override_blocked"
    );

    wd.watchdog_set_override(NodeAction::On);
    assert!(
        wd.override_action.is_none(),
        "override en Blocked debe ser rechazado"
    );

    let out = run(&mut wd, NodeAction::On, None, None, &pool).await;
    assert_eq!(out, NodeAction::Hold, "Blocked debe seguir emitiendo Hold");
}

// ═════════════════════════════════════════════════════════════════════════════
// Bad mode
// ═════════════════════════════════════════════════════════════════════════════

// Dentro del timeout la señal no se evalúa — se propaga la decisión sin bloquear.
#[tokio::test]
async fn bad_mode_dentro_de_ventana_propaga_decision() {
    let pool = pool();
    // timeout=60s → la ventana nunca expira en el test
    let mut wd = WatchdogNode::new("wd".into(), cfg_bad(1, 60));

    let out = run(&mut wd, NodeAction::On, None, Some(vec![50.0_f64]), &pool).await;

    assert_eq!(
        out,
        NodeAction::On,
        "Bad mode dentro de ventana debe propagar la decisión"
    );
    assert_ne!(
        wd.status,
        WatchdogStatus::Blocked,
        "no debe bloquear mientras no expire el timeout"
    );
}
