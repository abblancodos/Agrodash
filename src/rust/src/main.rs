// src/main.rs

mod auth;
mod models;
mod routes;
mod script_engine;
mod tasks;

use axum::{routing::{delete, get, patch, post, put}, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};
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

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL no está definida");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("No se pudo conectar a PostgreSQL");

    info!("Conectado a PostgreSQL");

    tokio::spawn(tasks::stats_worker::run(pool.clone()));

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    // Seed route — solo se registra si SEED_SECRET está definido en .env.
    // Una vez creado el primer admin: borrar SEED_SECRET del .env y reiniciar.
    // La ruta desaparece completamente y devuelve 404.
    let seed_enabled = std::env::var("SEED_SECRET").is_ok();
    if seed_enabled {
        info!("SEED_SECRET definido — ruta /api/v1/admin/seed activa");
    }

    let base = Router::new()
        // ── AgroDash sensor dashboard ───────────────────────────────────────
        .route("/api/v1/boxes",                   get(routes::boxes::get_boxes))
        .route("/api/v1/readings/time-range",     get(routes::readings::get_time_range))
        .route("/api/v1/readings/last",           get(routes::readings::get_last_reading))
        .route("/api/v1/readings",                get(routes::readings::get_readings))
        .route("/api/v1/environment/temperature", get(routes::readings::get_temperature))
        .route("/api/v1/stats",                   get(routes::stats::get_stats))

        // ── Auth (sin registro público) ─────────────────────────────────────
        .route("/api/v1/auth/login",           post(routes::auth::login))
        .route("/api/v1/auth/me",              get(routes::auth::me))
        .route("/api/v1/auth/change-password", post(routes::auth::change_password));

    // Seed condicional
    let base = if seed_enabled {
        base.route("/api/v1/admin/seed", post(routes::seed::seed_admin))
    } else {
        base
    };

    let app = base

        // ── Admin — gestión de usuarios (requiere rol admin) ────────────────
        .route("/api/v1/admin/users",
            get(routes::auth::admin_list_users)
            .post(routes::auth::admin_create_user))

        // ── Plantillas de experimento ───────────────────────────────────────
        .route("/api/v1/experiment-templates",
            get(routes::experiments::list_templates)
            .post(routes::experiments::create_template))
        .route("/api/v1/experiment-templates/:id",
            get(routes::experiments::get_template))

        // ── Experimentos ────────────────────────────────────────────────────
        .route("/api/v1/experiments",
            get(routes::experiments::list_experiments)
            .post(routes::experiments::create_experiment))
        .route("/api/v1/experiments/:id",
            get(routes::experiments::get_experiment))
        .route("/api/v1/experiments/:id/constants",
            put(routes::experiments::update_constants))

        // ── Eventos (registro cronológico) ──────────────────────────────────
        .route("/api/v1/experiments/:id/events",
            get(routes::experiments::list_events)
            .post(routes::experiments::create_event))
        .route("/api/v1/experiments/:id/events/:eid",
            delete(routes::experiments::delete_event))

        // ── Series temporales (pesadas periódicas) ──────────────────────────
        .route("/api/v1/experiments/:id/series",
            get(routes::experiments::list_series)
            .post(routes::experiments::create_series_point))

        // ── CSV upload ──────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/upload-csv",
            post(routes::experiments::upload_csv))

        // ── Script execution ────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/steps/:step_key/run",
            get(routes::experiments::run_step_script))
        .route("/api/v1/scripts/validate",
            post(routes::experiments::validate_script))

        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Escuchando en http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}