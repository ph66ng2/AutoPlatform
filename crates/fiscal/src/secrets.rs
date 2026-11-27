use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use ap_identity::{Permission, Session};
use postgres::Client;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::FiscalError;

pub struct MasterKey([u8; 32]);

impl MasterKey {
    /// A chave-mestra é carregada somente do ambiente do serviço.
    pub fn from_env() -> Result<Self, FiscalError> {
        let hex = std::env::var("FISCAL_MASTER_KEY_HEX").map_err(|_| FiscalError::Crypto)?;
        Self::from_hex(&hex)
    }

    pub fn from_hex(hex: &str) -> Result<Self, FiscalError> {
        let bytes = hex.as_bytes();
        if bytes.len() != 64 {
            return Err(FiscalError::Crypto);
        }
        let mut key = [0_u8; 32];
        for (index, pair) in bytes.chunks_exact(2).enumerate() {
            let hi = (pair[0] as char).to_digit(16).ok_or(FiscalError::Crypto)?;
            let lo = (pair[1] as char).to_digit(16).ok_or(FiscalError::Crypto)?;
            key[index] = ((hi << 4) | lo) as u8;
        }
        Ok(Self(key))
    }
}

impl Drop for MasterKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey([REDACTED])")
    }
}

fn aad(company: Uuid, establishment: Uuid, provider: &str, key_id: &str) -> Vec<u8> {
    serde_json::to_vec(&(
        company.to_string(),
        establishment.to_string(),
        provider,
        key_id,
    ))
    .expect("serialização de identificadores")
}

fn encrypt(cipher: &Aes256Gcm, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, FiscalError> {
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| FiscalError::Crypto)?;
    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

fn decrypt(cipher: &Aes256Gcm, stored: &[u8], aad: &[u8]) -> Result<Vec<u8>, FiscalError> {
    let (nonce, ciphertext) = stored.split_at_checked(12).ok_or(FiscalError::Crypto)?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| FiscalError::Crypto)
}

fn seal(
    master: &MasterKey,
    company: Uuid,
    establishment: Uuid,
    provider: &str,
    key_id: &str,
    secret: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), FiscalError> {
    if provider.is_empty() || key_id.is_empty() || secret.is_empty() {
        return Err(FiscalError::Crypto);
    }
    let context = aad(company, establishment, provider, key_id);
    let mut dek = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(&mut *dek);
    let wrapped = encrypt(
        &Aes256Gcm::new_from_slice(&master.0).map_err(|_| FiscalError::Crypto)?,
        &*dek,
        &context,
    )?;
    let encrypted = encrypt(
        &Aes256Gcm::new_from_slice(&dek[..]).map_err(|_| FiscalError::Crypto)?,
        secret,
        &context,
    )?;
    Ok((wrapped, encrypted))
}

fn open(
    master: &MasterKey,
    company: Uuid,
    establishment: Uuid,
    provider: &str,
    key_id: &str,
    wrapped: &[u8],
    encrypted: &[u8],
) -> Result<Zeroizing<Vec<u8>>, FiscalError> {
    let context = aad(company, establishment, provider, key_id);
    let dek = Zeroizing::new(decrypt(
        &Aes256Gcm::new_from_slice(&master.0).map_err(|_| FiscalError::Crypto)?,
        wrapped,
        &context,
    )?);
    let secret = decrypt(
        &Aes256Gcm::new_from_slice(&dek).map_err(|_| FiscalError::Crypto)?,
        encrypted,
        &context,
    )?;
    Ok(Zeroizing::new(secret))
}

fn authorized(session: Session) -> Result<(), FiscalError> {
    if session.allows(Permission::Fiscal) {
        Ok(())
    } else {
        Err(FiscalError::Denied)
    }
}

fn context(
    client: &mut Client,
    session: Session,
) -> Result<postgres::Transaction<'_>, FiscalError> {
    authorized(session)?;
    let mut tx = client.transaction().map_err(|_| FiscalError::Database)?;
    tx.execute("SET LOCAL ROLE fiscal_app", &[])
        .map_err(|_| FiscalError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id',$1,true),set_config('app.account_id',$2,true)",
        &[
            &session.company_id.to_string(),
            &session.account_id.to_string(),
        ],
    )
    .map_err(|_| FiscalError::Database)?;
    Ok(tx)
}

