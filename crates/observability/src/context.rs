use std::fmt;

use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const HEADER_CORRELATION_ID: &str = "x-correlation-id";
pub const HEADER_EVENT_ID: &str = "x-event-id";
pub const HEADER_TENANT_KEY: &str = "x-tenant-key";
pub const HEADER_TENANT_PSEUDONYM: &str = "x-tenant-pseudonym";

/// Identificador de uma cadeia de trabalho. Não carrega segredo nem documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelationId(String);

impl CorrelationId {
    #[must_use]
    pub fn generate() -> Self {
        Self(random_token())
    }

    /// Aceita só um token opaco. Valor inseguro é trocado por um id novo.
    #[must_use]
    pub fn from_header(raw: Option<&str>) -> Self {
        raw.and_then(Self::trusted).unwrap_or_else(Self::generate)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn trusted(raw: &str) -> Option<Self> {
        is_opaque_token(raw).then(|| Self(raw.to_string()))
    }
}

/// Token opaco de correlação. Rejeita palavra de credencial mesmo com o alfabeto certo.
#[must_use]
pub fn is_opaque_token(value: &str) -> bool {
    (16..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        && !contains_credential_word(value)
}

fn contains_credential_word(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "bearer",
        "private",
        "credential",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Identificador de um evento dentro da cadeia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventId(String);

impl EventId {
    #[must_use]
    pub fn generate() -> Self {
        Self(random_token())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Tenant irreversível para log. A chave crua não sai daqui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantPseudonym(String);

impl TenantPseudonym {
    #[must_use]
    pub fn from_raw(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Self("absent".to_string());
        }
        Self(hex_sha256(trimmed))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TenantPseudonym {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn random_token() -> String {
    Uuid::new_v4().simple().to_string()
}

fn hex_sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
