// src/routes/seed.rs
//
// POST /api/v1/admin/seed
//
// Crea el primer usuario admin. Solo funciona si no existe ningún admin.
// Requiere SEED_SECRET en .env para evitar que cualquiera lo llame.
// Deshabilitar en producción una vez creado el primer admin.

use axum::{extract::State, http::StatusCode, Json};
use bcrypt::{hash, DEFAULT_COST};
use serde::Deserialize;
use sqlx::PgPool;

const TEMP_PASSWORD: &str = "Estacion2";

#[derive(Deserialize)]
pub struct SeedRequest {
    pub secret:       String,
    pub email:        String,
    pub display_name: String,
}

pub async fn seed_admin(
    State(pool): State<PgPool>,
    Json(body): Json<SeedRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let expected = std::env::var("SEED_SECRET").unwrap_or_default();
    if expected.is_empty() || body.secret != expected {
        return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Secret inválido" }))));
    }

    // Solo si no hay ningún admin todavía
    let admin_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM users WHERE role = 'admin'"
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?
    .unwrap_or(0);

    if admin_count > 0 {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({
            "error": "Ya existe al menos un admin. Usá POST /api/v1/admin/users con un token de admin."
        }))));
    }

    let password_hash = hash(TEMP_PASSWORD, DEFAULT_COST)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let user = sqlx::query!(
        r#"
        INSERT INTO users (email, display_name, password_hash, role, must_change_pw)
        VALUES ($1, $2, $3, 'admin', true)
        RETURNING id AS "id: uuid::Uuid", email, display_name
        "#,
        body.email,
        body.display_name,
        password_hash,
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    Ok(Json(serde_json::json!({
        "id":           user.id,
        "email":        user.email,
        "display_name": user.display_name,
        "role":         "admin",
        "temp_password": TEMP_PASSWORD,
        "note": "Cambiá la contraseña con POST /api/v1/auth/change-password"
    })))
}
