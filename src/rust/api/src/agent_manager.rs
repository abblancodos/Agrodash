// api/src/agent_manager.rs
//
// Gestiona el ciclo de vida de los agentes.
// Un proceso tiene N pipelines → N agentes.
// Cada agente tiene su socket en /run/agents/<process_id>/<pipeline_id>.sock

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agrodash_shared::{AgentCommand, AgentResponse, ProcessConfig};
use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AgentManagerConfig {
    pub agent_bin:     PathBuf,
    pub api_url:       String,
    pub api_token:     String,
    pub sock_dir:      PathBuf,
    pub database_url:  String,
    pub max_restarts:  u32,
    pub restart_delay: Duration,
}

impl Default for AgentManagerConfig {
    fn default() -> Self {
        Self {
            agent_bin:     PathBuf::from(
                std::env::var("AGENT_BIN")
                    .unwrap_or_else(|_| "/app/agrodash-agent".into())
            ),
            api_url:       std::env::var("API_INTERNAL_URL")
                               .unwrap_or_else(|_| "http://localhost:3000/api/v1".into()),
            api_token:     std::env::var("AGENT_TOKEN")
                               .unwrap_or_else(|_| "internal-token".into()),
            sock_dir:      PathBuf::from("/run/agents"),
            database_url:  std::env::var("DATABASE_URL").unwrap_or_default(),
            max_restarts:  5,
            restart_delay: Duration::from_secs(5),
        }
    }
}

// ── AgentKey — identifica un agente concreto ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentKey {
    pub process_id:  Uuid,
    pub pipeline_id: String,
}

struct AgentEntry {
    key:      AgentKey,
    child:    Child,
    restarts: u32,
}

// ── Manager ───────────────────────────────────────────────────────────────────

pub struct AgentManager {
    cfg:    AgentManagerConfig,
    pool:   PgPool,
    agents: Arc<RwLock<HashMap<AgentKey, Mutex<AgentEntry>>>>,
}

impl AgentManager {
    pub fn new(cfg: AgentManagerConfig, pool: PgPool) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            pool,
            agents: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Arrancar todos los procesos en estado 'running'.
    pub async fn start_all(self: &Arc<Self>) {
        let rows = sqlx::query!(
            "SELECT id, config FROM processes WHERE status = 'running'"
        )
        .fetch_all(&self.pool)
        .await;

        match rows {
            Ok(rows) => {
                info!("AgentManager: {} procesos a arrancar", rows.len());
                for row in rows {
                    if let Ok(cfg) = serde_json::from_value::<ProcessConfig>(row.config) {
                        for pipeline in &cfg.pipelines {
                            self.spawn(row.id, pipeline.id.clone()).await;
                        }
                    }
                }
            }
            Err(e) => error!("Error cargando procesos: {e}"),
        }
    }

    /// Spawnear un agente para un pipeline específico.
    pub async fn spawn(self: &Arc<Self>, process_id: Uuid, pipeline_id: String) {
        let key = AgentKey { process_id, pipeline_id: pipeline_id.clone() };

        match self.spawn_child(&key).await {
            Ok(child) => {
                let entry = AgentEntry { key: key.clone(), child, restarts: 0 };
                self.agents.write().await.insert(key.clone(), Mutex::new(entry));
                info!("Agente {}/{} spawnado", process_id, pipeline_id);

                let mgr = Arc::clone(self);
                tokio::spawn(async move { mgr.monitor(key).await });
            }
            Err(e) => {
                error!("No se pudo spawnear agente {}/{}: {e}", process_id, pipeline_id);
                self.mark_status(process_id, "error").await;
            }
        }
    }

    /// Detener todos los agentes de un proceso.
    pub async fn stop_process(self: &Arc<Self>, process_id: Uuid) {
        let keys: Vec<AgentKey> = {
            self.agents.read().await
                .keys()
                .filter(|k| k.process_id == process_id)
                .cloned()
                .collect()
        };
        for key in keys {
            self.stop_agent(&key).await;
        }
    }

    /// Graceful shutdown de todos los agentes.
    pub async fn shutdown_all(self: &Arc<Self>) {
        info!("AgentManager: shutdown de todos los agentes");
        let keys: Vec<AgentKey> = self.agents.read().await.keys().cloned().collect();

        // Checkpoint en paralelo
        let futs: Vec<_> = keys.iter()
            .map(|k| self.socket_cmd(k, AgentCommand::Checkpoint))
            .collect();
        futures::future::join_all(futs).await;

        sleep(Duration::from_secs(4)).await;

        let mut agents = self.agents.write().await;
        for entry in agents.values() {
            let mut e = entry.lock().await;
            let _ = e.child.kill().await;
        }
        agents.clear();
        info!("AgentManager: todos los agentes detenidos");
    }

