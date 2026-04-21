// src/routes/oauth.rs
//
// OAuth2 con Gitea como proveedor.
//
// Flujo:
//   1. GET  /api/v1/auth/gitea/login     → redirige al Gitea del lab
//   2. GET  /api/v1/auth/gitea/callback  → Gitea redirige acá con ?code=...
//   3. Backend intercambia code por token de Gitea
//   4. Backend obtiene perfil del usuario de Gitea
//   5. Crea o actualiza el usuario en la DB local
//   6. Redirige al frontend con el JWT de AgroDash en el hash
//
// Variables de entorno requeridas:
//   GITEA_URL           — URL base del Gitea, ej: https://git.nm.35-208-114-233.nip.io
//   GITEA_CLIENT_ID     — Client ID del OAuth2 app en Gitea
//   GITEA_CLIENT_SECRET — Client Secret del OAuth2 app en Gitea
//   APP_URL             — URL base del frontend, ej: https://agrodash.nm.35-208-114-233.nip.io

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::auth::{encode_token, Claims};

fn gitea_url() -> String {
    std::env::var("GITEA_URL").expect("GITEA_URL no definida")
}
fn client_id() -> String {
    std::env::var("GITEA_CLIENT_ID").expect("GITEA_CLIENT_ID no definida")
}
fn client_secret() -> String {
    std::env::var("GITEA_CLIENT_SECRET").expect("GITEA_CLIENT_SECRET no definida")
}
fn app_url() -> String {
    std::env::var("APP_URL")
        .unwrap_or_else(|_| "https://agrodash.nm.35-208-114-233.nip.io".into())
}
fn redirect_uri() -> String {
    let api = std::env::var("API_URL")
        .unwrap_or_else(|_| "https://api-agrodash.nm.35-208-114-233.nip.io".into());
    format!("{}/api/v1/auth/gitea/callback", api)
}

fn url_encode(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

// ── GET /api/v1/auth/gitea/login ──────────────────────────────────────────────

pub async fn gitea_login() -> Redirect {
    let url = format!(
        "{}/login/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read:user",
        gitea_url(),
        client_id(),
        url_encode(&redirect_uri()),
    );
    Redirect::temporary(&url)
}

// ── GET /api/v1/auth/gitea/callback ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code:  Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct GiteaTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GiteaUser {
    full_name:  String,
    login:      String,
    email:      String,
    is_admin:   bool,
}

pub async fn gitea_callback(
    State(pool): State<PgPool>,
    Query(q): Query<CallbackQuery>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let err_redirect = |msg: &str| {
        let url = format!("{}/#/auth/error?reason={}", app_url(), url_encode(msg));
        Ok(Redirect::temporary(&url))
    };

    if let Some(ref e) = q.error {
        return err_redirect(e);
    }

    let code = match q.code {
        Some(c) => c,
        None    => return err_redirect("No se recibió código de autorización"),
    };

    let http = reqwest::Client::new();

    // 1. Intercambiar code por access_token
    let token_res = http
        .post(format!("{}/login/oauth/access_token", gitea_url()))
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id":     client_id(),
            "client_secret": client_secret(),
            "code":          code,
            "redirect_uri":  redirect_uri(),
            "grant_type":    "authorization_code",
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let token_data: GiteaTokenResponse = token_res
        .json()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": "Respuesta de token inválida" }))))?;

    // 2. Obtener perfil del usuario
    let gitea_user: GiteaUser = http
        .get(format!("{}/api/v1/user", gitea_url()))
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": e.to_string() }))))?
        .json()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": "Perfil de usuario inválido" }))))?;

    // 3. Upsert usuario en DB local
    let role = if gitea_user.is_admin { "admin" } else { "user" };
    let display_name = if gitea_user.full_name.is_empty() {
        gitea_user.login.clone()
    } else {
        gitea_user.full_name.clone()
    };

    let user = sqlx::query!(
        r#"
        INSERT INTO users (email, display_name, password_hash, role, must_change_pw)
        VALUES ($1, $2, '', $3, false)
        ON CONFLICT (email) DO UPDATE
            SET display_name = EXCLUDED.display_name
        RETURNING id AS "id: uuid::Uuid", email, display_name, role
        "#,
        gitea_user.email,
        display_name,
        role,
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    // 4. Generar JWT de AgroDash
    let claims = Claims::new(user.id, user.email.clone(), user.role.clone());
    let jwt = encode_token(&claims)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    // 5. Redirigir al frontend con el token en el hash
    // Nunca en query string para evitar que quede en logs del servidor
    let redirect_url = format!("{}/#/auth/callback?token={}", app_url(), jwt);
    Ok(Redirect::temporary(&redirect_url))
}
