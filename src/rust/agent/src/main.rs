// agent/src/main.rs
//
// agrodash-agent — binario standalone.
//
// Args:
//   --process-id <UUID>
//   --api-url    <http://backupserver/api/v1>
//   --api-token  <JWT o token interno>
//   --sock-path  <path del unix socket, default /run/agents/<id>.sock>
//
// Variables de entorno:
//   DATABASE_URL  postgresql://...
//   AGENT_LOG     debug | info | warn | error

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agrodash_shared::{
    AgentCommand, AgentResponse, AgentState, FullAgentState,
    ActuatorAction, ProcessConfig, SourceConfig,
};
use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};

mod filters;
mod decision;
mod actuators;
mod source;

use filters::{Filter, build as build_filter};
use decision::{Decision, build as build_decision};
use actuators::{Actuator, build as build_actuator};
use source::read_source;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
struct Args {
    #[arg(long)] process_id: String,
    #[arg(long)] api_url:    String,
    #[arg(long)] api_token:  String,
    #[arg(long)] sock_path:  Option<PathBuf>,
}

// ── Shared state ──────────────────────────────────────────────────────────────

struct AgentShared {
    config:         RwLock<ProcessConfig>,
    state:          Mutex<AgentState>,
    override_act:   Mutex<Option<ActuatorAction>>,
    stop_flag:      Mutex<bool>,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("AGENT_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL no definida")?;

    let pool = PgPool::connect(&db_url).await.context("No se pudo conectar a PostgreSQL")?;
    info!("Agente {} conectado a PostgreSQL", args.process_id);

    // Cargar config y estado desde API
    let config = fetch_config(&args).await?;
    let saved_state = fetch_state(&args).await.unwrap_or_default();

    let dim = match &config.source {
        SourceConfig::PostgresSensor(s) => s.sensors.len(),
    };

    // Construir pipeline
    let mut filter   = build_filter(&config.filter, dim);
    let mut decision = build_decision(&config.decision);
    let mut actuator = build_actuator(&config.actuator, &config.connections).await?;

    // Warmstart
    filter.load_state(&saved_state.filter);
    decision.load_state(&saved_state.decision);

    info!("Pipeline construido — dim={} filtro={:?} warmup={}",
        dim, config.filter.kind, config.filter.warmup_samples);

    let shared = Arc::new(AgentShared {
        config:       RwLock::new(config),
        state:        Mutex::new(saved_state),
        override_act: Mutex::new(None),
        stop_flag:    Mutex::new(false),
    });

    // Socket server en background
    let sock_path = args.sock_path.clone().unwrap_or_else(|| {
        PathBuf::from(format!("/run/agents/{}.sock", args.process_id))
    });
    let shared_sock = shared.clone();
    let process_id  = args.process_id.clone();
    tokio::spawn(async move {
        if let Err(e) = run_socket_server(sock_path, shared_sock, process_id).await {
            error!("SocketServer error: {e}");
        }
    });

    // Loop principal
    run_loop(&args, &pool, shared.clone(), &mut filter, &mut decision, &mut actuator).await;

    Ok(())
}

// ── Loop principal ────────────────────────────────────────────────────────────

async fn run_loop(
    args:     &Args,
    pool:     &PgPool,
    shared:   Arc<AgentShared>,
    filter:   &mut Box<dyn Filter>,
    decision: &mut Box<dyn Decision>,
    actuator: &mut Box<dyn Actuator>,
) {
    loop {
        // Chequear stop flag
        if *shared.stop_flag.lock().await { break; }

        let t0 = Instant::now();

        let cfg = shared.config.read().await.clone();
        let interval = Duration::from_secs_f64(cfg.loop_interval_seconds);

        match run_cycle(&cfg, pool, filter, decision, actuator, &shared).await {
            Ok(state) => {
                // Persistir estado en API
                if let Err(e) = post_state(args, &state).await {
                    warn!("No se pudo postear estado: {e}");
                }
                *shared.state.lock().await = state;
            }
            Err(e) => {
                error!("Ciclo fallido: {e}");
                // Reportar error al API
                let _ = post_error(args, &e.to_string()).await;
            }
        }

        // Dormir el tiempo restante del intervalo
        let elapsed = t0.elapsed();
        if elapsed < interval {
            sleep(interval - elapsed).await;
        }
    }

    // Checkpoint al salir
    let state = shared.state.lock().await.clone();
    let _ = post_state(args, &state).await;
    info!("Agente {} detenido", args.process_id);
}

