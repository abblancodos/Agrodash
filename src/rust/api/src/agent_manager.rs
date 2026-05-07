// api/src/agent_manager.rs
//
// Responsabilidades:
//   - spawn(): arrancar un agente como child process
//   - notify(): enviar comando al agente vía PG NOTIFY
//   - monitor(): detectar muerte del agente y reiniciar si corresponde
//   - start_all(): al arrancar la API, spawnar procesos en status='running'
//   - shutdown_all(): al cerrar la API, notificar checkpoint a todos
//
// El agente se comunica con la API escribiendo en la DB, no con sockets.
// La dirección del agente es el pipeline_id — asignado al crear, nunca cambia.
#![allow(clippy::panic)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agrodash_shared::{AgentCommand, ProcessConfig};
use sqlx::PgPool;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AgentManagerConfig {
    pub agent_bin: PathBuf,
    pub database_url: String,
    pub max_restarts: u32,
    pub restart_delay: Duration,
}

impl Default for AgentManagerConfig {
    fn default() -> Self {
        Self {
            agent_bin: PathBuf::from(
                std::env::var("AGENT_BIN").unwrap_or_else(|_| "/app/agrodash-agent".into()),
            ),
            database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
            max_restarts: 5,
            restart_delay: Duration::from_secs(5),
        }
    }
}

// ── AgentKey ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentKey {
    pub process_id: Uuid,
    pub pipeline_id: String,
}

pub(crate) struct AgentEntry {
    pub(crate) child: Child,
    pub(crate) restarts: u32,
}

// ── Manager ───────────────────────────────────────────────────────────────────

pub struct AgentManager {
    cfg: AgentManagerConfig,
    pool: PgPool,
    pub(crate) agents: Arc<RwLock<HashMap<AgentKey, Mutex<AgentEntry>>>>,
}

impl AgentManager {
    pub fn new(cfg: AgentManagerConfig, pool: PgPool) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            pool,
            agents: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Al arrancar la API: spawnar procesos con status='running'.
    pub async fn start_all(self: &Arc<Self>) {
        let rows = sqlx::query!("SELECT id, config FROM processes WHERE status = 'running'")
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

    /// Spawnar un agente para un pipeline.
    pub async fn spawn(self: &Arc<Self>, process_id: Uuid, pipeline_id: String) {
        let key = AgentKey {
            process_id,
            pipeline_id: pipeline_id.clone(),
        };

        if self.agents.read().await.contains_key(&key) {
            warn!("Agente {}/{} ya existe", process_id, pipeline_id);
            return;
        }

        match self.spawn_child(&key).await {
            Ok(child) => {
                self.agents
                    .write()
                    .await
                    .insert(key.clone(), Mutex::new(AgentEntry { child, restarts: 0 }));
                info!("Agente {}/{} spawnado", process_id, pipeline_id);

                let mgr = Arc::clone(self);
                tokio::spawn(async move { mgr.monitor(key).await });
            }
            Err(e) => {
                error!(
                    "No se pudo spawnear agente {}/{}: {e}",
                    process_id, pipeline_id
                );
                self.mark_status(process_id, "error").await;
            }
        }
    }

    /// Enviar un comando al agente vía PG NOTIFY.
    /// Fire-and-forget — el agente lo procesa en su propio tiempo.
    /// Si el agente no está escuchando, el desired state en la DB garantiza
    /// que se ejecute al reiniciar.
    pub async fn notify(&self, key: &AgentKey, cmd: AgentCommand) -> Result<(), sqlx::Error> {
        let channel = format!("agent_cmd_{}", key.pipeline_id);
        let payload = serde_json::to_string(&cmd).unwrap_or_default();

        // PG NOTIFY via query — sqlx no tiene API directa para NOTIFY
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(&channel)
            .bind(&payload)
            .execute(&self.pool)
            .await?;

        info!(
            "NOTIFY '{}' → {:?}",
            channel,
            serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|v| v.get("cmd").cloned())
                .unwrap_or_default()
        );
        Ok(())
    }

    /// Detener todos los agentes de un proceso:
    ///   1. Escribir status='stopping' en DB (desired state)
    ///   2. NOTIFY Stop a cada pipeline
    ///   3. Remover del mapa — el monitor no reiniciará
    pub async fn stop_process(self: &Arc<Self>, process_id: Uuid) {
        let keys: Vec<AgentKey> = {
            self.agents
                .read()
                .await
                .keys()
                .filter(|k| k.process_id == process_id)
                .cloned()
                .collect()
        };

        for key in &keys {
            // Remover antes de NOTIFY — el monitor verá que ya no está
            self.agents.write().await.remove(key);
            self.notify(key, AgentCommand::Stop).await.ok();
        }

        // Si no había agentes en el mapa (arrancaron antes del manager),
        // el desired state en DB (status='stopping') garantiza que al
        // próximo ciclo el agente se detenga solo.
    }

