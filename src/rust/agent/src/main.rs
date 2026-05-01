// agent/src/main.rs

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agrodash_shared::{
    AgentCommand, AgentResponse, AgentState, FullAgentState,
    NodeAction, ProcessConfig, Signal,
};
use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

mod nodes;
mod scheduler;

use scheduler::PipelineGraph;

#[derive(Parser)]
struct Args {
    #[arg(long)] process_id:  String,
    #[arg(long)] pipeline_id: String,
    #[arg(long)] api_url:     String,
    #[arg(long)] api_token:   String,
    #[arg(long)] sock_path:   Option<PathBuf>,
}

struct AgentShared {
    graph:        RwLock<PipelineGraph>,
    state:        Mutex<AgentState>,
    override_act: Mutex<Option<NodeAction>>,
    stop_flag:    Mutex<bool>,
    process_cfg:  RwLock<ProcessConfig>,
    /// Label legible del pipeline (para FullAgentState)
    pipeline_label: String,
}

#[derive(Clone)]
struct RunCtx {
    process_id:  Uuid,
    pipeline_id: String,
    api_url:     String,
    api_token:   String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let args   = Args::parse();
    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL no definida")?;
    let pool   = PgPool::connect(&db_url).await.context("No se pudo conectar a PostgreSQL")?;

    info!("Agente {}/{} arrancando", args.process_id, args.pipeline_id);

    let proc_config = fetch_config(&args).await?;
    let pipeline    = proc_config.pipelines.iter()
        .find(|p| p.id == args.pipeline_id)
        .cloned()
        .context(format!("Pipeline '{}' no encontrado", args.pipeline_id))?;

    // FIX: guardamos el label legible
    let pipeline_label = pipeline.label.clone();

    let saved_state = fetch_state(&args).await.unwrap_or_default();

    let mut graph = PipelineGraph::build(
        &pipeline.nodes,
        &pipeline.edges,
        &proc_config.shared_connections,
        &pool,
    ).await?;

    graph.load_state(&saved_state);

    info!("Grafo construido — {} nodos, {} edges, ready={}",
        pipeline.nodes.len(), pipeline.edges.len(), graph.is_ready());

    let ctx = RunCtx {
        process_id:  args.process_id.parse().context("process_id inválido")?,
        pipeline_id: args.pipeline_id.clone(),
        api_url:     args.api_url.clone(),
        api_token:   args.api_token.clone(),
    };

    let shared = Arc::new(AgentShared {
        graph:          RwLock::new(graph),
        state:          Mutex::new(saved_state),
        override_act:   Mutex::new(None),
        stop_flag:      Mutex::new(false),
        process_cfg:    RwLock::new(proc_config),
        pipeline_label,
    });

    let sock_path = args.sock_path.clone().unwrap_or_else(|| {
        PathBuf::from(format!("/run/agents/{}/{}.sock", args.process_id, args.pipeline_id))
    });

    let shared_sock = shared.clone();
    let ctx_sock    = ctx.clone();
    let pool_sock   = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = run_socket_server(sock_path, shared_sock, ctx_sock, pool_sock).await {
            error!("Socket error: {e}");
        }
    });

    run_loop(&args, &pool, shared, ctx).await;
    Ok(())
}

// ── Loop principal ────────────────────────────────────────────────────────────