    /// Mandar un comando a un agente específico.
    pub async fn socket_cmd(
        &self,
        key: &AgentKey,
        cmd: AgentCommand,
    ) -> Option<AgentResponse> {
        let sock = self.sock_path(key);

        // Esperar hasta 3s a que el socket exista
        for _ in 0..6 {
            if sock.exists() { break; }
            sleep(Duration::from_millis(500)).await;
        }
        if !sock.exists() { return None; }

        let stream = UnixStream::connect(&sock).await.ok()?;
        let (reader, mut writer) = stream.into_split();

        let mut payload = serde_json::to_vec(&cmd).ok()?;
        payload.push(b'\n');
        writer.write_all(&payload).await.ok()?;

        let mut lines = BufReader::new(reader).lines();
        let line = lines.next_line().await.ok()??;
        serde_json::from_str(&line).ok()
    }

    /// Retorna los sockets activos de un proceso para el SSE.
    pub fn process_sockets(&self, process_id: Uuid) -> Vec<(String, PathBuf)> {
        // Sync — solo lee keys
        // En producción esto se haría async; por simplicidad usamos try_read
        if let Ok(agents) = self.agents.try_read() {
            agents.keys()
                .filter(|k| k.process_id == process_id)
                .map(|k| (k.pipeline_id.clone(), self.sock_path(k)))
                .collect()
        } else {
            vec![]
        }
    }

    // ── Internos ──────────────────────────────────────────────────────────────

    async fn spawn_child(&self, key: &AgentKey) -> anyhow::Result<Child> {
        let sock = self.sock_path(key);
        std::fs::create_dir_all(sock.parent().unwrap_or(&self.cfg.sock_dir))?;

        let child = Command::new(&self.cfg.agent_bin)
            .arg("--process-id").arg(key.process_id.to_string())
            .arg("--pipeline-id").arg(&key.pipeline_id)
            .arg("--api-url").arg(&self.cfg.api_url)
            .arg("--api-token").arg(&self.cfg.api_token)
            .arg("--sock-path").arg(&sock)
            .env("DATABASE_URL", &self.cfg.database_url)
            .env("RUST_LOG", std::env::var("AGENT_LOG").unwrap_or_else(|_| "info".into()))
            .kill_on_drop(false)
            .spawn()?;

        Ok(child)
    }

    async fn stop_agent(&self, key: &AgentKey) {
        let _ = self.socket_cmd(key, AgentCommand::Checkpoint).await;
        sleep(Duration::from_secs(3)).await;
        let agents = self.agents.write().await;
        if let Some(entry) = agents.get(key) {
            let _ = entry.lock().await.child.kill().await;
        }
    }

    async fn monitor(self: Arc<Self>, key: AgentKey) {
        let mut delay = self.cfg.restart_delay;

        loop {
            // Esperar muerte del child
            let exit = {
                let agents = self.agents.read().await;
                if let Some(entry) = agents.get(&key) {
                    entry.lock().await.child.wait().await.ok()
                } else { break; }
            };

            let code = exit.and_then(|s| s.code()).unwrap_or(-1);
            warn!("Agente {}/{} terminó con código {code}",
                key.process_id, key.pipeline_id);

            let restarts = {
                let agents = self.agents.read().await;
                agents.get(&key)
                    .map(|e| futures::executor::block_on(async { e.lock().await.restarts }))
                    .unwrap_or(0)
            };

            if restarts >= self.cfg.max_restarts {
                error!("Agente {}/{} alcanzó máximo de reinicios",
                    key.process_id, key.pipeline_id);
                self.mark_status(key.process_id, "error").await;
                self.agents.write().await.remove(&key);
                break;
            }

            warn!("Reiniciando {}/{} en {}s (intento {})",
                key.process_id, key.pipeline_id, delay.as_secs(), restarts);
            sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(60));

            match self.spawn_child(&key).await {
                Ok(child) => {
                    let agents = self.agents.read().await;
                    if let Some(entry) = agents.get(&key) {
                        let mut e = entry.lock().await;
                        e.child    = child;
                        e.restarts += 1;
                    }
                    info!("Agente {}/{} reiniciado", key.process_id, key.pipeline_id);
                }
                Err(e) => {
                    error!("No se pudo reiniciar {}/{}: {e}",
                        key.process_id, key.pipeline_id);
                    self.mark_status(key.process_id, "error").await;
                    self.agents.write().await.remove(&key);
                    break;
                }
            }
        }
    }

    fn sock_path(&self, key: &AgentKey) -> PathBuf {
        self.cfg.sock_dir
            .join(key.process_id.to_string())
            .join(format!("{}.sock", key.pipeline_id))
    }

    async fn mark_status(&self, process_id: Uuid, status: &str) {
        let _ = sqlx::query!(
            "UPDATE processes SET status = $1, updated_at = now() WHERE id = $2",
            status, process_id,
        )
        .execute(&self.pool)
        .await;
    }
}
