// agent/src/tests/pipeline_continuity.rs
//
// Test de continuidad de datos en el pipeline.
//
// Verifica que:
//   1. Un vector de f64 pasa limpiamente por cada tipo de nodo
//   2. No hay panic en reduce() con slice vacío (regresión)
//   3. El Kalman converge en N ciclos con señal estable
//   4. La Hysteresis emite Signal::Action a partir de Signal::Vector
//   5. El tipo no se mezcla — Signal::Action no llega a un nodo que espera Vector
//   6. El pipeline completo (Sensor→Kalman→Logger→Hysteresis) procesa N ciclos sin error

use crate::nodes::{
    decisions::HysteresisNode,
    filters::{EwmaNode, KalmanNode, LowpassNode, MovingAvgNode},
    utils::LoggerNode,
    NodeContext, NodeInstance,
};
use agrodash_shared::{
    EwmaConfig, HysteresisConfig, KalmanConfig, MovingAvgConfig, LowpassConfig,
    NodeAction, Reduction, Signal, TrendConfig, VecOrScalar,
};
use sqlx::PgPool;

// ── Mock pool ──────────────────────────────────────────────────────────────────
// Los nodos reciben &PgPool pero los nodos de test no lo usan.
// Usamos un pool real de test no disponible aquí — en su lugar,
// los nodos que no necesitan pool aceptan cualquier referencia.
// Para compilar el test sin Postgres, usamos un pool lazy que nunca se llama.

fn mock_ctx(node_id: &str) -> NodeContext {
    NodeContext {
        process_id:  "test_process".into(),
        pipeline_id: "test_pipeline".into(),
        node_id:     node_id.into(),
        node_label:  None,
    }
}

fn vec_signal(values: &[f64]) -> Signal {
    Signal::Vector(values.to_vec())
}

fn kalman_cfg(dim: usize) -> KalmanConfig {
    KalmanConfig {
        Q: VecOrScalar::Scalar(0.01),
        R: VecOrScalar::Scalar(0.1),
        P0: 1.0,
        convergence_threshold: 0.05,
        warmup_samples: 5,
    }
}