    /// Checkpoint a todos y cerrar al apagar la API.
    pub async fn shutdown_all(self: &Arc<Self>) {
        info!("AgentManager: shutdown");
        let keys: Vec<AgentKey> = self.agents.read().await.keys().cloned().collect();

        for key in &keys {
            self.notify(key, AgentCommand::Checkpoint).await.ok();
        }

        sleep(Duration::from_secs(3)).await;

        // Matar child processes
        let entries: Vec<_> = self.agents.write().await.drain().collect();
        for (_, entry) in entries {
            entry.lock().await.child.kill().await.ok();
        }
    }

    /// Retorna los pipeline_ids de los agentes activos de un proceso.
    pub async fn active_pipelines(&self, process_id: Uuid) -> Vec<String> {
        self.agents
            .read()
            .await
            .keys()
            .filter(|k| k.process_id == process_id)
            .map(|k| k.pipeline_id.clone())
            .collect()
    }

    // ── Internos ──────────────────────────────────────────────────────────────

    async fn spawn_child(&self, key: &AgentKey) -> anyhow::Result<Child> {
        let child = Command::new(&self.cfg.agent_bin)
            .arg("--process-id")
            .arg(key.process_id.to_string())
            .arg("--pipeline-id")
            .arg(&key.pipeline_id)
            .env("DATABASE_URL", &self.cfg.database_url)
            .env(
                "RUST_LOG",
                std::env::var("AGENT_LOG").unwrap_or_else(|_| "info".into()),
            )
            .kill_on_drop(false)
            .spawn()?;
        Ok(child)
    }

    /// Monitor: espera que el child muera y decide si reiniciar.
    /// No reinicia si la key fue removida por stop_process().
    async fn monitor(self: Arc<Self>, key: AgentKey) {
        let mut delay = self.cfg.restart_delay;

        loop {
            // Esperar muerte del child
            let exit_code = {
                let agents = self.agents.read().await;
                match agents.get(&key) {
                    None => break, // removido por stop_process — salir limpiamente
                    Some(entry) => entry.lock().await.child.wait().await.ok(),
                }
            };

            let code = exit_code.and_then(|s| s.code()).unwrap_or(-1);
            warn!(
                "Agente {}/{} terminó (código {code})",
                key.process_id, key.pipeline_id
            );

            // Re-verificar si sigue en el mapa
            if !self.agents.read().await.contains_key(&key) {
                info!(
                    "Agente {}/{} detenido intencionalmente",
                    key.process_id, key.pipeline_id
                );
                break;
            }

            // Verificar desired state en DB — si el proceso ya no debería correr, no reiniciar
            let status =
                sqlx::query_scalar!("SELECT status FROM processes WHERE id = $1", key.process_id,)
                    .fetch_optional(&self.pool)
                    .await
                    .ok()
                    .flatten();

            match status.as_deref() {
                Some("stopping") | Some("stopped") | Some("error") | None => {
                    info!("Proceso en estado {:?} — no reiniciar agente", status);
                    self.agents.write().await.remove(&key);
                    break;
                }
                _ => {}
            }

            // Verificar límite de reinicios
            let restarts = {
                let agents = self.agents.read().await;
                agents
                    .get(&key)
                    .map(|e| futures::executor::block_on(async { e.lock().await.restarts }))
                    .unwrap_or(0)
            };

            if restarts >= self.cfg.max_restarts {
                error!(
                    "Agente {}/{} alcanzó máximo de reinicios",
                    key.process_id, key.pipeline_id
                );
                self.mark_status(key.process_id, "error").await;
                self.agents.write().await.remove(&key);
                break;
            }

            warn!(
                "Reiniciando {}/{} en {}s (intento {})",
                key.process_id,
                key.pipeline_id,
                delay.as_secs(),
                restarts + 1
            );
            sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(60));

            // Re-verificar una vez más tras el backoff
            if !self.agents.read().await.contains_key(&key) {
                break;
            }

            match self.spawn_child(&key).await {
                Ok(child) => {
                    let agents = self.agents.read().await;
                    if let Some(entry) = agents.get(&key) {
                        let mut e = entry.lock().await;
                        e.child = child;
                        e.restarts += 1;
                    }
                    info!("Agente {}/{} reiniciado", key.process_id, key.pipeline_id);
                }
                Err(e) => {
                    error!("No se pudo reiniciar: {e}");
                    self.mark_status(key.process_id, "error").await;
                    self.agents.write().await.remove(&key);
                    break;
                }
            }
        }
    }

    async fn mark_status(&self, process_id: Uuid, status: &str) {
        sqlx::query!(
            "UPDATE processes SET status=$1, updated_at=now() WHERE id=$2",
            status,
            process_id,
        )
        .execute(&self.pool)
        .await
        .ok();
    }
}

#[cfg(test)]
#[path = "agent_manager_tests.rs"]
mod agent_manager_tests;
