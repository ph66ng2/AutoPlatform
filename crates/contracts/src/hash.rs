use std::{fs, path::Path};

use sha2::{Digest, Sha256};

use crate::catalog::CONTRACTS;

#[derive(Debug)]
pub enum HashError {
    Io(String),
    Manifest,
    Mismatch(String),
}

impl std::fmt::Display for HashError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => formatter.write_str(error),
            Self::Manifest => formatter.write_str("manifest de contrato ausente ou incompleto"),
            Self::Mismatch(name) => write!(formatter, "hash divergente: {name}"),
        }
    }
}

pub fn verify_manifest(dir: impl AsRef<Path>) -> Result<(), HashError> {
    let dir = dir.as_ref();
    let manifest = fs::read_to_string(dir.join("manifest.sha256"))
        .map_err(|error| HashError::Io(error.to_string()))?;
    let mut listed = Vec::new();
    for line in manifest.lines() {
        let (digest, name) = line.split_once("  ").ok_or(HashError::Manifest)?;
        listed.push(name.to_string());
        let path = dir.join(name);
        let actual = file_sha256(&path)?;
        if actual != digest {
            return Err(HashError::Mismatch(name.to_string()));
        }
    }
    let expected: Vec<String> = CONTRACTS
        .iter()
        .map(|name| format!("{name}.schema.json"))
        .collect();
    if listed != expected {
        return Err(HashError::Manifest);
    }
    Ok(())
}

pub fn file_sha256(path: &Path) -> Result<String, HashError> {
    let bytes = fs::read(path).map_err(|error| HashError::Io(error.to_string()))?;
    Ok(hex(Sha256::digest(&bytes)))
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