async fn run_loop(args: &Args, pool: &PgPool, shared: Arc<AgentShared>, ctx: RunCtx) {
    loop {
        if *shared.stop_flag.lock().await { break; }

        let t0 = Instant::now();
        let interval = {
            let cfg = shared.process_cfg.read().await;
            let pl  = cfg.pipelines.iter().find(|p| p.id == ctx.pipeline_id);
            Duration::from_secs_f64(pl.map(|p| p.loop_interval_seconds).unwrap_or(60.0))
        };

        let ov = shared.override_act.lock().await.clone();

        let result = {
            let mut graph = shared.graph.write().await;
            graph.run_cycle(pool, interval.as_secs_f64(), &ov).await
        };

        match result {
            Ok(signals) => {
                let cycle    = shared.state.lock().await.cycle + 1;
                let is_ready = shared.graph.read().await.is_ready();
                let state    = shared.graph.read().await.save_state(&ctx.pipeline_id, cycle);

                // Extraer valores para process_readings
                let (raw_vals, filtered_vals, p_diag_vals) = extract_reading_values(&signals, &shared).await;

                let act_str = signals.values()
                    .find_map(|s| match s {
                        Signal::Action(a) => Some(match a {
                            NodeAction::On   => "on",
                            NodeAction::Off  => "off",
                            NodeAction::Hold => "hold",
                        }),
                        _ => None,
                    })
                    .unwrap_or("hold");

                // FIX: escribir raw, filtered, p_diag en process_readings
                let raw_json      = raw_vals.as_ref().map(|v| serde_json::to_value(v).ok()).flatten();
                let filtered_json = filtered_vals.as_ref().map(|v| serde_json::to_value(v).ok()).flatten();
                let p_diag_json   = p_diag_vals.as_ref().map(|v| serde_json::to_value(v).ok()).flatten();

                let _ = sqlx::query!(
                    r#"INSERT INTO process_readings
                       (process_id, pipeline_id, raw, filtered, p_diag, actuator)
                       VALUES ($1::uuid, $2, $3, $4, $5, $6)"#,
                    ctx.process_id,
                    ctx.pipeline_id,
                    raw_json,
                    filtered_json,
                    p_diag_json,
                    act_str,
                )
                .execute(pool)
                .await;

                info!("Ciclo {cycle} ready={is_ready} act={act_str}");

                if let Err(e) = post_state(args, &state).await {
                    warn!("No se pudo postear estado: {e}");
                }
                *shared.state.lock().await = state;
            }
            Err(e) => {
                error!("Ciclo fallido: {e}");
                let _ = post_error(args, &e.to_string()).await;
            }
        }

        let elapsed = t0.elapsed();
        if elapsed < interval { sleep(interval - elapsed).await; }
    }

    let state = shared.state.lock().await.clone();
    let _ = post_state(args, &state).await;
    info!("Agente {}/{} detenido", ctx.process_id, ctx.pipeline_id);
}

/// Extrae raw (fuente), filtered (kalman/ewma/etc) y p_diag del mapa de señales del ciclo.
async fn extract_reading_values(
    signals: &HashMap<String, Signal>,
    shared:  &AgentShared,
) -> (Option<Vec<f64>>, Option<Vec<f64>>, Option<Vec<f64>>) {
    let states = shared.graph.read().await;

    // raw: primer nodo tipo postgres_sensor
    let raw = signals.values().find_map(|s| match s {
        Signal::Vector(v) => Some(v.clone()),
        _ => None,
    });

    // filtered y p_diag: del nodo kalman si existe
    let kalman_state = states.node_state_by_type("kalman");
    let filtered = kalman_state.as_ref()
        .and_then(|ns| ns.data.get("x"))
        .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok());
    let p_diag = kalman_state.as_ref()
        .and_then(|ns| ns.data.get("p"))
        .and_then(|v| serde_json::from_value::<Vec<f64>>(v.clone()).ok());

    // Si no hay kalman, buscar ewma/moving_avg/lowpass
    let filtered = filtered.or_else(|| {
        for kind in &["ewma", "moving_avg", "lowpass"] {
            if let Some(ns) = states.node_state_by_type(kind) {
                let key = if *kind == "moving_avg" { "buf" } else { "y" };
                if let Some(v) = ns.data.get(key) {
                    if let Ok(vecs) = serde_json::from_value::<Vec<Vec<f64>>>(v.clone()) {
                        // moving_avg guarda un buffer — retornar el último
                        return vecs.last().cloned();
                    }
                    if let Ok(vec) = serde_json::from_value::<Vec<f64>>(v.clone()) {
                        return Some(vec);
                    }
                }
            }
        }
        None
    });

    (raw, filtered, p_diag)
}

