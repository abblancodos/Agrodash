// src/auth.rs
//
// JWT + middleware de autenticación.
// El token se pasa en el header Authorization: Bearer <token>

use axum::{
    extract::{FromRequestParts, Request},
    http::{request::Parts, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const JWT_SECRET_ENV: &str = "JWT_SECRET";

fn secret() -> String {
    std::env::var(JWT_SECRET_ENV)
        .expect("JWT_SECRET no está definida en .env")
}

// ── Claims ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub:   Uuid,    // user id
    pub email: String,
    pub role:  String,
    pub exp:   usize,   // unix timestamp de expiración
}

impl Claims {
    pub fn new(id: Uuid, email: String, role: String) -> Self {
        let exp = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(30))
            .unwrap()
            .timestamp() as usize;
        Self { sub: id, email, role, exp }
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

// ── Generar / verificar tokens ────────────────────────────────────────────────

pub fn encode_token(claims: &Claims) -> Result<String, jsonwebtoken::errors::Error> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret().as_bytes()),
    )
}

pub fn decode_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret().as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

// ── Extractor de Claims desde request ────────────────────────────────────────
//
// Uso en handlers:
//   async fn my_handler(claims: Claims, ...) -> ...
//
// Si el token no es válido o falta → 401

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for Claims {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Token no proporcionado" })),
                )
            })?;

        decode_token(auth).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Token inválido o expirado" })),
            )
        })
    }
}

// ── Extractor opcional — no falla si no hay token ────────────────────────────

pub struct OptionalClaims(pub Option<Claims>);

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for OptionalClaims {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let claims = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|t| decode_token(t).ok());
        Ok(OptionalClaims(claims))
    }
}