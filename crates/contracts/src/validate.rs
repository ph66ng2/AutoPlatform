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
    OneOf,
    Totals,
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
            Self::OneOf => formatter.write_str("variante de item inválida"),
            Self::Totals => formatter.write_str("totais inconsistentes"),
        }
    }
}

pub fn validate_instance(schema: &Value, instance: &Value) -> Result<(), ValidationError> {
    if schema
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "object")
    {
        return Err(ValidationError::Type);
    }
    let Some(object) = instance.as_object() else {
        return Err(ValidationError::Type);
    };
    if schema
        .get("maxProperties")
        .and_then(Value::as_u64)
        .is_some_and(|max| object.len() > max as usize)
    {
        return Err(ValidationError::Range);
    }
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
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, spec) in properties {
            if let Some(value) = object.get(name) {
                validate_value(spec, value)?;
            }
        }
    }
    validate_one_of(schema, instance)?;
    Ok(())
}

fn validate_one_of(schema: &Value, value: &Value) -> Result<(), ValidationError> {
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = variants
            .iter()
            .filter(|variant| validate_value(variant, value).is_ok())
            .count();
        if matches != 1 {
            return Err(ValidationError::OneOf);
        }
    }
    Ok(())
}

/// Validate the v2 wire shape and the arithmetic that JSON Schema cannot express.
pub fn validate_fato_comercial_v2(schema: &Value, instance: &Value) -> Result<(), ValidationError> {
    validate_instance(schema, instance)?;
    let items = instance["items"].as_array().ok_or(ValidationError::Type)?;
    let bundle = instance["pricing_mode"] == "bundle";
    let mut gross = 0_i64;
    let mut discount = 0_i64;
    let mut total = 0_i64;
    let mut ids = std::collections::HashSet::new();
    for item in items {
        let id = item["item_id"].as_str().ok_or(ValidationError::Type)?;
        if !ids.insert(id) {
            return Err(ValidationError::Totals);
        }
        if bundle {
            continue;
        }
        let quantity = item["quantity_milli"]
            .as_i64()
            .ok_or(ValidationError::Type)?;
        let unit = item["unit_price_cents"]
            .as_i64()
            .ok_or(ValidationError::Type)?;
        let actual_gross = item["gross_cents"].as_i64().ok_or(ValidationError::Type)?;
        let actual_discount = item["discount_cents"]
            .as_i64()
            .ok_or(ValidationError::Type)?;
        let actual_total = item["total_cents"].as_i64().ok_or(ValidationError::Type)?;
        let expected_gross = quantity
            .checked_mul(unit)
            .and_then(|v| v.checked_add(500))
            .map(|v| v / 1000)
            .ok_or(ValidationError::Range)?;
        if actual_gross != expected_gross
            || actual_discount > actual_gross
            || actual_total != actual_gross - actual_discount
        {
            return Err(ValidationError::Totals);
        }
        gross = gross
            .checked_add(actual_gross)
            .ok_or(ValidationError::Range)?;
        discount = discount
            .checked_add(actual_discount)
            .ok_or(ValidationError::Range)?;
        total = total
            .checked_add(actual_total)
            .ok_or(ValidationError::Range)?;
    }
    if bundle {
        let totals = &instance["totals"];
        let gross = totals["gross_cents"]
            .as_i64()
            .ok_or(ValidationError::Type)?;
        let discount = totals["discount_cents"]
            .as_i64()
            .ok_or(ValidationError::Type)?;
        let total = totals["total_cents"]
            .as_i64()
            .ok_or(ValidationError::Type)?;
        if discount > gross || total != gross - discount {
            return Err(ValidationError::Totals);
        }
        return Ok(());
    }
    if instance["totals"]["gross_cents"].as_i64() != Some(gross)
        || instance["totals"]["discount_cents"].as_i64() != Some(discount)
        || instance["totals"]["total_cents"].as_i64() != Some(total)
    {
        return Err(ValidationError::Totals);
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
        Some("object") => validate_instance(schema, value)?,
        Some("array") => {
            let values = value.as_array().ok_or(ValidationError::Type)?;
            if schema
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|min| values.len() < min as usize)
            {
                return Err(ValidationError::Range);
            }
            if let Some(item_schema) = schema.get("items") {
                for item in values {
                    validate_value(item_schema, item)?;
                }
            }
        }
        None => {}
        _ => return Err(ValidationError::Type),
    }
    validate_one_of(schema, value)?;
    if schema.get("type").is_none()
        && (schema.get("required").is_some() || schema.get("properties").is_some())
    {
        validate_instance(schema, value)?;
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
