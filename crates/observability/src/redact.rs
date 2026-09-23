/// Remove documento, credencial e XML antes do texto ir para log ou trace.
#[must_use]
pub fn redact(input: &str) -> String {
    if contains_xml_payload(input) || contains_private_key(input) {
        return "[REDACTED]".to_string();
    }
    let mut output = redact_urls(input);
    output = redact_assignments(&output);
    output = redact_bearer(&output);
    output = redact_emails(&output);
    output = redact_formatted_documents(&output);
    output = redact_bounded_digits(&output, 14);
    redact_bounded_digits(&output, 11)
}

#[must_use]
pub fn contains_sensitive(input: &str) -> bool {
    redact(input) != input
}

fn contains_xml_payload(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    ["<nfe", "<nfse", "<?xml", "<xml", "<infnfe", "<cfe"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn contains_private_key(input: &str) -> bool {
    let upper = input.to_ascii_uppercase();
    upper.contains("PRIVATE KEY") && upper.contains("BEGIN ")
}

fn redact_urls(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"://") {
            output.push_str("://");
            index += 3;
            let rest = &bytes[index..];
            if let Some(at) = rest.iter().position(|byte| *byte == b'@') {
                let userinfo = &rest[..at];
                let tail_end = rest
                    .iter()
                    .position(|byte| *byte == b'/' || byte.is_ascii_whitespace())
                    .unwrap_or(rest.len());
                if at < tail_end && userinfo.contains(&b':') {
                    output.push_str("[REDACTED]@");
                    index += at + 1;
                    continue;
                }
            }
            continue;
        }
        let character = input[index..].chars().next().expect("char");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn redact_assignments(input: &str) -> String {
    let keys = [
        "password=",
        "token=",
        "secret=",
        "document=",
        "cpf=",
        "cnpj=",
        "authorization=",
    ];
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let matched = keys
            .iter()
            .find_map(|key| lower[index..].starts_with(key).then_some(key.len()));
        if let Some(key_len) = matched {
            output.push_str(&input[index..index + key_len]);
            output.push_str("[REDACTED]");
            index += key_len;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }
        let character = input[index..].chars().next().expect("char");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn redact_bearer(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if lower[index..].starts_with("bearer ") {
            output.push_str("Bearer [REDACTED]");
            index += "bearer ".len();
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }
        let character = input[index..].chars().next().expect("char");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn redact_emails(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '@' {
            let start = chars[..index]
                .iter()
                .rposition(|character| !is_email_char(*character))
                .map_or(0, |position| position + 1);
            let mut end = index + 1;
            let mut dot = false;
            while end < chars.len() && is_email_char(chars[end]) {
                dot |= chars[end] == '.';
                end += 1;
            }
            if start < index && dot && end > index + 1 {
                for _ in 0..(index - start) {
                    output.pop();
                }
                output.push_str("[REDACTED]");
                index = end;
                continue;
            }
        }
        output.push(chars[index]);
        index += 1;
    }
    output
}

fn is_email_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '%' | '+' | '-')
}

fn redact_formatted_documents(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some(size) = formatted_document_len(&bytes[index..]) {
            output.push_str("[REDACTED]");
            index += size;
            continue;
        }
        let character = input[index..].chars().next().expect("char");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn formatted_document_len(bytes: &[u8]) -> Option<usize> {
    if cnpj_at(bytes) {
        Some(18)
    } else if cpf_at(bytes) {
        Some(14)
    } else {
        None
    }
}

fn cpf_at(bytes: &[u8]) -> bool {
    pattern(bytes, "NNN.NNN.NNN-NN")
}

fn cnpj_at(bytes: &[u8]) -> bool {
    pattern(bytes, "NN.NNN.NNN/NNNN-NN")
}

fn pattern(bytes: &[u8], shape: &str) -> bool {
    let expected = shape.as_bytes();
    if bytes.len() < expected.len() {
        return false;
    }
    expected
        .iter()
        .zip(bytes)
        .all(|(marker, byte)| match marker {
            b'N' => byte.is_ascii_digit(),
            other => byte == other,
        })
}

fn redact_bounded_digits(input: &str, len: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            if index - start == len {
                output.push_str("[REDACTED]");
            } else {
                for character in &chars[start..index] {
                    output.push(*character);
                }
            }
            continue;
        }
        output.push(chars[index]);
        index += 1;
    }
    output
}
