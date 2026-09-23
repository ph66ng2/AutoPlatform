use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{contract_dir, read_json, CONTRACTS};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSurface {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractSurface {
    pub required: Vec<String>,
    pub fields: BTreeMap<String, FieldSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSurface(pub BTreeMap<String, ContractSurface>);

#[derive(Debug)]
pub enum SurfaceError {
    Catalog(crate::catalog::CatalogError),
    MissingContract(&'static str),
    Breaking(&'static str, String),
}

impl From<crate::catalog::CatalogError> for SurfaceError {
    fn from(value: crate::catalog::CatalogError) -> Self {
        Self::Catalog(value)
    }
}

impl std::fmt::Display for SurfaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "{error}"),
            Self::MissingContract(name) => {
                write!(formatter, "contrato ausente na superfície: {name}")
            }
            Self::Breaking(name, detail) => write!(formatter, "quebra em {name}: {detail}"),
        }
    }
}

pub fn extract_surface(schema: &Value) -> Result<ContractSurface, SurfaceError> {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| SurfaceError::Breaking("schema", "required ausente".into()))?;
    let required = required
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| SurfaceError::Breaking("schema", "properties ausente".into()))?;
    let mut fields = BTreeMap::new();
    for (name, spec) in properties {
        fields.insert(name.clone(), field_surface(spec));
    }
    Ok(ContractSurface { required, fields })
}

fn field_surface(spec: &Value) -> FieldSurface {
    let kind = spec
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| spec.get("const").map(json_kind))
        .unwrap_or_else(|| "unknown".to_string());
    let mut values = Vec::new();
    if let Some(constant) = spec.get("const").and_then(Value::as_str) {
        values.push(constant.to_string());
    }
    if let Some(options) = spec.get("enum").and_then(Value::as_array) {
        for option in options {
            if let Some(text) = option.as_str() {
                values.push(text.to_string());
            }
        }
    }
    values.sort();
    values.dedup();
    FieldSurface {
        kind,
        values: if values.is_empty() {
            None
        } else {
            Some(values)
        },
    }
}

fn json_kind(value: &Value) -> String {
    match value {
        Value::String(_) => "string",
        Value::Number(number) if number.is_i64() => "integer",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
    .to_string()
}

pub fn load_published_surface() -> Result<CatalogSurface, SurfaceError> {
    let value = read_json(&contract_dir().join("surface.json"))?;
    serde_json::from_value(value).map_err(|error| {
        SurfaceError::Catalog(crate::catalog::CatalogError::Json(error.to_string()))
    })
}

pub fn assert_compatible(
    published: &ContractSurface,
    current: &ContractSurface,
) -> Result<(), SurfaceError> {
    if published.required != current.required {
        return Err(SurfaceError::Breaking(
            "required",
            "conjunto required divergiu".into(),
        ));
    }
    for (name, field) in &published.fields {
        let Some(actual) = current.fields.get(name) else {
            return Err(SurfaceError::Breaking("field", format!("{name} removido")));
        };
        if actual.kind != field.kind {
            return Err(SurfaceError::Breaking(
                "type",
                format!("{name} mudou de tipo"),
            ));
        }
        if let Some(published_values) = &field.values {
            let Some(actual_values) = &actual.values else {
                return Err(SurfaceError::Breaking(
                    "enum",
                    format!("{name} perdeu enum"),
                ));
            };
            for value in published_values {
                if !actual_values.contains(value) {
                    return Err(SurfaceError::Breaking(
                        "enum",
                        format!("{name} perdeu valor {value}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn published_from_disk() -> Result<CatalogSurface, SurfaceError> {
    let mut catalog = BTreeMap::new();
    for name in CONTRACTS {
        let schema = crate::catalog::load_schema(name)?;
        catalog.insert((*name).to_string(), extract_surface(&schema)?);
    }
    Ok(CatalogSurface(catalog))
}
