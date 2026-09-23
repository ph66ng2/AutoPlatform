use std::fmt;

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use uuid::Uuid;

use crate::AccessError;

/// Segredo HMAC do JWT. Não entra em `Debug`.
pub struct JwtSecret(Vec<u8>);

impl JwtSecret {
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }
}

impl fmt::Debug for JwtSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JwtSecret([REDACTED])")
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    role: Option<String>,
}

pub fn account_id(secret: &JwtSecret, token: &str) -> Result<Uuid, AccessError> {
    let token = token.trim().trim_start_matches("Bearer ").trim();
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["authenticated"]);
    validation.set_required_spec_claims(&["exp", "sub", "aud"]);
    let data = decode::<Claims>(token, &DecodingKey::from_secret(&secret.0), &validation)
        .map_err(|_| AccessError::Unauthenticated)?;
    if data.claims.role.as_deref() != Some("authenticated") {
        return Err(AccessError::Unauthenticated);
    }
    Uuid::parse_str(&data.claims.sub).map_err(|_| AccessError::Unauthenticated)
}