/// Grava apenas DEK encapsulada e segredo cifrado. A chave-mestra não entra no banco.
pub fn save_provider_secret(
    client: &mut Client,
    session: Session,
    establishment: Uuid,
    provider: &str,
    key_id: &str,
    master: &MasterKey,
    secret: &[u8],
) -> Result<(), FiscalError> {
    authorized(session)?;
    let (wrapped, encrypted) = seal(
        master,
        session.company_id,
        establishment,
        provider,
        key_id,
        secret,
    )?;
    let mut tx = context(client, session)?;
    tx.execute(
        "INSERT INTO fiscal.provider_configs(company_id,establishment_id,provider,key_id,wrapped_dek,encrypted_secret) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,establishment_id,provider) DO UPDATE SET key_id=EXCLUDED.key_id,wrapped_dek=EXCLUDED.wrapped_dek,encrypted_secret=EXCLUDED.encrypted_secret,rotated_at=clock_timestamp()",
        &[&session.company_id, &establishment, &provider, &key_id, &wrapped, &encrypted],
    ).map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)
}

/// O segredo só existe em memória dentro do callback do adapter server-side.
pub fn read_provider_secret<T>(
    client: &mut Client,
    session: Session,
    establishment: Uuid,
    provider: &str,
    master: &MasterKey,
    use_secret: impl FnOnce(&[u8]) -> T,
) -> Result<T, FiscalError> {
    let mut tx = context(client, session)?;
    let row = tx.query_opt(
        "SELECT key_id,wrapped_dek,encrypted_secret FROM fiscal.provider_configs WHERE company_id=$1 AND establishment_id=$2 AND provider=$3",
        &[&session.company_id, &establishment, &provider],
    ).map_err(|_| FiscalError::Database)?.ok_or(FiscalError::NotFound)?;
    tx.commit().map_err(|_| FiscalError::Database)?;
    let key_id: String = row.get(0);
    let wrapped: Vec<u8> = row.get(1);
    let encrypted: Vec<u8> = row.get(2);
    let plaintext = open(
        master,
        session.company_id,
        establishment,
        provider,
        &key_id,
        &wrapped,
        &encrypted,
    )?;
    Ok(use_secret(&plaintext))
}

/// Rotaciona em transação única e trava a linha para evitar perda concorrente.
pub fn rotate_provider_secret(
    client: &mut Client,
    session: Session,
    establishment: Uuid,
    provider: &str,
    old_master: &MasterKey,
    new_master: &MasterKey,
    new_key_id: &str,
) -> Result<(), FiscalError> {
    let mut tx = context(client, session)?;
    let row = tx.query_opt(
        "SELECT key_id,wrapped_dek,encrypted_secret FROM fiscal.provider_configs WHERE company_id=$1 AND establishment_id=$2 AND provider=$3 FOR UPDATE",
        &[&session.company_id, &establishment, &provider],
    ).map_err(|_| FiscalError::Database)?.ok_or(FiscalError::NotFound)?;
    let old_key_id: String = row.get(0);
    let wrapped: Vec<u8> = row.get(1);
    let encrypted: Vec<u8> = row.get(2);
    let plaintext = open(
        old_master,
        session.company_id,
        establishment,
        provider,
        &old_key_id,
        &wrapped,
        &encrypted,
    )?;
    let (new_wrapped, new_encrypted) = seal(
        new_master,
        session.company_id,
        establishment,
        provider,
        new_key_id,
        &plaintext,
    )?;
    tx.execute(
        "UPDATE fiscal.provider_configs SET key_id=$4,wrapped_dek=$5,encrypted_secret=$6,rotated_at=clock_timestamp() WHERE company_id=$1 AND establishment_id=$2 AND provider=$3",
        &[&session.company_id, &establishment, &provider, &new_key_id, &new_wrapped, &new_encrypted],
    ).map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)
}

#[cfg(test)]
mod tests {
    use super::{open, seal, MasterKey};
    use uuid::Uuid;

    #[test]
    fn envelope_binds_tenant_and_provider_and_redacts_master() {
        let master = MasterKey::from_hex(&"01".repeat(32)).expect("synthetic key");
        let company = Uuid::from_u128(1);
        let establishment = Uuid::from_u128(2);
        let secret = b"synthetic-token";
        let (wrapped, encrypted) =
            seal(&master, company, establishment, "nfse", "v1", secret).expect("seal");
        assert!(!wrapped.windows(secret.len()).any(|window| window == secret));
        assert!(!encrypted
            .windows(secret.len())
            .any(|window| window == secret));
        assert_eq!(
            &*open(
                &master,
                company,
                establishment,
                "nfse",
                "v1",
                &wrapped,
                &encrypted
            )
            .expect("open"),
            secret
        );
        assert!(open(
            &master,
            Uuid::from_u128(3),
            establishment,
            "nfse",
            "v1",
            &wrapped,
            &encrypted
        )
        .is_err());
        assert!(open(
            &master,
            company,
            establishment,
            "nfe",
            "v1",
            &wrapped,
            &encrypted
        )
        .is_err());
        assert_eq!(format!("{master:?}"), "MasterKey([REDACTED])");
    }
}
