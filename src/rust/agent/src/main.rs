// agent/src/main.rs
//
// Arquitectura desired-state + PG LISTEN/NOTIFY:
//
//   Al arrancar:
//     1. Leer processes.status — si es 'stopping' o 'stopped', salir.
//     2. Leer config de DB (processes.config).
//     3. Leer estado guardado (pipeline_states).
//     4. Leer override_action de pipeline_states.
//     5. LISTEN "agent_cmd_{pipeline_id}" en conexión dedicada.
//
//   En cada ciclo:
//     - Ejecutar grafo con override si aplica.
//     - Escribir process_readings.
//     - Cada N ciclos: guardar pipeline_states.
//     - Procesar comandos NOTIFY pendientes (canal mpsc).
//
//   Al recibir NOTIFY:
//     - Stop       → marcar stop_flag, terminar loop.
//     - Override   → aplicar a actuador(es) con validación.
//     - ClearOverride → limpiar override.
//     - Reload     → re-leer config de DB, reconstruir grafo.
//     - Checkpoint → guardar estado ahora.
//     - SelfTest   → dry-run, escribir en process_self_test.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agrodash_shared::{
    AgentCommand, AgentCmdResult, AgentState, NodeAction, ProcessConfig,
};
use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use sqlx::PgPool;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

mod nodes;
mod scheduler;

use scheduler::PipelineGraph;

// ── Args ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
struct Args {
    #[arg(long)] process_id:  String,
    #[arg(long)] pipeline_id: String,
}

// ── Shared state ──────────────────────────────────────────────────────────────

struct AgentShared {
    graph:          RwLock<PipelineGraph>,
    override_acts:  RwLock<HashMap<String, NodeAction>>, // actuator_id → action
    stop_flag:      tokio::sync::Mutex<bool>,
    cycle:          tokio::sync::Mutex<u64>,
    pipeline_label: String,
    process_id:     Uuid,
    pipeline_id:    String,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let args      = Args::parse();
    let db_url    = std::env::var("DATABASE_URL").context("DATABASE_URL no definida")?;
    let pool      = PgPool::connect(&db_url).await.context("Falló conexión a PostgreSQL")?;
    let process_id: Uuid = args.process_id.parse().context("process_id inválido")?;

    info!("Agente {}/{} arrancando", args.process_id, args.pipeline_id);

    // ── 1. Desired state: verificar si debería estar corriendo ────────────────
    let status = sqlx::query_scalar!(
        "SELECT status FROM processes WHERE id = $1",
        process_id
    )
    .fetch_optional(&pool).await?
    .ok_or_else(|| anyhow::anyhow!("Proceso {} no encontrado", process_id))?;

    if status == "stopping" || status == "stopped" {
        info!("Proceso en estado '{status}' — agente no arranca");
        return Ok(());
    }

    // ── 2. Cargar config de DB ────────────────────────────────────────────────
    let proc_config = fetch_config(&pool, process_id).await?;
    let pipeline = proc_config.pipelines.iter()
        .find(|p| p.id == args.pipeline_id)
        .cloned()
        .context(format!("Pipeline '{}' no encontrado en config", args.pipeline_id))?;

    let pipeline_label = pipeline.label.clone();
    let loop_secs      = pipeline.loop_interval_seconds;

    // ── 3. Cargar estado guardado ─────────────────────────────────────────────
    let saved_state = fetch_state(&pool, process_id, &args.pipeline_id).await
        .unwrap_or_default();

    // ── 4. Cargar override desde desired state en DB ──────────────────────────
    let override_acts = load_overrides(&pool, process_id, &args.pipeline_id).await;

    // ── 5. Construir grafo ────────────────────────────────────────────────────
    let mut graph = PipelineGraph::build(
        &pipeline.nodes,
        &pipeline.edges,
        &proc_config.shared_connections,
        &pool,
    ).await?;
    graph.load_state(&saved_state);

    info!("Grafo construido — {} nodos, ready={}",
        pipeline.nodes.len(), graph.is_ready());

    let shared = Arc::new(AgentShared {
        graph:          RwLock::new(graph),
        override_acts:  RwLock::new(override_acts),
        stop_flag:      tokio::sync::Mutex::new(false),
        cycle:          tokio::sync::Mutex::new(saved_state.cycle),
        pipeline_label,
        process_id,
        pipeline_id:    args.pipeline_id.clone(),
    });

