// src/main.rs

mod auth;
mod crypto;
mod models;
mod routes;
mod script_engine;
mod tasks;

use axum::{routing::{delete, get, post, put}, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use axum::http::{
    header::{AUTHORIZATION, CONTENT_TYPE, ACCEPT},
    Method,
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

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL no está definida");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("No se pudo conectar a PostgreSQL");

    info!("Conectado a PostgreSQL");

    tokio::spawn(tasks::stats_worker::run(pool.clone()));

    // CORS — credentials:include requiere origen explícito, no wildcard
    let app_origin = std::env::var("APP_URL")
        .unwrap_or_else(|_| "https://agrodash.nm.35-208-114-233.nip.io".into());
    let origin = axum::http::HeaderValue::from_str(&app_origin)
        .expect("APP_URL inválida como origin");
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([Method::GET, Method::POST, Method::PUT,
                        Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE, ACCEPT])
        .allow_credentials(true);

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
        // ── OAuth Gitea ─────────────────────────────────────────────────────
        .route("/api/v1/auth/gitea/login",    get(routes::oauth::gitea_login))
        .route("/api/v1/auth/gitea/callback", get(routes::oauth::gitea_callback))
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
        .route("/api/v1/admin/invites",
            get(routes::invites::list_invites)
            .post(routes::invites::create_invite))
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
            get(routes::experiments::get_experiment)
            .delete(routes::experiment_features::delete_experiment))
        .route("/api/v1/experiments/:id/constants",
            put(routes::experiments::update_constants))
        .route("/api/v1/experiments/:id/columns",
            axum::routing::patch(routes::experiment_features::update_columns))
        .route("/api/v1/experiments/:id/values",
            get(routes::experiment_features::get_all_entry_values))
        .route("/api/v1/experiments/:id/events/:eid/values",
            get(routes::experiment_features::get_entry_values)
            .post(routes::experiment_features::save_entry_values))

        // ── Eventos (registro cronológico) ──────────────────────────────────
        .route("/api/v1/experiments/:id/events",
            get(routes::experiments::list_events)
            .post(routes::experiments::create_event))
        .route("/api/v1/experiments/:id/events/:eid",
            delete(routes::experiments::delete_event))
        .route("/api/v1/experiments/:id/events/:eid/void",
            axum::routing::post(routes::experiment_features::void_event))

        // ── Series temporales (pesadas periódicas) ──────────────────────────
        .route("/api/v1/experiments/:id/series",
            get(routes::experiments::list_series)
            .post(routes::experiments::create_series_point))

        // ── CSV upload ──────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/upload-csv",
            post(routes::experiments::upload_csv))


        // ── Server time ─────────────────────────────────────────────────────────
        .route("/api/v1/time", get(routes::experiment_features::server_time))

        // ── User search ─────────────────────────────────────────────────────────
        .route("/api/v1/users/search", get(routes::experiment_features::search_users))

        // ── Experiment collaborators ─────────────────────────────────────────────
        .route("/api/v1/experiments/:id/collaborators",
            get(routes::experiment_features::list_collaborators)
            .post(routes::experiment_features::add_collaborator))
        .route("/api/v1/experiments/:id/collaborators/:uid",
            delete(routes::experiment_features::remove_collaborator))

        // ── Event corrections ────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/events/:eid/correct",
            post(routes::experiment_features::correct_event))

        // ── Definitions ──────────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/definitions",
            get(routes::experiment_features::list_definitions)
            .post(routes::experiment_features::create_definition))
        .route("/api/v1/experiments/:id/definitions/:did",
            put(routes::experiment_features::update_definition)
            .delete(routes::experiment_features::delete_definition))

        // ── Objectives ───────────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/objectives",
            get(routes::experiment_features::list_objectives)
            .post(routes::experiment_features::create_objective))
        .route("/api/v1/experiments/:id/objectives/:oid",
            delete(routes::experiment_features::delete_objective))

        // ── Export CSV ───────────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/export-csv",
            get(routes::experiment_features::export_csv))

        // ── Status ───────────────────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/status",
            axum::routing::patch(routes::experiment_features::update_status))
        .route("/api/v1/experiments/:id/clone",
            post(routes::experiments::clone_experiment))

        // ── Definition groups ─────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/groups",
            get(routes::experiment_features::list_groups)
            .post(routes::experiment_features::create_group))
        .route("/api/v1/experiments/:id/groups/:gid",
            axum::routing::patch(routes::experiment_features::update_group)
            .delete(routes::experiment_features::delete_group))
        .route("/api/v1/experiments/:id/definitions/:did/group",
            axum::routing::patch(routes::experiment_features::set_definition_group))
        .route("/api/v1/experiments/:id/events/:eid/group",
            axum::routing::patch(routes::experiment_features::set_event_group))

        // ── Script execution ────────────────────────────────────────────────
        .route("/api/v1/experiments/:id/steps/:step_key/run",
            get(routes::experiments::run_step_script))
        .route("/api/v1/scripts/validate",
            post(routes::experiments::validate_script))

        //── Processes ──────────────────────────────────────────────── 
        
        .route("/api/v1/processes",
            get(routes::processes::list_processes)
            .post(routes::processes::create_process))
        .route("/api/v1/processes/:id",
            get(routes::processes::get_process))
        .route("/api/v1/processes/:id/state",
            get(routes::processes::get_state))
        .route("/api/v1/processes/:id/stream",
            get(routes::processes::stream_state))
        .route("/api/v1/processes/:id/command",
            post(routes::processes::send_command))
        .route("/api/v1/processes/:id/logs",
            get(routes::processes::get_logs))
        .route("/api/v1/processes/:id/logs/tail",
            get(routes::processes::tail_control_log))
        .route("/api/v1/processes/:id/valve-events",
            get(routes::processes::get_valve_events))
        .route("/api/v1/processes/:id/collaborators",
            get(routes::processes::list_collaborators)
            .post(routes::processes::add_collaborator))
        .route("/api/v1/processes/:id/collaborators/:uid",
            delete(routes::processes::remove_collaborator))


        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Escuchando en http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}