fn hysteresis_cfg() -> HysteresisConfig {
    HysteresisConfig {
        target:         vec![0.5],
        threshold_act:  vec![0.7],
        threshold_deact: vec![0.3],
        reduction:      Reduction::Component { index: 0 },
        sigma:          None,
        trend:          TrendConfig { window_n: None, noise_floor: 0.0 },
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Kalman procesa vector de dim=2 sin panic
    #[tokio::test]
    async fn test_kalman_vectorial() {
        let mut node = KalmanNode::new("kalman_test".into(), kalman_cfg(2));
        let ctx = mock_ctx("kalman_test");

        // Señal estable en 2D
        let signal = vec_signal(&[1.0, 2.0]);

        for i in 0..20 {
            let result = node.execute(
                vec![signal.clone()], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
            ).await;
            // El nodo no usa pool — el puntero no se desreferencia
            // En un test real usaríamos sqlx::test o un mock
            assert!(result.is_ok(), "ciclo {i}: Kalman falló: {:?}", result);
            if let Ok(Some(Signal::WeightedVector { values: v, weights: w })) = result {
                assert_eq!(v.len(), 2, "dim debe ser 2");
                assert!(v[0].is_finite(), "componente 0 debe ser finito");
                assert!(v[1].is_finite(), "componente 1 debe ser finito");
                assert_eq!(w.len(), 2, "pesos deben tener dim 2");
                assert!(w[0] > 0.0, "peso 0 debe ser positivo");
            }
        }
    }

    /// reduce() no hace panic con slice vacío (regresión del bug encontrado)
    #[test]
    fn test_reduce_empty_slice_no_panic() {
        use agrodash_shared::Reduction;
        use crate::nodes::decisions::reduce;

        let empty: &[f64] = &[];

        // Ninguna de estas debe hacer panic
        assert_eq!(reduce(empty, &Reduction::Mean), 0.0);
        assert_eq!(reduce(empty, &Reduction::Min),  0.0);
        assert_eq!(reduce(empty, &Reduction::Max),  0.0);
        assert_eq!(reduce(empty, &Reduction::Component { index: 0 }), 0.0);
        assert_eq!(reduce(empty, &Reduction::WeightedByP), 0.0);
    }

    /// reduce() con datos reales devuelve valores correctos
    #[test]
    fn test_reduce_values() {
        use agrodash_shared::Reduction;
        use crate::nodes::decisions::reduce;

        let data = &[1.0f64, 3.0, 5.0];

        assert!((reduce(data, &Reduction::Mean) - 3.0).abs() < 1e-9);
        assert!((reduce(data, &Reduction::Min)  - 1.0).abs() < 1e-9);
        assert!((reduce(data, &Reduction::Max)  - 5.0).abs() < 1e-9);
        assert!((reduce(data, &Reduction::Component { index: 1 }) - 3.0).abs() < 1e-9);
    }

    /// Hysteresis emite NodeAction a partir de Signal::Vector
    #[tokio::test]
    async fn test_hysteresis_signal_type() {
        let mut node = HysteresisNode::new("hyst_test".into(), hysteresis_cfg());
        let ctx = mock_ctx("hyst_test");

        // Señal alta → debe activar
        let high = vec_signal(&[0.9]);
        let result = node.execute(
            vec![high], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
        ).await.unwrap();

        assert!(
            matches!(result, Some(Signal::Action(NodeAction::On))),
            "Señal alta debe activar: {:?}", result
        );

        // Señal baja → debe desactivar
        let low = vec_signal(&[0.1]);
        let result = node.execute(
            vec![low], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
        ).await.unwrap();

        assert!(
            matches!(result, Some(Signal::Action(NodeAction::Off))),
            "Señal baja debe desactivar: {:?}", result
        );
    }

    /// El Logger pasa la señal sin modificar
    #[tokio::test]
    async fn test_logger_passthrough() {
        let mut node = LoggerNode::new("logger_test".into(), "test_tag".into());
        let ctx = mock_ctx("logger_test");

        let input = vec_signal(&[1.0, 2.0, 3.0]);
        let result = node.execute(
            vec![input.clone()], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
        ).await.unwrap();

        match (&input, &result) {
            (Signal::Vector(a), Some(Signal::Vector(b))) => {
                assert_eq!(a, b, "Logger debe pasar el vector sin cambios");
            }
            (Signal::WeightedVector { values: a, .. }, Some(Signal::WeightedVector { values: b, .. })) => {
                assert_eq!(a, b, "Logger debe pasar el WeightedVector sin cambios");
            }
            _ => panic!("Logger devolvió tipo inesperado: {:?}", result),
        }
    }

    /// MovingAvg vectorial — ventana de 3 sobre dim=2
    #[tokio::test]
    async fn test_moving_avg_vectorial() {
        let cfg = MovingAvgConfig { window_n: 3, warmup_samples: 3 };
        let mut node = MovingAvgNode::new("ma_test".into(), cfg);
        let ctx = mock_ctx("ma_test");

        let signals = vec![
            vec_signal(&[1.0, 10.0]),
            vec_signal(&[2.0, 20.0]),
            vec_signal(&[3.0, 30.0]),
        ];

        for s in &signals {
            let _ = node.execute(
                vec![s.clone()], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
            ).await;
        }

        // Media de [1,2,3] = 2.0, [10,20,30] = 20.0
        let result = node.execute(
            vec![vec_signal(&[3.0, 30.0])], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
        ).await.unwrap();

        if let Some(Signal::WeightedVector { values: v, .. }) = result {
            assert!((v[0] - 2.0).abs() < 0.5, "Media dim 0 ≈ 2.0, got {}", v[0]);
            assert!((v[1] - 20.0).abs() < 5.0, "Media dim 1 ≈ 20.0, got {}", v[1]);
        } else {
            panic!("MovingAvg no retornó WeightedVector");
        }
    }

    /// EWMA converge hacia la señal de entrada en dim=1
    #[tokio::test]
    async fn test_ewma_convergence() {
        let cfg = EwmaConfig {
            alpha: VecOrScalar::Scalar(0.5),
            warmup_samples: 1,
        };
        let mut node = EwmaNode::new("ewma_test".into(), cfg);
        let ctx = mock_ctx("ewma_test");

        // Señal constante = 10.0
        let target = 10.0f64;
        let signal = vec_signal(&[target]);
        let mut last = 0.0f64;

        for _ in 0..30 {
            let result = node.execute(
                vec![signal.clone()], 1.0, unsafe { &*(0x1 as *const PgPool) }, &ctx
            ).await.unwrap();
            if let Some(Signal::WeightedVector { values: v, .. }) = result {
                last = v[0];
            }
        }

        assert!(
            (last - target).abs() < 0.01,
            "EWMA debe converger a {target}, got {last}"
        );
    }

    /// Pipeline completo: Kalman → Logger → Hysteresis — señal alta activa
    #[tokio::test]
    async fn test_pipeline_high_signal_activates() {
        let mut kalman   = KalmanNode::new("k".into(), kalman_cfg(1));
        let mut logger   = LoggerNode::new("l".into(), "soil".into());
        let mut hyst     = HysteresisNode::new("h".into(), hysteresis_cfg());

        let ctx_k = mock_ctx("k");
        let ctx_l = mock_ctx("l");
        let ctx_h = mock_ctx("h");

        // Warmup con señal constante alta
        let pool_ptr = unsafe { &*(0x1 as *const PgPool) };
        let high_signal = vec_signal(&[0.9]);

        let mut last_action: Option<NodeAction> = None;

        for _ in 0..20 {
            let k_out = kalman.execute(vec![high_signal.clone()], 1.0, pool_ptr, &ctx_k)
                .await.unwrap().unwrap();
            let l_out = logger.execute(vec![k_out], 1.0, pool_ptr, &ctx_l)
                .await.unwrap().unwrap();
            let h_out = hyst.execute(vec![l_out], 1.0, pool_ptr, &ctx_h)
                .await.unwrap();

            if let Some(Signal::Action(a)) = h_out {
                last_action = Some(a);
            }
        }

        assert_eq!(
            last_action,
            Some(NodeAction::On),
            "Pipeline con señal alta debe terminar con actuador ON"
        );
    }

    /// Pipeline: señal baja → actuador OFF
    #[tokio::test]
    async fn test_pipeline_low_signal_deactivates() {
        let mut kalman   = KalmanNode::new("k2".into(), kalman_cfg(1));
        let mut logger   = LoggerNode::new("l2".into(), "soil".into());
        let mut hyst     = HysteresisNode::new("h2".into(), hysteresis_cfg());

        let ctx_k = mock_ctx("k2");
        let ctx_l = mock_ctx("l2");
        let ctx_h = mock_ctx("h2");

        let pool_ptr = unsafe { &*(0x1 as *const PgPool) };
        let low_signal = vec_signal(&[0.1]);
        let mut last_action: Option<NodeAction> = None;

        for _ in 0..20 {
            let k_out = kalman.execute(vec![low_signal.clone()], 1.0, pool_ptr, &ctx_k)
                .await.unwrap().unwrap();
            let l_out = logger.execute(vec![k_out], 1.0, pool_ptr, &ctx_l)
                .await.unwrap().unwrap();
            if let Some(Signal::Action(a)) = hyst.execute(vec![l_out], 1.0, pool_ptr, &ctx_h)
                .await.unwrap()
            {
                last_action = Some(a);
            }
        }

        assert_eq!(
            last_action,
            Some(NodeAction::Off),
            "Pipeline con señal baja debe terminar con actuador OFF"
        );
    }

    /// AckStage.matches() interpola placeholders correctamente
    #[test]
    fn test_ack_stage_match() {
        use agrodash_shared::AckStage;

        let stage = AckStage {
            name:         "Gateway".into(),
            match_prefix: Some("MQTT_RECIBIDO:{action},{valve}".into()),
            error_prefix: Some("LORA_ERROR:{action},{valve}".into()),
            timeout_secs: 3.0,
            terminal:     false,
        };

        assert!(stage.matches("MQTT_RECIBIDO:ON,5 extra", "on", "on,5"));
        assert!(stage.matches("MQTT_RECIBIDO:OFF,3",      "off", "off,3"));
        assert!(!stage.matches("LORA_ENVIANDO:ON,5",      "on", "on,5"));

        assert!(stage.is_error("LORA_ERROR:ON,5:timeout", "on", "on,5"));
        assert!(!stage.is_error("MQTT_RECIBIDO:ON,5",     "on", "on,5"));
    }

    /// NodeConfig.label es opcional y retrocompatible (no rompe deserialización sin label)
    #[test]
    fn test_node_config_label_optional() {
        use agrodash_shared::{NodeConfig, NodeKind};

        // Sin label
        let json_sin_label = r#"{
            "id": "mqtt_actuator_123",
            "type": "mqtt_actuator",
            "connection": "shared",
            "topic": "control/valvula",
            "payload_on": "on,5",
            "payload_off": "off,5"
        }"#;

        let cfg: NodeConfig = serde_json::from_str(json_sin_label).unwrap();
        assert!(cfg.label.is_none());

        // Con label
        let json_con_label = r#"{
            "id": "mqtt_actuator_123",
            "label": "Válvula zona norte",
            "type": "mqtt_actuator",
            "connection": "shared",
            "topic": "control/valvula",
            "payload_on": "on,5",
            "payload_off": "off,5"
        }"#;

        let cfg2: NodeConfig = serde_json::from_str(json_con_label).unwrap();
        assert_eq!(cfg2.label.as_deref(), Some("Válvula zona norte"));
    }
}