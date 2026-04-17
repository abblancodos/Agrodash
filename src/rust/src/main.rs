// src/main.rs

mod models;
mod routes;
mod tasks;

use axum::{routing::get, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL no está definida en .env");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("No se pudo conectar a PostgreSQL");

    info!("Conectado a PostgreSQL");

    // ── Tarea de estadísticas en background ───────────────────────────────────
    // Se lanza antes del servidor para que el primer cálculo corra
    // inmediatamente al arrancar. El pool se clona (Arc interno, barato).
    tokio::spawn(tasks::stats_worker::run(pool.clone()));

    // ── CORS ──────────────────────────────────────────────────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // ── Router ────────────────────────────────────────────────────────────────
    let app = Router::new()
        .route("/api/v1/boxes",                       get(routes::boxes::get_boxes))
        .route("/api/v1/readings/time-range",         get(routes::readings::get_time_range))
        .route("/api/v1/readings/last",               get(routes::readings::get_last_reading))
        .route("/api/v1/readings",                    get(routes::readings::get_readings))
        .route("/api/v1/environment/temperature",     get(routes::readings::get_temperature))
        // Nuevo: stats pre-calculadas — no toca readings en producción
        .route("/api/v1/stats",                       get(routes::stats::get_stats))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Escuchando en http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}