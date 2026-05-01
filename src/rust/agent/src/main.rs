// agent/src/main.rs

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agrodash_shared::{
    AgentCommand, AgentResponse, AgentState, FullAgentState,
    NodeAction, ProcessConfig,
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
    pipeline_label: String,     // label legible del pipeline
}

#[derive(Clone)]
struct RunCtx {
    process_id:  Uuid,
    pipeline_id: String,
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

    let pipeline_label = pipeline.label.clone();

    let saved_state = fetch_state(&args).await.unwrap_or_default();

    let mut graph = PipelineGraph::build(
        &pipeline.nodes,
        &pipeline.edges,
        &proc_config.shared_connections,
        &pipeline.connections,
        &pool,
    ).await?;

    graph.load_state(&saved_state);

    info!("Grafo construido — {} nodos, {} edges, ready={}",
        pipeline.nodes.len(), pipeline.edges.len(), graph.is_ready());

    let ctx = RunCtx {
        process_id:  args.process_id.parse().context("process_id inválido")?,
        pipeline_id: args.pipeline_id.clone(),
    };

    let shared = Arc::new(AgentShared {
        graph:          RwLock::new(graph),
        state:          Mutex::new(saved_state),
        override_act:   Mutex::new(None),
        stop_flag:      Mutex::new(false),
        process_cfg:    RwLock::new(proc_config),
        pipeline_label,
    });

    // Socket Unix
    let sock_path = args.sock_path.clone().unwrap_or_else(|| {
        PathBuf::from(format!("/run/agents/{}/{}.sock", args.process_id, args.pipeline_id))
    });
    let shared_sock = shared.clone();
    let ctx_sock    = ctx.clone();
    tokio::spawn(async move {
        if let Err(e) = run_socket_server(sock_path, shared_sock, ctx_sock).await {
            error!("Socket error: {e}");
        }
    });

    run_loop(&args, &pool, shared, ctx).await;
    Ok(())
}

async fn run_loop(args: &Args, pool: &PgPool, shared: Arc<AgentShared>, ctx: RunCtx) {
    loop {
        if *shared.stop_flag.lock().await { break; }

        let t0       = Instant::now();
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
            Ok(cycle_result) => {
                let cycle    = shared.state.lock().await.cycle + 1;
                let is_ready = shared.graph.read().await.is_ready();
                let state    = shared.graph.read().await.save_state(&ctx.pipeline_id, cycle);

                // Escribir process_readings con raw, filtered, p_diag y actuator
                let raw_json      = cycle_result.raw.as_ref()
                    .map(|v| serde_json::to_value(v).ok())
                    .flatten();
                let filtered_json = cycle_result.filtered.as_ref()
                    .map(|v| serde_json::to_value(v).ok())
                    .flatten();
                let p_diag_json   = cycle_result.p_diag.as_ref()
                    .map(|v| serde_json::to_value(v).ok())
                    .flatten();

                let _ = sqlx::query!(
                    r#"INSERT INTO process_readings
                       (process_id, pipeline_id, raw, filtered, p_diag, decision, actuator)
                       VALUES ($1::uuid, $2, $3, $4, $5, $6, $7)"#,
                    ctx.process_id,
                    ctx.pipeline_id,
                    raw_json,
                    filtered_json,
                    p_diag_json,
                    cycle_result.decision,
                    cycle_result.actuator,
                )
                .execute(pool)
                .await;

                info!("Ciclo {cycle} ready={is_ready} act={}", cycle_result.actuator);

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

async fn run_socket_server(path: PathBuf, shared: Arc<AgentShared>, ctx: RunCtx) -> Result<()> {
    if path.exists() { std::fs::remove_file(&path)?; }
    if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
    let listener = UnixListener::bind(&path)?;
    info!("Socket en {:?}", path);
    loop {
        let (stream, _) = listener.accept().await?;
        let sh = shared.clone(); let cx = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, sh, cx).await {
                warn!("Conn error: {e}");
            }
        });
    }
}

async fn handle_conn(stream: UnixStream, shared: Arc<AgentShared>, ctx: RunCtx) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let resp = match serde_json::from_str::<AgentCommand>(&line) {
            Err(e) => AgentResponse::err(format!("JSON inválido: {e}")),
            Ok(cmd) => dispatch(cmd, &shared, &ctx).await,
        };
        let mut b = serde_json::to_vec(&resp)?; b.push(b'\n');
        writer.write_all(&b).await?;
    }
    Ok(())
}

async fn dispatch(cmd: AgentCommand, shared: &AgentShared, ctx: &RunCtx) -> AgentResponse {
    match cmd {
        AgentCommand::GetState => {
            let state    = shared.state.lock().await.clone();
            let is_ready = shared.graph.read().await.is_ready();
            let ov       = shared.override_act.lock().await.clone();
            let full = FullAgentState {
                pipeline_id:     ctx.pipeline_id.clone(),
                label:           shared.pipeline_label.clone(),   // label correcto
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
        AgentCommand::SetConfig { config } => {
            // Hot-reload: actualizar label y reconstruir grafo si es posible.
            // El rebuild necesita pool, que no tenemos aquí, así que solo
            // actualizamos el config y el label. El cambio estructural requiere restart.
            let new_label = config.label.clone();
            let mut proc_cfg = shared.process_cfg.write().await;
            if let Some(pl) = proc_cfg.pipelines.iter_mut()
                .find(|p| p.id == config.id)
            {
                *pl = config;
            }
            drop(proc_cfg);
            // Nota: el label del AgentShared no es mutable post-construcción.
            // Para cambios de label, el manager debe reiniciar el agente.
            AgentResponse::ok(serde_json::json!({
                "msg": "config actualizado — para cambios estructurales reiniciá el agente",
                "new_label": new_label,
            }))
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
            info!("Checkpoint solicitado");
            AgentResponse::ok(serde_json::json!({"msg": "ok"}))
        }
        AgentCommand::Stop => {
            *shared.stop_flag.lock().await = true;
            AgentResponse::ok(serde_json::json!({"msg": "deteniendo"}))
        }
    }
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

async fn fetch_config(args: &Args) -> Result<ProcessConfig> {
    reqwest::Client::new()
        .get(format!("{}/processes/{}/config", args.api_url, args.process_id))
        .bearer_auth(&args.api_token)
        .send().await?.json::<ProcessConfig>().await
        .context("No se pudo parsear config")
}

async fn fetch_state(args: &Args) -> Result<AgentState> {
    reqwest::Client::new()
        .get(format!("{}/processes/{}/agent-state/{}", args.api_url, args.process_id, args.pipeline_id))
        .bearer_auth(&args.api_token)
        .send().await?.json::<AgentState>().await
        .context("No se pudo cargar estado")
}

async fn post_state(args: &Args, state: &AgentState) -> Result<()> {
    reqwest::Client::new()
        .post(format!("{}/processes/{}/agent-state/{}", args.api_url, args.process_id, args.pipeline_id))
        .bearer_auth(&args.api_token)
        .json(state).send().await?;
    Ok(())
}

async fn post_error(args: &Args, msg: &str) -> Result<()> {
    reqwest::Client::new()
        .post(format!("{}/processes/{}/agent-error", args.api_url, args.process_id))
        .bearer_auth(&args.api_token)
        .json(&serde_json::json!({"error": msg, "pipeline_id": args.pipeline_id}))
        .send().await?;
    Ok(())
}