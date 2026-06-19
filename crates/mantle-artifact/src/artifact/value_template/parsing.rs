use std::collections::BTreeSet;

use super::model::{ArtifactMapEntry, ArtifactRecordField, ArtifactValue};
use super::scalar::ArtifactScalarValue;
use crate::validation::validate_ident_field;
use crate::{
    Error, MAX_PRIMITIVE_DATA_BYTES, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, Result,
};

const STRING_VALUE_PREFIX: &str = "String(";
const BYTES_VALUE_PREFIX: &str = "Bytes(";

pub(super) fn parse_value(label: &str, depth: usize) -> Result<ArtifactValue> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "artifact value exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
        )));
    }
    if let Some(value) = ArtifactScalarValue::parse_label("artifact scalar value", label)? {
        return Ok(ArtifactValue::Scalar(value));
    }
    if let Some(value) = parse_primitive_data_value(label)? {
        return Ok(value);
    }
    if let Some(body) = label.strip_prefix("List[") {
        let Some(body) = body.strip_suffix(']') else {
            return Err(Error::new(format!("{label} is not a list value")));
        };
        return parse_list(body, depth + 1);
    }
    if let Some(body) = label.strip_prefix("Map[") {
        let Some(body) = body.strip_suffix(']') else {
            return Err(Error::new(format!("{label} is not a map value")));
        };
        return parse_map(label, body, depth + 1);
    }
    if let Some(open) = top_level_char(label, '{')? {
        let Some(body) = label.strip_suffix('}') else {
            return Err(Error::new(format!("{label} is not a record value")));
        };
        let constructor = &label[..open];
        validate_ident_field("artifact record value type", constructor)?;
        return parse_record(constructor, &body[open + 1..], depth + 1);
    }
    if let Some(open) = top_level_char(label, '(')? {
        let Some(body) = label.strip_suffix(')') else {
            return Err(Error::new(format!("{label} is not an enum payload value")));
        };
        let variant = &label[..open];
        validate_ident_field("artifact enum variant value", variant)?;
        return Ok(ArtifactValue::EnumVariant {
            variant: variant.to_string(),
            payload: Box::new(parse_value(&body[open + 1..], depth + 1)?),
        });
    }
    validate_ident_field("artifact atom value", label)?;
    Ok(ArtifactValue::Atom(label.to_string()))
}

fn parse_primitive_data_value(label: &str) -> Result<Option<ArtifactValue>> {
    if let Some(body) = label.strip_prefix(STRING_VALUE_PREFIX) {
        let Some(hex) = body.strip_suffix(')') else {
            return Err(Error::new(format!("{label} is not a String value")));
        };
        let bytes = parse_hex_bytes("String value", hex)?;
        let value = String::from_utf8(bytes)
            .map_err(|_| Error::new(format!("String value {label} is not valid UTF-8")))?;
        return Ok(Some(ArtifactValue::String(value)));
    }
    if let Some(body) = label.strip_prefix(BYTES_VALUE_PREFIX) {
        let Some(hex) = body.strip_suffix(')') else {
            return Err(Error::new(format!("{label} is not a Bytes value")));
        };
        return Ok(Some(ArtifactValue::Bytes(parse_hex_bytes(
            "Bytes value",
            hex,
        )?)));
    }
    Ok(None)
}

fn parse_hex_bytes(field: &str, hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err(Error::new(format!(
            "{field} hex encoding must have even length"
        )));
    }
    let len = hex.len() / 2;
    if len > MAX_PRIMITIVE_DATA_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum primitive data length of {MAX_PRIMITIVE_DATA_BYTES} bytes"
        )));
    }
    let mut bytes = Vec::with_capacity(len);
    for pair in hex.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0])
            .ok_or_else(|| Error::new(format!("{field} contains non-lowercase-hex data")))?;
        let low = hex_nibble(pair[1])
            .ok_or_else(|| Error::new(format!("{field} contains non-lowercase-hex data")))?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn parse_record(constructor: &str, body: &str, depth: usize) -> Result<ArtifactValue> {
    if body.is_empty() {
        return Err(Error::new(format!(
            "fieldless record values use {constructor}; braced record values must declare at least one field"
        )));
    }
    let parts = split_top_level(body, ',')?;
    if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "record value {constructor}{{{body}}} field count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    let mut fields = Vec::with_capacity(parts.len());
    let mut seen = BTreeSet::new();
    for part in parts {
        let index = top_level_char(part, ':')?.ok_or_else(|| {
            Error::new(format!(
                "record value {constructor}{{{body}}} contains malformed field"
            ))
        })?;
        let name = &part[..index];
        validate_ident_field("artifact record field", name)?;
        if !seen.insert(name) {
            return Err(Error::new(format!(
                "record value {constructor}{{{body}}} duplicates field {name}"
            )));
        }
        fields.push(ArtifactRecordField {
            name: name.to_string(),
            value: parse_value(&part[index + 1..], depth)?,
        });
    }
    Ok(ArtifactValue::Record {
        constructor: constructor.to_string(),
        fields,
    })
}

fn parse_list(body: &str, depth: usize) -> Result<ArtifactValue> {
    let items = if body.is_empty() {
        Vec::new()
    } else {
        let parts = split_top_level(body, ',')?;
        if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "list value item count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        parts
            .into_iter()
            .map(|part| parse_value(part, depth))
            .collect::<Result<Vec<_>>>()?
    };
    Ok(ArtifactValue::List(items))
}

fn parse_map(label: &str, body: &str, depth: usize) -> Result<ArtifactValue> {
    let mut entries = Vec::new();
    if !body.is_empty() {
        let parts = split_top_level(body, ',')?;
        if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "map value {label} entry count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        entries.reserve(parts.len());
        let mut seen = BTreeSet::new();
        for part in parts {
            let index = top_level_fat_arrow(part)
                .ok_or_else(|| Error::new(format!("map value {label} contains malformed entry")))?;
            let key = parse_value(&part[..index], depth)?;
            if !seen.insert(key.clone()) {
                return Err(Error::new(format!(
                    "map value {label} duplicates key {}",
                    key.label()
                )));
            }
            entries.push(ArtifactMapEntry {
                key,
                value: parse_value(&part[index + 2..], depth)?,
            });
        }
    }
    Ok(ArtifactValue::Map(entries))
}

fn split_top_level(value: &str, separator: char) -> Result<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => {
                paren_depth = paren_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced parentheses"))
                })?
            }
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced brackets"))
                })?
            }
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced braces"))
                })?
            }
            _ => {}
        }
        if ch == separator && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            parts.push(&value[start..index]);
            start = index + ch.len_utf8();
        }
    }
    if paren_depth != 0 || bracket_depth != 0 || brace_depth != 0 {
        return Err(Error::new(format!("value label {value} is unbalanced")));
    }
    parts.push(&value[start..]);
    Ok(parts)
}

fn top_level_char(value: &str, target: char) -> Result<Option<usize>> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        if ch == target && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            return Ok(Some(index));
        }
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => {
                paren_depth = paren_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced parentheses"))
                })?
            }
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced brackets"))
                })?
            }
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced braces"))
                })?
            }
            _ => {}
        }
    }
    if paren_depth != 0 || bracket_depth != 0 || brace_depth != 0 {
        return Err(Error::new(format!("value label {value} is unbalanced")));
    }
    Ok(None)
}

fn top_level_fat_arrow(value: &str) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '=' if paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0
                && value[index..].starts_with("=>") =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}