async fn run_cycle(
    cfg:      &ProcessConfig,
    pool:     &PgPool,
    filter:   &mut Box<dyn Filter>,
    decision: &mut Box<dyn Decision>,
    actuator: &mut Box<dyn Actuator>,
    shared:   &AgentShared,
) -> Result<AgentState> {
    // 1. Leer fuente
    let raw = read_source(&cfg.source, pool).await
        .context("Error leyendo fuente")?;

    // 2. Filtrar
    let dt = cfg.loop_interval_seconds;
    let x_hat = filter.update(&raw, dt);
    let p_diag = filter.p_diag();
    let is_ready = filter.is_ready();

    // 3. Decidir (solo si el filtro está listo)
    let decision_output = if is_ready {
        // Chequear override manual
        let ov = shared.override_act.lock().await.clone();
        if let Some(action) = ov {
            match action {
                ActuatorAction::On  => decision::DecisionOutput::On,
                ActuatorAction::Off => decision::DecisionOutput::Off,
            }
        } else {
            decision.evaluate(&x_hat, p_diag.as_deref())
        }
    } else {
        info!("Filtro en warmup (n_updates={}), sin acción", filter.save_state().n_updates);
        decision::DecisionOutput::Hold
    };

    // 4. Actuar
    match &decision_output {
        decision::DecisionOutput::On   => actuator.activate().await?,
        decision::DecisionOutput::Off  => actuator.deactivate().await?,
        decision::DecisionOutput::Hold => {}
    }

    // 5. Construir estado
    let filter_state   = filter.save_state();
    let decision_state = decision.state();
    let actuator_state = actuator.state();

    let state = AgentState {
        filter:   filter_state,
        decision: decision_state,
        actuator: actuator_state,
        last_raw: Some(raw),
        cycle:    shared.state.lock().await.cycle + 1,
    };

    info!(
        "Ciclo {} — x_hat={:?} ready={} d={:?} output={:?}",
        state.cycle,
        x_hat.iter().map(|v| format!("{:.4}", v)).collect::<Vec<_>>(),
        is_ready,
        state.decision.last_mahalanobis,
        decision_output,
    );

    Ok(state)
}

// ── Socket server ─────────────────────────────────────────────────────────────

async fn run_socket_server(
    path:       PathBuf,
    shared:     Arc<AgentShared>,
    process_id: String,
) -> Result<()> {
    // Limpiar socket viejo
    if path.exists() { std::fs::remove_file(&path)?; }

    // Crear directorio si no existe
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&path)?;
    info!("Socket escuchando en {:?}", path);

    loop {
        let (stream, _) = listener.accept().await?;
        let shared = shared.clone();
        let pid    = process_id.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, shared, pid).await {
                warn!("Error en conexión socket: {e}");
            }
        });
    }
}

async fn handle_connection(
    stream:     UnixStream,
    shared:     Arc<AgentShared>,
    process_id: String,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let resp = match serde_json::from_str::<AgentCommand>(&line) {
            Err(e) => AgentResponse::err(format!("JSON inválido: {e}")),
            Ok(cmd) => dispatch(cmd, &shared, &process_id).await,
        };
        let mut bytes = serde_json::to_vec(&resp)?;
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
    }
    Ok(())
}

async fn dispatch(
    cmd:        AgentCommand,
    shared:     &AgentShared,
    process_id: &str,
) -> AgentResponse {
    match cmd {
        AgentCommand::GetState => {
            let state  = shared.state.lock().await.clone();
            let cfg    = shared.config.read().await.clone();
            let ov     = shared.override_act.lock().await.clone();
            let labels = match &cfg.source {
                SourceConfig::PostgresSensor(s) => {
                    s.sensors.iter().map(|s| s.label.clone()).collect()
                }
            };
            let full = FullAgentState {
                process_id:      process_id.to_string(),
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
            let cfg = shared.config.read().await.clone();
            AgentResponse::ok(cfg)
        }

        AgentCommand::SetConfig { config } => {
            *shared.config.write().await = config;
            AgentResponse::ok(serde_json::json!({"msg": "config actualizado, efectivo en el próximo ciclo"}))
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
            // El API pide guardar estado antes del deploy
            // El state ya se postea cada ciclo; aquí forzamos un log
            info!("Checkpoint solicitado por el API");
            AgentResponse::ok(serde_json::json!({"msg": "checkpoint anotado"}))
        }

        AgentCommand::Stop => {
            *shared.stop_flag.lock().await = true;
            AgentResponse::ok(serde_json::json!({"msg": "deteniendo..."}))
        }
    }
}

// ── HTTP client → API Rust ────────────────────────────────────────────────────

async fn fetch_config(args: &Args) -> Result<ProcessConfig> {
    let url = format!("{}/processes/{}/config", args.api_url, args.process_id);
    let client = reqwest::Client::new();
    let cfg = client.get(&url)
        .bearer_auth(&args.api_token)
        .send().await?
        .json::<ProcessConfig>().await
        .context("No se pudo parsear config")?;
    Ok(cfg)
}

async fn fetch_state(args: &Args) -> Result<AgentState> {
    let url = format!("{}/processes/{}/agent-state", args.api_url, args.process_id);
    let client = reqwest::Client::new();
    let state = client.get(&url)
        .bearer_auth(&args.api_token)
        .send().await?
        .json::<AgentState>().await?;
    Ok(state)
}

async fn post_state(args: &Args, state: &AgentState) -> Result<()> {
    let url = format!("{}/processes/{}/agent-state", args.api_url, args.process_id);
    reqwest::Client::new()
        .post(&url)
        .bearer_auth(&args.api_token)
        .json(state)
        .send().await?;
    Ok(())
}

async fn post_error(args: &Args, msg: &str) -> Result<()> {
    let url = format!("{}/processes/{}/agent-error", args.api_url, args.process_id);
    reqwest::Client::new()
        .post(&url)
        .bearer_auth(&args.api_token)
        .json(&serde_json::json!({"error": msg}))
        .send().await?;
    Ok(())
}
