// agent/src/main.rs — agrodash-agent
//
// Un agente por pipeline. Recibe:
//   --process-id  UUID del proceso
//   --pipeline-id ID del pipeline dentro del proceso
//   --api-url     URL base del API Rust
//   --api-token   Token interno
//   --sock-path   Path del Unix socket

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agrodash_shared::{
    AgentCommand, AgentResponse, AgentState, FullAgentState,
    ActuatorAction, ProcessConfig, PipelineConfig, SourceConfig,
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

mod filters;
mod decision;
mod actuators;
mod source;

use filters::{Filter, build as build_filter};
use decision::{Decision, build as build_decision, DecisionOutput};
use actuators::{Actuator, build as build_actuator};
use source::read_source;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
struct Args {
    #[arg(long)] process_id:  String,
    #[arg(long)] pipeline_id: String,
    #[arg(long)] api_url:     String,
    #[arg(long)] api_token:   String,
    #[arg(long)] sock_path:   Option<PathBuf>,
}

// ── Shared state ──────────────────────────────────────────────────────────────

struct AgentShared {
    pipeline:       RwLock<PipelineConfig>,
    state:          Mutex<AgentState>,
    override_act:   Mutex<Option<ActuatorAction>>,
    stop_flag:      Mutex<bool>,
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct RunCtx {
    process_id:  Uuid,
    pipeline_id: String,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL no definida")?;
    let pool   = PgPool::connect(&db_url).await.context("No se pudo conectar a PostgreSQL")?;

    info!("Agente {}/{} conectado a PostgreSQL", args.process_id, args.pipeline_id);

    // Cargar config completa y extraer el pipeline
    let proc_config = fetch_config(&args).await?;
    let pipeline = proc_config.pipelines.iter()
        .find(|p| p.id == args.pipeline_id)
        .cloned()
        .context(format!("Pipeline '{}' no encontrado en config", args.pipeline_id))?;

    let saved_state = fetch_state(&args).await.unwrap_or_default();

    let dim = match &pipeline.source {
        SourceConfig::PostgresSensor(s) => s.sensors.len(),
    };

    // Construir pipeline
    let mut filter   = build_filter(&pipeline.filter, dim);
    let mut decision = build_decision(&pipeline.decision);
    let mut actuator = build_actuator(&pipeline.actuator, &pipeline.connections).await?;

    filter.load_state(&saved_state.filter);
    decision.load_state(&saved_state.decision);

    info!("Pipeline construido — dim={} filtro={:?} warmup={}",
        dim, pipeline.filter.kind, pipeline.filter.warmup_samples);

    let shared = Arc::new(AgentShared {
        pipeline:     RwLock::new(pipeline),
        state:        Mutex::new(saved_state),
        override_act: Mutex::new(None),
        stop_flag:    Mutex::new(false),
    });

    let ctx = RunCtx {
        process_id:  args.process_id.parse().context("process_id inválido")?,
        pipeline_id: args.pipeline_id.clone(),
    };

    // Socket server
    let sock_path = args.sock_path.clone().unwrap_or_else(|| {
        PathBuf::from(format!("/run/agents/{}/{}.sock",
            args.process_id, args.pipeline_id))
    });

    let shared_sock = shared.clone();
    let ctx_sock    = ctx.clone();
    tokio::spawn(async move {
        if let Err(e) = run_socket_server(sock_path, shared_sock, ctx_sock).await {
            error!("SocketServer error: {e}");
        }
    });

    // Loop principal
    run_loop(&args, &pool, shared.clone(), ctx.clone(),
        &mut filter, &mut decision, &mut actuator).await;

    Ok(())
}

// ── Loop principal ────────────────────────────────────────────────────────────

async fn run_loop(
    args:     &Args,
    pool:     &PgPool,
    shared:   Arc<AgentShared>,
    ctx:      RunCtx,
    filter:   &mut Box<dyn Filter>,
    decision: &mut Box<dyn Decision>,
    actuator: &mut Box<dyn Actuator>,
) {
    loop {
        if *shared.stop_flag.lock().await { break; }

        let t0       = Instant::now();
        let interval = {
            let p = shared.pipeline.read().await;
            Duration::from_secs_f64(p.loop_interval_seconds)
        };

        let pipeline = shared.pipeline.read().await.clone();

        match run_cycle(&pipeline, pool, filter, decision, actuator, &shared, &ctx).await {
            Ok(state) => {
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

async fn run_cycle(
    pipeline: &PipelineConfig,
    pool:     &PgPool,
    filter:   &mut Box<dyn Filter>,
    decision: &mut Box<dyn Decision>,
    actuator: &mut Box<dyn Actuator>,
    shared:   &AgentShared,
    ctx:      &RunCtx,
) -> Result<AgentState> {
    // 1. Leer fuente
    let raw = read_source(&pipeline.source, pool).await
        .context("Error leyendo fuente")?;

    // 2. Filtrar
    let x_hat  = filter.update(&raw, pipeline.loop_interval_seconds);
    let p_diag = filter.p_diag();
    let is_ready = filter.is_ready();

    // 3. Decidir
    let output = if is_ready {
        let ov = shared.override_act.lock().await.clone();
        match ov {
            Some(ActuatorAction::On)  => DecisionOutput::On,
            Some(ActuatorAction::Off) => DecisionOutput::Off,
            None => decision.evaluate(&x_hat, p_diag.as_deref()),
        }
    } else {
        info!("Warmup ({} updates)", filter.save_state().n_updates);
        DecisionOutput::Hold
    };

    // 4. Actuar
    match &output {
        DecisionOutput::On   => actuator.activate().await?,
        DecisionOutput::Off  => actuator.deactivate().await?,
        DecisionOutput::Hold => {}
    }

    // 5. Estado
    let filter_state   = filter.save_state();
    let decision_state = decision.state();
    let actuator_state = actuator.state();

    let cycle = shared.state.lock().await.cycle + 1;
    let state = AgentState {
        pipeline_id: ctx.pipeline_id.clone(),
        filter:      filter_state.clone(),
        decision:    decision_state.clone(),
        actuator:    actuator_state.clone(),
        last_raw:    Some(raw.clone()),
        cycle,
    };

    // 6. Escribir a process_readings
    let act_str = match &output {
        DecisionOutput::On   => "on",
        DecisionOutput::Off  => "off",
        DecisionOutput::Hold => "hold",
    };
    let _ = sqlx::query!(
        r#"
        INSERT INTO process_readings
            (process_id, pipeline_id, raw, filtered, p_diag, decision, actuator)
        VALUES ($1::uuid, $2, $3::jsonb, $4::jsonb, $5::jsonb, $6, $7)
        "#,
        ctx.process_id,
        ctx.pipeline_id,
        serde_json::to_value(&raw).ok(),
        serde_json::to_value(&x_hat).ok(),
        p_diag.as_ref().and_then(|p| serde_json::to_value(p).ok()),
        decision_state.last_mahalanobis.or(decision_state.last_reduced),
        act_str,
    )
    .execute(pool)
    .await;

    info!("Ciclo {cycle} — ready={is_ready} d={:?} output={output:?}",
        decision_state.last_mahalanobis);

    Ok(state)
}

// ── Socket server ─────────────────────────────────────────────────────────────

async fn run_socket_server(
    path:   PathBuf,
    shared: Arc<AgentShared>,
    ctx:    RunCtx,
) -> Result<()> {
    if path.exists() { std::fs::remove_file(&path)?; }
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }

    let listener = UnixListener::bind(&path)?;
    info!("Socket en {:?}", path);

    loop {
        let (stream, _) = listener.accept().await?;
        let shared = shared.clone();
        let ctx    = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, shared, ctx).await {
                warn!("Socket error: {e}");
            }
        });
    }
}

async fn handle_conn(
    stream: UnixStream,
    shared: Arc<AgentShared>,
    ctx:    RunCtx,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let resp = match serde_json::from_str::<AgentCommand>(&line) {
            Err(e) => AgentResponse::err(format!("JSON inválido: {e}")),
            Ok(cmd) => dispatch(cmd, &shared, &ctx).await,
        };
        let mut bytes = serde_json::to_vec(&resp)?;
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
    }
    Ok(())
}

async fn dispatch(cmd: AgentCommand, shared: &AgentShared, ctx: &RunCtx) -> AgentResponse {
    match cmd {
        AgentCommand::GetState => {
            let state    = shared.state.lock().await.clone();
            let pipeline = shared.pipeline.read().await.clone();
            let ov       = shared.override_act.lock().await.clone();
            let labels   = match &pipeline.source {
                SourceConfig::PostgresSensor(s) => {
                    s.sensors.iter().map(|s| s.label.clone()).collect()
                }
            };
            let full = FullAgentState {
                pipeline_id:     ctx.pipeline_id.clone(),
                cycle:           state.cycle,
                is_ready:        state.filter.is_ready,
                override_active: ov.is_some(),
                filter:          state.filter,
                decision:        state.decision,
                actuator:        state.actuator,
                last_raw:        state.last_raw,
                sensor_labels:   labels,
            };
            AgentResponse::ok(full)
        }
        AgentCommand::GetConfig => {
            AgentResponse::ok(shared.pipeline.read().await.clone())
        }
        AgentCommand::SetConfig { config } => {
            *shared.pipeline.write().await = config;
            AgentResponse::ok(serde_json::json!({"msg": "config actualizado"}))
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

// ── HTTP client → API ─────────────────────────────────────────────────────────

async fn fetch_config(args: &Args) -> Result<ProcessConfig> {
    let url = format!("{}/processes/{}/config", args.api_url, args.process_id);
    reqwest::Client::new()
        .get(&url).bearer_auth(&args.api_token)
        .send().await?.json::<ProcessConfig>().await
        .context("No se pudo parsear config")
}

async fn fetch_state(args: &Args) -> Result<AgentState> {
    let url = format!("{}/processes/{}/agent-state/{}",
        args.api_url, args.process_id, args.pipeline_id);
    reqwest::Client::new()
        .get(&url).bearer_auth(&args.api_token)
        .send().await?.json::<AgentState>().await
        .context("No se pudo cargar estado")
}

async fn post_state(args: &Args, state: &AgentState) -> Result<()> {
    let url = format!("{}/processes/{}/agent-state/{}",
        args.api_url, args.process_id, args.pipeline_id);
    reqwest::Client::new()
        .post(&url).bearer_auth(&args.api_token)
        .json(state).send().await?;
    Ok(())
}

async fn post_error(args: &Args, msg: &str) -> Result<()> {
    let url = format!("{}/processes/{}/agent-error", args.api_url, args.process_id);
    reqwest::Client::new()
        .post(&url).bearer_auth(&args.api_token)
        .json(&serde_json::json!({"error": msg, "pipeline_id": args.pipeline_id}))
        .send().await?;
    Ok(())
}