// ── Socket server ─────────────────────────────────────────────────────────────

async fn run_socket_server(
    path:   PathBuf,
    shared: Arc<AgentShared>,
    ctx:    RunCtx,
    pool:   PgPool,
) -> Result<()> {
    if path.exists() { std::fs::remove_file(&path)?; }
    if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
    let listener = UnixListener::bind(&path)?;
    info!("Socket en {:?}", path);
    loop {
        let (stream, _) = listener.accept().await?;
        let sh = shared.clone();
        let cx = ctx.clone();
        let pl = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, sh, cx, pl).await {
                warn!("Conn error: {e}");
            }
        });
    }
}

async fn handle_conn(
    stream: UnixStream,
    shared: Arc<AgentShared>,
    ctx:    RunCtx,
    pool:   PgPool,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let resp = match serde_json::from_str::<AgentCommand>(&line) {
            Err(e) => AgentResponse::err(format!("JSON inválido: {e}")),
            Ok(cmd) => dispatch(cmd, &shared, &ctx, &pool).await,
        };
        let mut b = serde_json::to_vec(&resp)?;
        b.push(b'\n');
        writer.write_all(&b).await?;
    }
    Ok(())
}

async fn dispatch(
    cmd:    AgentCommand,
    shared: &AgentShared,
    ctx:    &RunCtx,
    pool:   &PgPool,
) -> AgentResponse {
    match cmd {
        AgentCommand::GetState => {
            let state    = shared.state.lock().await.clone();
            let is_ready = shared.graph.read().await.is_ready();
            let ov       = shared.override_act.lock().await.clone();
            let full = FullAgentState {
                pipeline_id:     ctx.pipeline_id.clone(),
                // FIX: usar el label legible guardado al arrancar
                label:           shared.pipeline_label.clone(),
                cycle:           state.cycle,
                is_ready,
                override_active: ov.is_some(),
                node_states:     state.node_states,
                last_signals:    state.last_signals,
            };
            AgentResponse::ok(full)
        }

        AgentCommand::GetConfig => {
            let cfg = shared.process_cfg.read().await.clone();
            AgentResponse::ok(cfg)
        }

        // FIX: SetConfig reconstruye el grafo con el nuevo pipeline config
        AgentCommand::SetConfig { config } => {
            // Buscar el pipeline actualizado en la nueva config
            match config.pipelines.iter().find(|p| p.id == ctx.pipeline_id) {
                None => AgentResponse::err("Pipeline no encontrado en nueva config"),
                Some(pl) => {
                    match PipelineGraph::build(
                        &pl.nodes,
                        &pl.edges,
                        &config.shared_connections,
                        pool,
                    ).await {
                        Err(e) => AgentResponse::err(format!("Error reconstruyendo grafo: {e}")),
                        Ok(new_graph) => {
                            *shared.graph.write().await = new_graph;
                            *shared.process_cfg.write().await = config;
                            AgentResponse::ok(serde_json::json!({
                                "msg": "config actualizado, grafo reconstruido"
                            }))
                        }
                    }
                }
            }
        }

        AgentCommand::Override { action } => {
            *shared.override_act.lock().await = Some(action);
            AgentResponse::ok(serde_json::json!({"override": true}))
        }

        AgentCommand::ClearOverride => {
            *shared.override_act.lock().await = None;
            AgentResponse::ok(serde_json::json!({"override": false}))
        }

        AgentCommand::Checkpoint => {
            info!("Checkpoint recibido");
            let state = shared.state.lock().await.clone();
            let args_stub = ArgsStub {
                api_url:     ctx.api_url.clone(),
                api_token:   ctx.api_token.clone(),
                process_id:  ctx.process_id.to_string(),
                pipeline_id: ctx.pipeline_id.clone(),
            };
            if let Err(e) = post_state_raw(&args_stub, &state).await {
                warn!("Checkpoint: no se pudo guardar estado: {e}");
            }
            AgentResponse::ok(serde_json::json!({"msg": "checkpoint ok"}))
        }

        AgentCommand::Stop => {
            *shared.stop_flag.lock().await = true;
            AgentResponse::ok(serde_json::json!({"msg": "deteniendo"}))
        }

        // SelfTest: dry-run de un ciclo sin actuadores, reporta estado por nodo
        AgentCommand::SelfTest => {
            run_self_test(shared, ctx, pool).await
        }
    }
}

