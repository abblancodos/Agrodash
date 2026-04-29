// src/routes/invites.rs

use axum::{extract::State, http::StatusCode, Json};
use bcrypt::{hash, DEFAULT_COST};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::auth::{encode_token, Claims};
use crate::crypto::decrypt_password;
use crate::routes::auth::{AuthResponse, UserInfo};

const TEMP_PASSWORD: &str = "Estacion2";

// ── Helpers ───────────────────────────────────────────────────────────────────

fn gen_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..10)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn err(msg: impl ToString) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": msg.to_string() })))
}
fn bad(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg })))
}

// ── POST /api/v1/admin/invites ────────────────────────────────────────────────
// Solo admins. Genera hasta 3 códigos activos por llamado.
// expires_in_days: 1 | 2 | 7 (default 7)

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    pub email_hint:      Option<String>,
    pub expires_in_days: Option<i64>,   // 1, 2, o 7
    pub count:           Option<i32>,   // cuántos generar (1–3, default 1)
}

#[derive(Serialize)]
pub struct InviteResponse {
    pub code:       String,
    pub email_hint: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_invite(
    State(pool): State<PgPool>,
    claims: Claims,
    Json(body): Json<CreateInviteRequest>,
) -> Result<Json<Vec<InviteResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if !claims.is_admin() {
        return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Solo admins pueden crear invitaciones" }))));
    }

    let days = match body.expires_in_days.unwrap_or(7) {
        1 => 1i64,
        2 => 2i64,
        7 => 7i64,
        _ => return Err(bad("expires_in_days debe ser 1, 2, o 7")),
    };

    let count = body.count.unwrap_or(1).clamp(1, 3);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(days);
    let mut results = Vec::new();

    for _ in 0..count {
        let code = gen_code();
        sqlx::query!(
            r#"
            INSERT INTO invites (code, created_by, email_hint, expires_at)
            VALUES ($1, $2, $3, $4)
            "#,
            code,
            claims.sub as uuid::Uuid,
            body.email_hint,
            expires_at,
        )
        .execute(&pool)
        .await
        .map_err(err)?;

        results.push(InviteResponse {
            code,
            email_hint: body.email_hint.clone(),
            expires_at,
        });
    }

    Ok(Json(results))
}

// ── GET /api/v1/admin/invites ─────────────────────────────────────────────────
// Lista invitaciones activas (no usadas, no expiradas). Solo admins.

pub async fn list_invites(
    State(pool): State<PgPool>,
    claims: Claims,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !claims.is_admin() {
        return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Solo admins" }))));
    }

    let rows = sqlx::query!(
        r#"
        SELECT code, email_hint, expires_at, used_at,
               created_by AS "created_by: uuid::Uuid"
        FROM invites
        WHERE expires_at > NOW()
        ORDER BY expires_at ASC
        "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    let list: Vec<_> = rows.iter().map(|r| serde_json::json!({
        "code":       r.code,
        "email_hint": r.email_hint,
        "expires_at": r.expires_at,
        "used":       r.used_at.is_some(),
    })).collect();

    Ok(Json(serde_json::json!(list)))
}

// ── POST /api/v1/auth/register ────────────────────────────────────────────────
// Público pero requiere código de invitación válido y de un solo uso.

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub invite_code:        String,
    pub email:              String,
    pub display_name:       String,
    pub password_encrypted: Option<String>,
    pub password:           Option<String>,   // solo para dev/testing
}

pub async fn register(
    State(pool): State<PgPool>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Validar código de invitación — debe existir, no expirado, no usado
    let invite = sqlx::query!(
        r#"
        SELECT code, used_at, expires_at
        FROM invites
        WHERE code = $1
        "#,
        body.invite_code,
    )
    .fetch_optional(&pool)
    .await
    .map_err(err)?
    .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Código de invitación inválido" }))))?;

    if invite.used_at.is_some() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "El código ya fue usado" }))));
    }
    if invite.expires_at < chrono::Utc::now() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "El código expiró" }))));
    }

    // 2. Validar email
    if body.email.is_empty() {
        return Err(bad("email requerido"));
    }

    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)",
        body.email
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?
    .unwrap_or(false);

    if exists {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "El email ya está registrado" }))));
    }

    // 3. Resolver contraseña
    let password = if let Some(enc) = &body.password_encrypted {
        decrypt_password(enc).map_err(|e| bad(&e))?
    } else if let Some(plain) = &body.password {
        plain.clone()
    } else {
        return Err(bad("Se requiere password_encrypted o password"));
    };

    if password.len() < 8 {
        return Err(bad("Mínimo 8 caracteres"));
    }

    let password_hash = hash(&password, DEFAULT_COST)
        .map_err(|e| err(e.to_string()))?;

    // 4. Crear usuario
    let user = sqlx::query!(
        r#"
        INSERT INTO users (email, display_name, password_hash, must_change_pw)
        VALUES ($1, $2, $3, false)
        RETURNING id AS "id: uuid::Uuid", email, display_name, role
        "#,
        body.email,
        body.display_name,
        password_hash,
    )
    .fetch_one(&pool)
    .await
    .map_err(err)?;

    // 5. Marcar código como usado (un solo uso)
    sqlx::query!(
        "UPDATE invites SET used_at = NOW(), used_by = $1 WHERE code = $2",
        user.id as uuid::Uuid,
        body.invite_code,
    )
    .execute(&pool)
    .await
    .map_err(err)?;

    // 6. Devolver token directamente — no necesita cambiar contraseña
    let token_claims = Claims::new(user.id, user.email.clone(), user.role.clone());
    let token = encode_token(&token_claims).map_err(|e| err(e.to_string()))?;

    Ok(Json(AuthResponse {
        token,
        must_change_pw: false,
        user: UserInfo {
            id:           user.id,
            email:        user.email,
            display_name: user.display_name,
            role:         user.role,
        },
    }))
}
