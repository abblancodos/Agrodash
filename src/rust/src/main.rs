// src/main.rs

mod models;
mod routes;

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
    // Cargar .env (si existe — en producción se usan variables del sistema)
    dotenvy::dotenv().ok();

    // Logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // DB pool
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL no está definida en .env");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("No se pudo conectar a PostgreSQL");

    info!("Conectado a PostgreSQL");

    // CORS — permite requests desde SvelteKit dev (localhost:5173) y producción
    let cors = CorsLayer::new()
        .allow_origin(Any)  // en producción: reemplazar con tu dominio
        .allow_methods(Any)
        .allow_headers(Any);

    // Router
    let app = Router::new()
        // Boxes
        .route("/api/v1/boxes", get(routes::boxes::get_boxes))
        // Readings — el orden importa: /time-range debe ir antes de /
        .route("/api/v1/readings/time-range",      get(routes::readings::get_time_range))
        .route("/api/v1/readings/last",             get(routes::readings::get_last_reading))
        .route("/api/v1/readings",                 get(routes::readings::get_readings))
        .route("/api/v1/environment/temperature",  get(routes::readings::get_temperature))
        // Middlewares
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // State compartido: el pool se clona (barato) por cada handler
        .with_state(pool);

    // Puerto
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Escuchando en http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}