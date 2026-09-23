use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Type,
    ExtraField(String),
    MissingField(String),
    Const,
    Enum,
    Format(&'static str),
    Pattern(&'static str),
    Range,
    UnknownVersion,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Type => formatter.write_str("tipo inválido"),
            Self::ExtraField(name) => write!(formatter, "campo extra: {name}"),
            Self::MissingField(name) => write!(formatter, "campo obrigatório ausente: {name}"),
            Self::Const => formatter.write_str("constante divergente"),
            Self::Enum => formatter.write_str("valor fora do enum"),
            Self::Format(name) => write!(formatter, "formato inválido: {name}"),
            Self::Pattern(name) => write!(formatter, "padrão inválido: {name}"),
            Self::Range => formatter.write_str("intervalo inválido"),
            Self::UnknownVersion => formatter.write_str("schema_version desconhecida"),
        }
    }
}

pub fn validate_instance(schema: &Value, instance: &Value) -> Result<(), ValidationError> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(ValidationError::Type);
    }
    let Some(object) = instance.as_object() else {
        return Err(ValidationError::Type);
    };
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .ok_or(ValidationError::Type)?;
        for key in object.keys() {
            if !properties.contains_key(key) {
                return Err(ValidationError::ExtraField(key.clone()));
            }
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required {
            let Some(name) = field.as_str() else {
                continue;
            };
            if !object.contains_key(name) {
                return Err(ValidationError::MissingField(name.to_string()));
            }
        }
    }
    if let Some(version) = object.get("schema_version") {
        let expected = schema.pointer("/properties/schema_version/const");
        if expected.is_some() && Some(version) != expected {
            return Err(ValidationError::UnknownVersion);
        }
    }
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Ok(());
    };
    for (name, spec) in properties {
        if let Some(value) = object.get(name) {
            validate_value(spec, value)?;
        }
    }
    Ok(())
}

fn validate_value(schema: &Value, value: &Value) -> Result<(), ValidationError> {
    if let Some(constant) = schema.get("const") {
        if value != constant {
            return Err(ValidationError::Const);
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) {
            return Err(ValidationError::Enum);
        }
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => {
            let Some(text) = value.as_str() else {
                return Err(ValidationError::Type);
            };
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                if (text.chars().count() as u64) < min {
                    return Err(ValidationError::Range);
                }
            }
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                if (text.chars().count() as u64) > max {
                    return Err(ValidationError::Range);
                }
            }
            match schema.get("format").and_then(Value::as_str) {
                Some("uuid") if !is_uuid(text) => {
                    return Err(ValidationError::Format("uuid"));
                }
                Some("date-time") if !is_date_time(text) => {
                    return Err(ValidationError::Format("date-time"));
                }
                _ => {}
            }
            match schema.get("pattern").and_then(Value::as_str) {
                Some("opaque-id") if !is_opaque_id(text) => {
                    return Err(ValidationError::Pattern("opaque-id"));
                }
                Some("sku") if !is_sku(text) => {
                    return Err(ValidationError::Pattern("sku"));
                }
                Some("capability") if !is_capability(text) => {
                    return Err(ValidationError::Pattern("capability"));
                }
                _ => {}
            }
        }
        Some("integer") => {
            let Some(number) = value.as_i64() else {
                return Err(ValidationError::Type);
            };
            if let Some(min) = schema.get("minimum").and_then(Value::as_i64) {
                if number < min {
                    return Err(ValidationError::Range);
                }
            }
        }
        Some("object") => return validate_instance(schema, value),
        None => {}
        _ => return Err(ValidationError::Type),
    }
    Ok(())
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn is_date_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 20 && bytes[10] == b'T' && bytes.ends_with(b"Z")
}

fn is_opaque_id(value: &str) -> bool {
    (16..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn is_sku(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}
