// api/src/agent_manager_tests.rs
//
// Tests del AgentManager. Declarado desde agent_manager.rs con:
//   #[cfg(test)]
//   #[path = "agent_manager_tests.rs"]
//   mod agent_manager_tests;
//
// Todos estos tests requieren DATABASE_URL — el CI la provee como secret.
// Ninguno spawna procesos reales (el binario no existe en CI durante los tests).
//
// Cómo correr solo estos tests:
//   cargo test -p agrodash-api agent_manager -- --nocapture

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

async fn manager() -> Arc<AgentManager> {
    let url = std::env::var("DATABASE_URL").expect("CI debe proveer DATABASE_URL");
    let pool = PgPool::connect(&url)
        .await
        .expect("no se pudo conectar a la DB de test");
    AgentManager::new(AgentManagerConfig::default(), pool)
}

// ─────────────────────────────────────────────────────────────────────────────
// stop_process
//
// Invariante: stop_process debe remover la key del mapa interno.
// Si no la remueve, el monitor intenta reiniciar un proceso que el usuario
// detuvo intencionalmente.
// ─────────────────────────────────────────────────────────────────────────────

// stop_process con proceso inexistente no debe fallar ni dejar estado sucio.
#[tokio::test]
async fn stop_process_inexistente_no_falla() {
    let mgr = manager().await;
    let fake_id = Uuid::new_v4();

    mgr.stop_process(fake_id).await;

    let activos = mgr.active_pipelines(fake_id).await;
    assert!(
        activos.is_empty(),
        "stop_process dejó entradas en el mapa para un proceso inexistente"
    );
}

// stop_process debe remover la key del mapa aunque no haya binario real.
// Insertamos una entrada dummy directamente en el mapa interno para simular
// un agente activo sin necesidad de spawnar un proceso real.
#[tokio::test]
async fn stop_process_remueve_del_mapa() {
    let mgr = manager().await;
    let process_id = Uuid::new_v4();
    let pipeline_id = "pl_test".to_string();

    // Insertar entrada dummy usando un child de `true` (siempre termina con 0).
    // En CI el binario `true` existe en /usr/bin/true.
    let key = AgentKey {
        process_id,
        pipeline_id: pipeline_id.clone(),
    };
    let child = tokio::process::Command::new("true")
        .spawn()
        .expect("no se pudo spawnear 'true' — ¿estás en un entorno muy restringido?");

    mgr.agents
        .write()
        .await
        .insert(key, Mutex::new(AgentEntry { child, restarts: 0 }));

    // Verificar que está en el mapa.
    assert!(
        mgr.active_pipelines(process_id)
            .await
            .contains(&pipeline_id),
        "precondición: el pipeline debe estar en el mapa"
    );

    // stop_process debe removerlo.
    mgr.stop_process(process_id).await;

    assert!(
        mgr.active_pipelines(process_id).await.is_empty(),
        "stop_process no removió el pipeline del mapa"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// notify
//
// Invariante: notify es fire-and-forget. Nunca debe fallar aunque el agente
// no esté escuchando. El desired state en DB garantiza la ejecución eventual.
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn notify_no_falla_sin_agente_activo() {
    let mgr = manager().await;
    let key = AgentKey {
        process_id: Uuid::new_v4(),
        pipeline_id: "pl_test_notify".into(),
    };

    let result = mgr.notify(&key, AgentCommand::Checkpoint).await;
    assert!(
        result.is_ok(),
        "notify falló con agente inactivo: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn notify_todos_los_comandos_serializan_correctamente() {
    let mgr = manager().await;
    let key = AgentKey {
        process_id: Uuid::new_v4(),
        pipeline_id: "pl_test_cmds".into(),
    };

    // Verificamos que cada comando llega a pg_notify sin error de serialización.
    // Si AgentCommand añade una variante sin actualizar serde, esto falla.
    let comandos = vec![
        AgentCommand::Stop,
        AgentCommand::Checkpoint,
        AgentCommand::Reload,
        AgentCommand::SelfTest,
        AgentCommand::ClearWatchdog { actuator_id: None },
        AgentCommand::ClearWatchdog {
            actuator_id: Some("bomba_1".into()),
        },
        AgentCommand::ConfirmWatchdog { actuator_id: None },
        AgentCommand::ConfirmWatchdog {
            actuator_id: Some("bomba_1".into()),
        },
        AgentCommand::Override {
            action: agrodash_shared::NodeAction::On,
            actuator_id: None,
        },
    ];

    for cmd in comandos {
        let result = mgr.notify(&key, cmd).await;
        assert!(
            result.is_ok(),
            "notify falló para {:?}: {:?}",
            result,
            result.err()
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// active_pipelines
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn active_pipelines_vacio_para_proceso_nuevo() {
    let mgr = manager().await;
    let fake_id = Uuid::new_v4();

    let activos = mgr.active_pipelines(fake_id).await;
    assert!(
        activos.is_empty(),
        "active_pipelines debe ser vacío para un proceso recién creado"
    );
}

#[tokio::test]
async fn active_pipelines_solo_devuelve_pipelines_del_proceso() {
    let mgr = manager().await;

    let proc_a = Uuid::new_v4();
    let proc_b = Uuid::new_v4();

    // Insertar pipelines para dos procesos distintos.
    for (proc_id, pl_id) in [(proc_a, "pl_a1"), (proc_a, "pl_a2"), (proc_b, "pl_b1")] {
        let key = AgentKey {
            process_id: proc_id,
            pipeline_id: pl_id.into(),
        };
        let child = tokio::process::Command::new("true").spawn().unwrap();
        mgr.agents
            .write()
            .await
            .insert(key, Mutex::new(AgentEntry { child, restarts: 0 }));
    }

    let activos_a = mgr.active_pipelines(proc_a).await;
    let activos_b = mgr.active_pipelines(proc_b).await;

    assert_eq!(activos_a.len(), 2, "proc_a debe tener 2 pipelines activos");
    assert_eq!(activos_b.len(), 1, "proc_b debe tener 1 pipeline activo");
    assert!(activos_a.contains(&"pl_a1".to_string()));
    assert!(activos_a.contains(&"pl_a2".to_string()));
    assert!(activos_b.contains(&"pl_b1".to_string()));

    // Limpiar
    mgr.stop_process(proc_a).await;
    mgr.stop_process(proc_b).await;
}