    // ── 6. Canal de comandos NOTIFY ───────────────────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::channel::<AgentCommand>(32);
    let pool_listener   = pool.clone();
    let pipeline_id_l   = args.pipeline_id.clone();
    tokio::spawn(async move {
        run_listener(pool_listener, pipeline_id_l, cmd_tx).await;
    });

    // ── 7. Loop principal ─────────────────────────────────────────────────────
    run_loop(&pool, shared, cmd_rx, loop_secs).await;

    info!("Agente {}/{} terminado", args.process_id, args.pipeline_id);
    Ok(())
}

// ── PG LISTENER ───────────────────────────────────────────────────────────────

async fn run_listener(pool: PgPool, pipeline_id: String, tx: mpsc::Sender<AgentCommand>) {
    let channel = format!("agent_cmd_{pipeline_id}");

    loop {
        // sqlx::PgListener necesita conexión propia — no comparte pool
        let mut listener = match sqlx::postgres::PgListener::connect_with(&pool).await {
            Ok(l)  => l,
            Err(e) => {
                error!("Listener: no se pudo conectar: {e} — reintento en 5s");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        if let Err(e) = listener.listen(&channel).await {
            error!("Listener: LISTEN falló: {e} — reintento en 5s");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        info!("Listener: escuchando en '{channel}'");

        loop {
            match listener.recv().await {
                Ok(notif) => {
                    let payload = notif.payload();
                    match serde_json::from_str::<AgentCommand>(payload) {
                        Ok(cmd) => {
                            if tx.send(cmd).await.is_err() {
                                return; // canal cerrado — agente terminó
                            }
                        }
                        Err(e) => {
                            warn!("Listener: payload inválido '{payload}': {e}");
                        }
                    }
                }
                Err(e) => {
                    error!("Listener: error de recepción: {e} — reconectando");
                    break; // salir del loop interno → reconectar
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

// ── Loop principal ────────────────────────────────────────────────────────────

async fn run_loop(
    pool:    &PgPool,
    shared:  Arc<AgentShared>,
    mut cmd_rx: mpsc::Receiver<AgentCommand>,
    loop_secs:  f64,
) {
    let interval = Duration::from_secs_f64(loop_secs.max(1.0));
    const SAVE_EVERY: u64 = 6; // guardar estado cada N ciclos

    loop {
        // Procesar todos los comandos pendientes antes del ciclo
        while let Ok(cmd) = cmd_rx.try_recv() {
            if !process_cmd(pool, &shared, cmd).await {
                // Stop recibido
                save_state(pool, &shared).await;
                return;
            }
        }

        if *shared.stop_flag.lock().await { break; }

        let t0 = Instant::now();

        // Ejecutar ciclo del grafo
        let ov_snapshot = shared.override_acts.read().await.clone();
        let result = {
            let mut graph = shared.graph.write().await;
            graph.run_cycle(pool, interval.as_secs_f64(), &ov_snapshot).await
        };

        let cycle = {
            let mut c = shared.cycle.lock().await;
            *c += 1;
            *c
        };

        match result {
            Err(e) => {
                error!("Ciclo {cycle} fallido: {e}");
                write_agent_error(pool, shared.process_id, &shared.pipeline_id, &e.to_string()).await;
            }
            Ok(signals) => {
                let is_ready  = shared.graph.read().await.is_ready();
                let act_str   = actuator_state_str(&signals);
                let (raw, filtered, p_diag) = extract_readings(&signals, &shared).await;

                write_readings(pool, &shared, &raw, &filtered, &p_diag, &act_str).await;

                info!("Ciclo {cycle} ready={is_ready} act={act_str}");

                // Guardar estado cada N ciclos
                if cycle % SAVE_EVERY == 0 {
                    save_state(pool, &shared).await;
                }
            }
        }

        // Esperar el resto del intervalo
        let elapsed = t0.elapsed();
        if elapsed < interval {
            // Procesar comandos mientras esperamos — así el stop es inmediato
            let wait = interval - elapsed;
            match tokio::time::timeout(wait, cmd_rx.recv()).await {
                Ok(Some(cmd)) => {
                    if !process_cmd(pool, &shared, cmd).await { break; }
                }
                Ok(None) => break, // canal cerrado
                Err(_)   => {}     // timeout normal
            }
        }
    }

    save_state(pool, &shared).await;
    mark_stopped(pool, shared.process_id).await;
}

// ── Procesador de comandos ────────────────────────────────────────────────────
//
// Retorna false si el agente debe detenerse.

async fn process_cmd(pool: &PgPool, shared: &Arc<AgentShared>, cmd: AgentCommand) -> bool {
    match cmd {
        // ── Stop ──────────────────────────────────────────────────────────────
        AgentCommand::Stop => {
            info!("Comando Stop recibido — deteniendo");
            *shared.stop_flag.lock().await = true;
            write_cmd_result(pool, shared, "stop", true, "Agente deteniéndose", None).await;
            return false;
        }

        // ── Override ──────────────────────────────────────────────────────────
        AgentCommand::Override { action, actuator_id } => {
            let actuators = shared.graph.read().await.actuator_ids();

            if actuators.is_empty() {
                write_cmd_result(pool, shared, "override", false,
                    "Este pipeline no tiene actuadores configurados", None).await;
                return true;
            }

            let target_id = match actuator_id {
                Some(id) => {
                    if !actuators.contains(&id) {
                        let msg = format!(
                            "Actuador '{}' no existe. Actuadores disponibles: {}",
                            id, actuators.join(", ")
                        );
                        write_cmd_result(pool, shared, "override", false, &msg, None).await;
                        return true;
                    }
                    id
                }
                None if actuators.len() == 1 => actuators[0].clone(),
                None => {
                    let msg = format!(
                        "Este pipeline tiene {} actuadores: {}. Especificá cuál con actuator_id.",
                        actuators.len(), actuators.join(", ")
                    );
                    write_cmd_result(pool, shared, "override", false, &msg, None).await;
                    return true;
                }
            };

            shared.override_acts.write().await.insert(target_id.clone(), action.clone());
            persist_overrides(pool, shared).await;

            let msg = format!("Override {:?} aplicado a '{target_id}'", action);
            info!("{msg}");
            write_cmd_result(pool, shared, "override", true, &msg, None).await;
        }

        // ── ClearOverride ─────────────────────────────────────────────────────
        AgentCommand::ClearOverride { actuator_id } => {
            let mut ovs = shared.override_acts.write().await;

            if ovs.is_empty() {
                write_cmd_result(pool, shared, "clear_override", false,
                    "No hay overrides activos", None).await;
                return true;
            }

            match actuator_id {
                Some(id) => { ovs.remove(&id); }
                None     => { ovs.clear(); }
            }
            drop(ovs);
            persist_overrides(pool, shared).await;

            write_cmd_result(pool, shared, "clear_override", true,
                "Override limpiado — modo automático", None).await;
        }

        // ── Reload config ─────────────────────────────────────────────────────
        AgentCommand::Reload => {
            match fetch_config(pool, shared.process_id).await {
                Err(e) => {
                    error!("Reload: no se pudo leer config: {e}");
                    write_cmd_result(pool, shared, "reload", false,
                        &format!("Error leyendo config: {e}"), None).await;
                }
                Ok(cfg) => {
                    match cfg.pipelines.iter().find(|p| p.id == shared.pipeline_id) {
                        None => {
                            write_cmd_result(pool, shared, "reload", false,
                                "Pipeline no encontrado en la nueva config", None).await;
                        }
                        Some(pl) => {
                            match PipelineGraph::build(
                                &pl.nodes, &pl.edges, &cfg.shared_connections, pool
                            ).await {
                                Err(e) => {
                                    error!("Reload: error construyendo grafo: {e}");
                                    write_cmd_result(pool, shared, "reload", false,
                                        &format!("Error en grafo: {e}"), None).await;
                                }
                                Ok(new_graph) => {
                                    *shared.graph.write().await = new_graph;
                                    let msg = format!("Config recargada — {} nodos",
                                        pl.nodes.len());
                                    info!("{msg}");
                                    write_cmd_result(pool, shared, "reload", true, &msg, None).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Checkpoint ────────────────────────────────────────────────────────
        AgentCommand::Checkpoint => {
            info!("Checkpoint solicitado");
            save_state(pool, shared).await;
            write_cmd_result(pool, shared, "checkpoint", true, "Estado guardado", None).await;
        }

        // ── SelfTest ──────────────────────────────────────────────────────────
        AgentCommand::SelfTest => {
            let t0 = Instant::now();
            let result = {
                let mut graph = shared.graph.write().await;
                graph.run_cycle_dry(pool, 1.0).await
            };
            let elapsed = t0.elapsed().as_millis();

            match result {
                Err(e) => {
                    write_cmd_result(pool, shared, "self_test", false,
                        &format!("Dry-run falló: {e}"), None).await;
                }
                Ok(signals) => {
                    let is_ready = shared.graph.read().await.is_ready();
                    let nodes: Vec<serde_json::Value> = signals.iter().map(|(id, sig)| {
                        serde_json::json!({
                            "node_id": id,
                            "signal": match sig {
                                agrodash_shared::Signal::Vector(v) =>
                                    serde_json::json!({"type":"vector","values":v}),
                                agrodash_shared::Signal::Action(a) =>
                                    serde_json::json!({"type":"action","value":format!("{a:?}")}),
                            }
                        })
                    }).collect();

                    let data = serde_json::json!({
                        "elapsed_ms": elapsed,
                        "is_ready":   is_ready,
                        "nodes":      nodes,
                    });

                    let msg = format!("Dry-run ok en {elapsed}ms, is_ready={is_ready}");
                    info!("{msg}");
                    write_cmd_result(pool, shared, "self_test", true, &msg, Some(data)).await;
                }
            }
        }
    }

    true // continuar
}

// ── Helpers de escritura ──────────────────────────────────────────────────────

async fn write_cmd_result(
    pool:    &PgPool,
    shared:  &AgentShared,
    cmd:     &str,
    ok:      bool,
    message: &str,
    data:    Option<serde_json::Value>,
) {
    let result = AgentCmdResult {
        pipeline_id: shared.pipeline_id.clone(),
        cmd:         cmd.to_string(),
        ok,
        message:     message.to_string(),
        data,
        ts:          Utc::now().to_rfc3339(),
    };

    let _ = sqlx::query!(
        r#"INSERT INTO agent_cmd_results
           (pipeline_id, process_id, cmd, ok, message, data, ts)
           VALUES ($1, $2, $3, $4, $5, $6, now())"#,
        shared.pipeline_id,
        shared.process_id,
        result.cmd,
        result.ok,
        result.message,
        result.data,
    )
    .execute(pool)
    .await;

    if !ok {
        warn!("[{}] cmd={cmd} error: {message}", shared.pipeline_id);
    }
}

async fn save_state(pool: &PgPool, shared: &AgentShared) {
    let cycle = *shared.cycle.lock().await;
    let state = shared.graph.read().await.save_state(&shared.pipeline_id, cycle);

    let _ = sqlx::query!(
        r#"INSERT INTO pipeline_states (process_id, pipeline_id, state, updated_at)
           VALUES ($1, $2, $3, now())
           ON CONFLICT (process_id, pipeline_id)
           DO UPDATE SET state = $3, updated_at = now()"#,
        shared.process_id,
        shared.pipeline_id,
        serde_json::to_value(&state).unwrap_or_default(),
    )
    .execute(pool)
    .await;
}

async fn persist_overrides(pool: &PgPool, shared: &AgentShared) {
    let ovs = shared.override_acts.read().await.clone();
    let json = serde_json::to_value(&ovs).unwrap_or_default();

    let _ = sqlx::query!(
        r#"INSERT INTO pipeline_states (process_id, pipeline_id, override_action, updated_at)
           VALUES ($1, $2, $3, now())
           ON CONFLICT (process_id, pipeline_id)
           DO UPDATE SET override_action = $3, updated_at = now()"#,
        shared.process_id,
        shared.pipeline_id,
        json,
    )
    .execute(pool)
    .await;
}

async fn mark_stopped(pool: &PgPool, process_id: Uuid) {
    let _ = sqlx::query!(
        "UPDATE processes SET status='stopped', updated_at=now() WHERE id=$1",
        process_id
    )
    .execute(pool)
    .await;
}

async fn write_agent_error(pool: &PgPool, process_id: Uuid, pipeline_id: &str, msg: &str) {
    let _ = sqlx::query!(
        "INSERT INTO process_logs (process_id, level, source, message)
         VALUES ($1, 'error', $2, $3)",
        process_id,
        format!("agent:{pipeline_id}"),
        msg,
    )
    .execute(pool)
    .await;
}

async fn write_readings(
    pool:     &PgPool,
    shared:   &AgentShared,
    raw:      &Option<Vec<f64>>,
    filtered: &Option<Vec<f64>>,
    p_diag:   &Option<Vec<f64>>,
    act_str:  &str,
) {
    let _ = sqlx::query!(
        r#"INSERT INTO process_readings
           (process_id, pipeline_id, raw, filtered, p_diag, actuator)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
        shared.process_id,
        shared.pipeline_id,
        raw.as_ref().and_then(|v| serde_json::to_value(v).ok()),
        filtered.as_ref().and_then(|v| serde_json::to_value(v).ok()),
        p_diag.as_ref().and_then(|v| serde_json::to_value(v).ok()),
        act_str,
    )
    .execute(pool)
    .await;

    // Actualizar last_seen_at del proceso
    let _ = sqlx::query!(
        "UPDATE processes SET last_seen_at=now() WHERE id=$1",
        shared.process_id
    )
    .execute(pool)
    .await;
}

// ── DB reads ──────────────────────────────────────────────────────────────────

async fn fetch_config(pool: &PgPool, process_id: Uuid) -> Result<ProcessConfig> {
    let row = sqlx::query!(
        "SELECT config FROM processes WHERE id = $1",
        process_id
    )
    .fetch_one(pool).await?;

    serde_json::from_value(row.config).context("Config inválida")
}

async fn fetch_state(pool: &PgPool, process_id: Uuid, pipeline_id: &str) -> Result<AgentState> {
    let row = sqlx::query!(
        "SELECT state FROM pipeline_states WHERE process_id=$1 AND pipeline_id=$2",
        process_id, pipeline_id,
    )
    .fetch_optional(pool).await?;

    match row {
        Some(r) => serde_json::from_value(r.state).context("Estado inválido"),
        None    => Ok(AgentState::default()),
    }
}

async fn load_overrides(
    pool:        &PgPool,
    process_id:  Uuid,
    pipeline_id: &str,
) -> HashMap<String, NodeAction> {
    let row = sqlx::query!(
        "SELECT override_action FROM pipeline_states WHERE process_id=$1 AND pipeline_id=$2",
        process_id, pipeline_id,
    )
    .fetch_optional(pool).await;

    match row {
        Ok(Some(r)) if r.override_action.is_some() => {
            serde_json::from_value(r.override_action.unwrap()).unwrap_or_default()
        }
        _ => HashMap::new(),
    }
}

// ── Signal helpers ────────────────────────────────────────────────────────────

fn actuator_state_str(signals: &HashMap<String, agrodash_shared::Signal>) -> String {
    signals.values().find_map(|s| match s {
        agrodash_shared::Signal::Action(a) => Some(match a {
            NodeAction::On   => "on",
            NodeAction::Off  => "off",
            NodeAction::Hold => "hold",
        }),
        _ => None,
    })
    .unwrap_or("hold")
    .to_string()
}

async fn extract_readings(
    signals: &HashMap<String, agrodash_shared::Signal>,
    shared:  &AgentShared,
) -> (Option<Vec<f64>>, Option<Vec<f64>>, Option<Vec<f64>>) {
    let graph = shared.graph.read().await;

    let raw = signals.values().find_map(|s| match s {
        agrodash_shared::Signal::Vector(v) => Some(v.clone()),
        _ => None,
    });

    let kalman_state = graph.node_state_by_type("kalman");
    let filtered = kalman_state.as_ref()
        .and_then(|ns| ns.data.get("x"))
        .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok())
        .or_else(|| {
            for kind in &["ewma", "moving_avg", "lowpass"] {
                if let Some(ns) = graph.node_state_by_type(kind) {
                    if let Some(v) = ns.data.get("y") {
                        if let Ok(vec) = serde_json::from_value::<Vec<f64>>(v.clone()) {
                            return Some(vec);
                        }
                    }
                }
            }
            None
        });

    let p_diag = kalman_state.as_ref()
        .and_then(|ns| ns.data.get("p"))
        .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok());

    (raw, filtered, p_diag)
}