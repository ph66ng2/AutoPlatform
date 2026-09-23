use std::{fs, path::PathBuf};

use serde_json::Value;

pub const SCHEMA_VERSION: &str = "1";

pub const CONTRACTS: &[&str] = &[
    "entitlement",
    "estado-financeiro",
    "estoque",
    "fato-comercial",
    "fiscal-document",
    "payment-intent",
];

const ENVELOPE: &[&str] = &["schema_version", "company_id", "correlation_id"];

#[derive(Debug)]
pub enum CatalogError {
    Io(String),
    Json(String),
    Envelope(&'static str),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) | Self::Json(error) => formatter.write_str(error),
            Self::Envelope(name) => write!(formatter, "envelope incompleto em {name}"),
        }
    }
}

#[must_use]
pub fn contract_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/v1")
}

#[must_use]
pub fn schema_path(name: &str) -> PathBuf {
    contract_dir().join(format!("{name}.schema.json"))
}

pub fn load_schema(name: &str) -> Result<Value, CatalogError> {
    read_json(&schema_path(name))
}

pub fn assert_envelope(schema: &Value) -> Result<(), CatalogError> {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or(CatalogError::Envelope("required"))?;
    let names: Vec<&str> = required.iter().filter_map(Value::as_str).collect();
    for field in ENVELOPE {
        if !names.contains(field) {
            return Err(CatalogError::Envelope(field));
        }
    }
    let version = schema
        .pointer("/properties/schema_version/const")
        .and_then(Value::as_str);
    if version != Some(SCHEMA_VERSION) {
        return Err(CatalogError::Envelope("schema_version"));
    }
    Ok(())
}

pub fn list_fixtures(name: &str, valid: bool) -> Result<Vec<Value>, CatalogError> {
    let folder = if valid { "valid" } else { "invalid" };
    let path = contract_dir()
        .join("fixtures")
        .join(folder)
        .join(format!("{name}.json"));
    if path.is_file() {
        return Ok(vec![read_json(&path)?]);
    }
    Ok(Vec::new())
}

pub fn list_named_invalid(name: &str) -> Result<Vec<(String, Value)>, CatalogError> {
    let dir = contract_dir().join("fixtures/invalid");
    let mut out = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|error| CatalogError::Io(error.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|error| CatalogError::Io(error.to_string()))?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with(&format!("{name}-")) && file_name.ends_with(".json") {
            out.push((file_name.into_owned(), read_json(&entry.path())?));
        }
    }
    Ok(out)
}

pub fn read_json(path: &std::path::Path) -> Result<Value, CatalogError> {
    let text = fs::read_to_string(path).map_err(|error| CatalogError::Io(error.to_string()))?;
    serde_json::from_str(&text).map_err(|error| CatalogError::Json(error.to_string()))
}
