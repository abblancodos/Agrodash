// src/routes/auth.rs

use axum::{extract::State, http::StatusCode, Json};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::auth::{encode_token, Claims};
use crate::crypto::{decrypt_password, public_key_pem};

const TEMP_PASSWORD: &str = "Estacion2";

// ── Shapes ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AuthResponse {
    pub token:           String,
    pub user:            UserInfo,
    pub must_change_pw:  bool,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id:           uuid::Uuid,
    pub email:        String,
    pub display_name: String,
    pub role:         String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email:              String,
    pub password:           Option<String>,           // plaintext (solo desarrollo)
    pub password_encrypted: Option<String>,           // base64 RSA-OAEP (producción)
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password:           Option<String>,
    pub current_password_encrypted: Option<String>,
    pub new_password:               Option<String>,
    pub new_password_encrypted:     Option<String>,
}

// ── GET /api/v1/auth/public-key ───────────────────────────────────────────────
// Devuelve la llave pública RSA en PEM para que el cliente cifre la contraseña.
// Se llama una vez al cargar la página de login.

pub async fn public_key() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "public_key": public_key_pem() }))
}

// ── Admin: crear usuario ───────────────────────────────────────────────────────
// POST /api/v1/admin/users
// Solo accesible con rol admin. Asigna contraseña temporal "Estacion2".

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub email:        String,
    pub display_name: String,
    pub role:         Option<String>,  // default: "user"
}

pub async fn admin_create_user(
    State(pool): State<PgPool>,
    claims: Claims,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !claims.is_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Solo admins pueden crear usuarios" })),
        ));
    }

    if body.email.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "email requerido" }))));
    }

    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)",
        body.email
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?
    .unwrap_or(false);

    if exists {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "Email ya registrado" }))));
    }

    let role = body.role.unwrap_or_else(|| "user".to_string());
    if !["user", "admin"].contains(&role.as_str()) {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "role inválido" }))));
    }

    // Contraseña temporal — must_change_pw se infiere comparando con el hash de TEMP_PASSWORD
    let password_hash = hash(TEMP_PASSWORD, DEFAULT_COST)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let user = sqlx::query!(
        r#"
        INSERT INTO users (email, display_name, password_hash, role)
        VALUES ($1, $2, $3, $4)
        RETURNING id AS "id: uuid::Uuid", email, display_name, role
        "#,
        body.email,
        body.display_name,
        password_hash,
        role,
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    Ok(Json(serde_json::json!({
        "id":           user.id,
        "email":        user.email,
        "display_name": user.display_name,
        "role":         user.role,
        "temp_password": TEMP_PASSWORD,
        "note": "Por favor pedile al usuario que cambie la contraseña en su primer login"
    })))
}

// ── Admin: listar usuarios ─────────────────────────────────────────────────────
// GET /api/v1/admin/users

pub async fn admin_list_users(
    State(pool): State<PgPool>,
    claims: Claims,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !claims.is_admin() {
        return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Solo admins" }))));
    }

    let users = sqlx::query!(
        r#"
        SELECT id AS "id: uuid::Uuid", email, display_name, role, created_at
        FROM users ORDER BY created_at
        "#
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let list: Vec<_> = users.iter().map(|u| serde_json::json!({
        "id": u.id, "email": u.email,
        "display_name": u.display_name, "role": u.role,
        "created_at": u.created_at,
    })).collect();

    Ok(Json(serde_json::json!(list)))
}

// ── POST /api/v1/auth/login ───────────────────────────────────────────────────

pub async fn login(
    State(pool): State<PgPool>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Resolver contraseña — acepta cifrada (producción) o plaintext (dev)
    let password = if let Some(enc) = &body.password_encrypted {
        decrypt_password(enc).map_err(|e| (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("Error de descifrado: {e}") })),
        ))?
    } else if let Some(plain) = &body.password {
        plain.clone()
    } else {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Se requiere password o password_encrypted" }))));
    };

    let user = sqlx::query!(
        r#"
        SELECT id AS "id: uuid::Uuid", email, display_name, password_hash, role
        FROM users WHERE email = $1
        "#,
        body.email,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?
    .ok_or_else(|| (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "Credenciales inválidas" })),
    ))?;

    let valid = verify(&password, &user.password_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    if !valid {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Credenciales inválidas" }))));
    }

    // Detectar si aún usa la contraseña temporal
    let must_change_pw = verify(TEMP_PASSWORD, &user.password_hash).unwrap_or(false);

    let claims = Claims::new(user.id, user.email.clone(), user.role.clone());
    let token  = encode_token(&claims)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    Ok(Json(AuthResponse {
        token,
        must_change_pw,
        user: UserInfo {
            id: user.id, email: user.email,
            display_name: user.display_name, role: user.role,
        },
    }))
}

// ── GET /api/v1/auth/me ───────────────────────────────────────────────────────

pub async fn me(
    State(pool): State<PgPool>,
    claims: Claims,
) -> Result<Json<UserInfo>, (StatusCode, Json<serde_json::Value>)> {
    let user = sqlx::query!(
        r#"SELECT id AS "id: uuid::Uuid", email, display_name, role FROM users WHERE id = $1"#,
        claims.sub,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Usuario no encontrado" }))))?;

    Ok(Json(UserInfo {
        id: user.id, email: user.email,
        display_name: user.display_name, role: user.role,
    }))
}

// ── POST /api/v1/auth/change-password ────────────────────────────────────────

pub async fn change_password(
    State(pool): State<PgPool>,
    claims: Claims,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Resolver contraseñas
    let current_password = if let Some(enc) = &body.current_password_encrypted {
        decrypt_password(enc).map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e }))))?
    } else if let Some(plain) = &body.current_password {
        plain.clone()
    } else {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Se requiere current_password o current_password_encrypted" }))));
    };

    let new_password = if let Some(enc) = &body.new_password_encrypted {
        decrypt_password(enc).map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e }))))?
    } else if let Some(plain) = &body.new_password {
        plain.clone()
    } else {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Se requiere new_password o new_password_encrypted" }))));
    };

    if new_password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Mínimo 8 caracteres" }))));
    }

    let user = sqlx::query!(
        r#"SELECT password_hash FROM users WHERE id = $1"#,
        claims.sub,
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let valid = verify(&current_password, &user.password_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    if !valid {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Contraseña actual incorrecta" }))));
    }

    if new_password == TEMP_PASSWORD {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "No podés usar la contraseña temporal como nueva contraseña" }))));
    }

    let new_hash = hash(&new_password, DEFAULT_COST)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    sqlx::query!(
        "UPDATE users SET password_hash = $1 WHERE id = $2",
        new_hash, claims.sub,
    )
    .execute(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    Ok(StatusCode::NO_CONTENT)
}
