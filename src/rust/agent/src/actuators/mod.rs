// agent/src/actuators/mod.rs

use agrodash_shared::{
    ActuatorConfig, ActuatorKind, ActuatorState, ActuatorAction,
    Connections, MqttActuatorParams, HttpActuatorParams,
};
use anyhow::Result;
use chrono::Utc;

#[async_trait::async_trait]
pub trait Actuator: Send {
    async fn activate(&mut self)   -> Result<()>;
    async fn deactivate(&mut self) -> Result<()>;
    fn state(&self)                -> ActuatorState;
}

pub async fn build(cfg: &ActuatorConfig, conns: &Connections) -> Result<Box<dyn Actuator>> {
    match &cfg.kind {
        ActuatorKind::Mqtt(p) => {
            let mqtt_cfg = conns.mqtt.as_ref()
                .ok_or_else(|| anyhow::anyhow!("actuador MQTT requiere connections.mqtt"))?;
            Ok(Box::new(MqttActuator::new(p.clone(), mqtt_cfg.broker_url.clone(),
                mqtt_cfg.client_id.clone(), mqtt_cfg.username.clone(),
                mqtt_cfg.password.clone(), mqtt_cfg.qos.unwrap_or(1)).await?))
        }
        ActuatorKind::Http(p) => {
            let http_cfg = conns.http.as_ref()
                .ok_or_else(|| anyhow::anyhow!("actuador HTTP requiere connections.http"))?;
            Ok(Box::new(HttpActuator::new(p.clone(), http_cfg.base_url.clone(),
                http_cfg.bearer_token.clone())))
        }
    }
}

// ── MQTT ──────────────────────────────────────────────────────────────────────

pub struct MqttActuator {
    params:      MqttActuatorParams,
    client:      rumqttc::AsyncClient,
    qos:         rumqttc::QoS,
    last_action: Option<ActuatorAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl MqttActuator {
    pub async fn new(
        params:    MqttActuatorParams,
        broker:    String,
        client_id: String,
        username:  Option<String>,
        password:  Option<String>,
        qos:       u8,
    ) -> Result<Self> {
        use rumqttc::{MqttOptions, AsyncClient, QoS};

        let url = broker.trim_start_matches("mqtt://");
        let (host, port) = url.split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(1883)))
            .unwrap_or((url.to_string(), 1883));

        let mut opts = MqttOptions::new(client_id, host, port);
        opts.set_keep_alive(std::time::Duration::from_secs(30));
        if let (Some(u), Some(p)) = (username, password) {
            opts.set_credentials(u, p);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 10);

        // Eventloop en background
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_)  => {}
                    Err(e) => {
                        tracing::warn!("MQTT eventloop error: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        let qos = match qos {
            0 => QoS::AtMostOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };

        Ok(Self { params, client, qos, last_action: None, last_at: None, total_on: 0.0, on_since: None })
    }

    async fn publish(&mut self, payload: &str) -> Result<()> {
        self.client.publish(
            &self.params.topic,
            self.qos,
            self.params.retain.unwrap_or(false),
            payload.as_bytes().to_vec(),
        ).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Actuator for MqttActuator {
    async fn activate(&mut self) -> Result<()> {
        if self.last_action == Some(ActuatorAction::On) { return Ok(()); }
        self.publish(&self.params.payload_on.clone()).await?;
        self.on_since    = Some(std::time::Instant::now());
        self.last_action = Some(ActuatorAction::On);
        self.last_at     = Some(Utc::now().to_rfc3339());
        tracing::info!("MQTT ON: {} → {}", self.params.topic, self.params.payload_on);
        Ok(())
    }

    async fn deactivate(&mut self) -> Result<()> {
        if self.last_action == Some(ActuatorAction::Off) { return Ok(()); }
        if let Some(t) = self.on_since.take() {
            self.total_on += t.elapsed().as_secs_f64();
        }
        self.publish(&self.params.payload_off.clone()).await?;
        self.last_action = Some(ActuatorAction::Off);
        self.last_at     = Some(Utc::now().to_rfc3339());
        tracing::info!("MQTT OFF: {} → {}", self.params.topic, self.params.payload_off);
        Ok(())
    }

    fn state(&self) -> ActuatorState {
        ActuatorState {
            last_action:    self.last_action.clone(),
            last_action_at: self.last_at.clone(),
            total_on_secs:  self.total_on,
        }
    }
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

pub struct HttpActuator {
    params:      HttpActuatorParams,
    base_url:    String,
    token:       Option<String>,
    client:      reqwest::Client,
    last_action: Option<ActuatorAction>,
    last_at:     Option<String>,
    total_on:    f64,
    on_since:    Option<std::time::Instant>,
}

impl HttpActuator {
    pub fn new(params: HttpActuatorParams, base_url: String, token: Option<String>) -> Self {
        Self {
            params, base_url, token,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .unwrap(),
            last_action: None, last_at: None, total_on: 0.0, on_since: None,
        }
    }

    async fn call(&self, path: &str, body: Option<&serde_json::Value>) -> Result<()> {
        let url    = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let method = self.params.method.as_deref().unwrap_or("POST");
        let mut req = match method {
            "GET"  => self.client.get(&url),
            "PUT"  => self.client.put(&url),
            _      => self.client.post(&url),
        };
        if let Some(t) = &self.token { req = req.bearer_auth(t); }
        if let Some(b) = body        { req = req.json(b); }
        let res = req.send().await?;
        if !res.status().is_success() {
            anyhow::bail!("HTTP actuator error: {}", res.status());
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Actuator for HttpActuator {
    async fn activate(&mut self) -> Result<()> {
        if self.last_action == Some(ActuatorAction::On) { return Ok(()); }
        self.call(&self.params.path_on.clone(), self.params.body_on.as_ref()).await?;
        self.on_since    = Some(std::time::Instant::now());
        self.last_action = Some(ActuatorAction::On);
        self.last_at     = Some(Utc::now().to_rfc3339());
        Ok(())
    }

    async fn deactivate(&mut self) -> Result<()> {
        if self.last_action == Some(ActuatorAction::Off) { return Ok(()); }
        if let Some(t) = self.on_since.take() {
            self.total_on += t.elapsed().as_secs_f64();
        }
        self.call(&self.params.path_off.clone(), self.params.body_off.as_ref()).await?;
        self.last_action = Some(ActuatorAction::Off);
        self.last_at     = Some(Utc::now().to_rfc3339());
        Ok(())
    }

    fn state(&self) -> ActuatorState {
        ActuatorState {
            last_action:    self.last_action.clone(),
            last_action_at: self.last_at.clone(),
            total_on_secs:  self.total_on,
        }
    }
}