// ── Self-test (dry-run) ───────────────────────────────────────────────────────

async fn run_self_test(
    shared: &AgentShared,
    ctx:    &RunCtx,
    pool:   &PgPool,
) -> AgentResponse {
    let t0 = Instant::now();
    let interval = {
        let cfg = shared.process_cfg.read().await;
        let pl  = cfg.pipelines.iter().find(|p| p.id == ctx.pipeline_id);
        Duration::from_secs_f64(pl.map(|p| p.loop_interval_seconds).unwrap_or(60.0))
    };

    // Dry-run: None override para no forzar actuadores, pero sí corremos el ciclo
    let result = {
        let mut graph = shared.graph.write().await;
        graph.run_cycle_dry(pool, interval.as_secs_f64()).await
    };

    let elapsed_ms = t0.elapsed().as_millis();

    match result {
        Err(e) => AgentResponse::err(format!("Dry-run falló: {e}")),
        Ok(signals) => {
            let node_results: Vec<serde_json::Value> = signals.iter().map(|(id, sig)| {
                serde_json::json!({
                    "node_id": id,
                    "signal":  match sig {
                        Signal::Vector(v) => serde_json::json!({"type":"vector","values":v}),
                        Signal::Action(a) => serde_json::json!({"type":"action","value":format!("{a:?}")}),
                    }
                })
            }).collect();

            AgentResponse::ok(serde_json::json!({
                "status":     "ok",
                "elapsed_ms": elapsed_ms,
                "is_ready":   shared.graph.read().await.is_ready(),
                "nodes":      node_results,
            }))
        }
    }
}

// ── Stub para post_state sin Args ─────────────────────────────────────────────

struct ArgsStub {
    api_url:     String,
    api_token:   String,
    process_id:  String,
    pipeline_id: String,
}

async fn post_state_raw(args: &ArgsStub, state: &AgentState) -> Result<()> {
    reqwest::Client::new()
        .post(format!("{}/processes/{}/agent-state/{}",
            args.api_url, args.process_id, args.pipeline_id))
        .bearer_auth(&args.api_token)
        .json(state)
        .send().await?;
    Ok(())
}

// ── API helpers ───────────────────────────────────────────────────────────────

async fn fetch_config(args: &Args) -> Result<ProcessConfig> {
    reqwest::Client::new()
        .get(format!("{}/processes/{}/config", args.api_url, args.process_id))
        .bearer_auth(&args.api_token)
        .send().await?
        .json::<ProcessConfig>().await
        .context("No se pudo parsear config")
}

async fn fetch_state(args: &Args) -> Result<AgentState> {
    reqwest::Client::new()
        .get(format!("{}/processes/{}/agent-state/{}",
            args.api_url, args.process_id, args.pipeline_id))
        .bearer_auth(&args.api_token)
        .send().await?
        .json::<AgentState>().await
        .context("No se pudo cargar estado")
}

async fn post_state(args: &Args, state: &AgentState) -> Result<()> {
    reqwest::Client::new()
        .post(format!("{}/processes/{}/agent-state/{}",
            args.api_url, args.process_id, args.pipeline_id))
        .bearer_auth(&args.api_token)
        .json(state)
        .send().await?;
    Ok(())
}

async fn post_error(args: &Args, msg: &str) -> Result<()> {
    reqwest::Client::new()
        .post(format!("{}/processes/{}/agent-error", args.api_url, args.process_id))
        .bearer_auth(&args.api_token)
        .json(&serde_json::json!({
            "error":       msg,
            "pipeline_id": args.pipeline_id,
        }))
        .send().await?;
    Ok(())
}
