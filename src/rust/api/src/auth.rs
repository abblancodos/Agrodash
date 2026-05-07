// src/auth.rs
//
// JWT + cookie HttpOnly.
// El token se guarda en una cookie HttpOnly/Secure/SameSite=Lax
// — JavaScript nunca puede leerlo.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const COOKIE_NAME: &str = "agrodash_session";
const EXPIRY_SECS: u64 = 60 * 60 * 24 * 7; // 7 días

fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").expect("JWT_SECRET no definida")
}

// ── Claims ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub email: String,
    pub role: String,
    pub exp: u64,
}

impl Claims {
    pub fn new(sub: Uuid, email: String, role: String) -> Self {
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time anterior a UNIX_EPOCH — clock del sistema mal configurado")
            .as_secs()
            + EXPIRY_SECS;
        Self {
            sub,
            email,
            role,
            exp,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

// ── Encode / decode ───────────────────────────────────────────────────────────

pub fn encode_token(claims: &Claims) -> Result<String, jsonwebtoken::errors::Error> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(jwt_secret().as_bytes()),
    )
}

pub fn decode_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret().as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

// ── Cookie helpers ────────────────────────────────────────────────────────────

/// Crea una cookie de sesión HttpOnly/Secure/SameSite=Lax con el JWT.
pub fn session_cookie(token: String) -> Cookie<'static> {
    let secure = std::env::var("COOKIE_SECURE").unwrap_or_else(|_| "true".into()) != "false";
    // Frontend y API en el mismo dominio — SameSite::Lax es suficiente
    Cookie::build((COOKIE_NAME, token))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(EXPIRY_SECS as i64))
        .build()
}

/// Crea una cookie de sesión vacía para hacer logout.
#[allow(dead_code)]
pub fn clear_session_cookie() -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, ""))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build()
}

// ── Extractor: Claims (requiere sesión) ───────────────────────────────────────

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Leer de cookie
        let jar = CookieJar::from_request_parts(parts, state).await.unwrap();
        if let Some(cookie) = jar.get(COOKIE_NAME) {
            if let Ok(claims) = decode_token(cookie.value()) {
                return Ok(claims);
            }
        }
        // Fallback: Authorization header (para herramientas de dev / curl)
        if let Some(auth) = parts.headers.get("authorization") {
            if let Ok(val) = auth.to_str() {
                if let Some(token) = val.strip_prefix("Bearer ") {
                    if let Ok(claims) = decode_token(token) {
                        return Ok(claims);
                    }
                }
            }
        }
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "No autorizado" })),
        ))
    }
}

// ── Extractor: OptionalClaims ─────────────────────────────────────────────────

pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl<S> FromRequestParts<S> for OptionalClaims
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state).await.unwrap();
        if let Some(cookie) = jar.get(COOKIE_NAME) {
            if let Ok(claims) = decode_token(cookie.value()) {
                return Ok(OptionalClaims(Some(claims)));
            }
        }
        if let Some(auth) = parts.headers.get("authorization") {
            if let Ok(val) = auth.to_str() {
                if let Some(token) = val.strip_prefix("Bearer ") {
                    if let Ok(claims) = decode_token(token) {
                        return Ok(OptionalClaims(Some(claims)));
                    }
                }
            }
        }
        Ok(OptionalClaims(None))
    }
}
