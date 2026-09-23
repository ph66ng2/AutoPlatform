use crate::hmac::{constant_eq, hex_lower, hmac_sha256};

pub const ASAAS_TOKEN_HEADER: &str = "asaas-access-token";
pub const MP_SIGNATURE_HEADER: &str = "x-signature";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookError {
    MissingSignature,
    InvalidSignature,
}

/// Asaas sandbox: o token do webhook fica no header, nunca no desktop.
pub fn verify_asaas(header: Option<&str>, webhook_token: &str) -> Result<(), WebhookError> {
    let Some(provided) = header else {
        return Err(WebhookError::MissingSignature);
    };
    if constant_eq(provided.as_bytes(), webhook_token.as_bytes()) {
        Ok(())
    } else {
        Err(WebhookError::InvalidSignature)
    }
}

/// Mercado Pago sandbox: `ts` + payload assinados em `v1`.
pub fn verify_mercado_pago(
    signature: Option<&str>,
    payload: &str,
    secret: &str,
) -> Result<(), WebhookError> {
    let Some(header) = signature else {
        return Err(WebhookError::MissingSignature);
    };
    let mut ts = None;
    let mut v1 = None;
    for part in header.split(',') {
        let (key, value) = part.split_once('=').ok_or(WebhookError::InvalidSignature)?;
        match key.trim() {
            "ts" => ts = Some(value.trim()),
            "v1" => v1 = Some(value.trim()),
            _ => {}
        }
    }
    let (ts, v1) = match (ts, v1) {
        (Some(ts), Some(v1)) => (ts, v1),
        _ => return Err(WebhookError::InvalidSignature),
    };
    let manifest = format!("{ts}.{payload}");
    let expected = hex_lower(&hmac_sha256(secret.as_bytes(), manifest.as_bytes()));
    if constant_eq(expected.as_bytes(), v1.as_bytes()) {
        Ok(())
    } else {
        Err(WebhookError::InvalidSignature)
    }
}
