use super::model::{ArtifactValue, MapProjectionMode};
use super::parsing::parse_value;
use super::projection::{
    ProjectionKeySetKind, labels, validate_projection_key_set, validate_projection_keys,
};
use crate::validation::{validate_count, validate_ident_field, validate_value_label};
use crate::{
    Error, MAX_PRIMITIVE_DATA_BYTES, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, Result,
    TypeId,
};
use std::fmt::Write as _;

const STRING_VALUE_PREFIX: &str = "String(";
const BYTES_VALUE_PREFIX: &str = "Bytes(";
const HEX_TABLE: &[u8; 16] = b"0123456789abcdef";

impl ArtifactValue {
    pub fn parse(label: &str) -> Result<Self> {
        Self::parse_field("artifact value", label)
    }

    pub(crate) fn parse_field(field: &str, label: &str) -> Result<Self> {
        validate_value_label(field, label)?;
        let value = parse_value(label, 0)?;
        value.validate(field)?;
        Ok(value)
    }

    pub fn process_ref(type_id: TypeId, pid: u64) -> Self {
        Self::ProcessRef { type_id, pid }
    }

    pub fn validate(&self, field: &str) -> Result<()> {
        self.validate_shape(field, 0)?;
        self.validate_generated_label_len(field)
    }

    pub(crate) fn validate_without_process_ref(&self, field: &str) -> Result<()> {
        self.validate(field)?;
        if self.contains_process_ref() {
            return Err(Error::new(format!(
                "{field} must not contain a process reference value"
            )));
        }
        Ok(())
    }

    fn validate_shape(&self, field: &str, depth: usize) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        match self {
            Self::Atom(value) => validate_ident_field(field, value),
            Self::String(value) => validate_primitive_data_len(field, value.len()),
            Self::Bytes(value) => validate_primitive_data_len(field, value.len()),
            Self::Scalar(value) => value.ty().validate_value(field, value.value()),
            Self::EnumVariant { variant, payload } => {
                validate_ident_field(&format!("{field}.variant"), variant)?;
                payload.validate_shape(&format!("{field}.payload"), depth + 1)
            }
            Self::Record {
                constructor,
                fields,
            } => {
                validate_ident_field(&format!("{field}.constructor"), constructor)?;
                validate_count(
                    &format!("{field}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, entry) in fields.iter().enumerate() {
                    let name = entry.name.as_str();
                    validate_ident_field(&format!("{field}.field"), name)?;
                    if fields[..index]
                        .iter()
                        .any(|previous| previous.name == entry.name)
                    {
                        return Err(Error::new(format!(
                            "{field} duplicates field {}",
                            entry.name
                        )));
                    }
                    entry
                        .value
                        .validate_shape(&format!("{field}.field.{name}"), depth + 1)?;
                }
                Ok(())
            }
            Self::List(items) => {
                validate_count(
                    &format!("{field}.item_count"),
                    items.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, value) in items.iter().enumerate() {
                    value.validate_shape(&format!("{field}.item.{index}"), depth + 1)?;
                }
                Ok(())
            }
            Self::Map(entries) => {
                validate_count(
                    &format!("{field}.entry_count"),
                    entries.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, entry) in entries.iter().enumerate() {
                    entry
                        .key
                        .validate_shape(&format!("{field}.entry.{index}.key"), depth + 1)?;
                    if entries[..index]
                        .iter()
                        .any(|previous| previous.key == entry.key)
                    {
                        return Err(Error::new(format!(
                            "{field} duplicates key {}",
                            entry.key.label()
                        )));
                    }
                    entry
                        .value
                        .validate_shape(&format!("{field}.entry.{index}.value"), depth + 1)?;
                }
                Ok(())
            }
            Self::ProcessRef { pid, .. } => {
                if *pid == 0 {
                    return Err(Error::new(format!(
                        "{field} process reference pid must be greater than zero"
                    )));
                }
                Ok(())
            }
        }
    }

    pub fn label(&self) -> String {
        let mut label = String::with_capacity(self.label_len().unwrap_or(0));
        self.write_label(&mut label);
        label
    }

    pub fn write_label(&self, output: &mut String) {
        match self {
            Self::Atom(value) => output.push_str(value),
            Self::String(value) => {
                write_data_value_label(output, STRING_VALUE_PREFIX, value.as_bytes())
            }
            Self::Bytes(value) => write_data_value_label(output, BYTES_VALUE_PREFIX, value),
            Self::Scalar(value) => {
                let _ = write!(output, "{}{}", value.value(), value.ty().suffix());
            }
            Self::EnumVariant { variant, payload } => {
                output.push_str(variant);
                output.push('(');
                payload.write_label(output);
                output.push(')');
            }
            Self::Record {
                constructor,
                fields,
            } => {
                output.push_str(constructor);
                output.push('{');
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&field.name);
                    output.push(':');
                    field.value.write_label(output);
                }
                output.push('}');
            }
            Self::List(items) => {
                output.push_str("List[");
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    item.write_label(output);
                }
                output.push(']');
            }
            Self::Map(entries) => {
                output.push_str("Map[");
                for (index, entry) in entries.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    entry.key.write_label(output);
                    output.push_str("=>");
                    entry.value.write_label(output);
                }
                output.push(']');
            }
            Self::ProcessRef { type_id, pid } => {
                let _ = write!(output, "type{}#{pid}", type_id.as_u32());
            }
        }
    }

    pub fn label_len(&self) -> Result<usize> {
        match self {
            Self::Atom(value) => Ok(value.len()),
            Self::String(value) => data_value_label_len(STRING_VALUE_PREFIX, value.len()),
            Self::Bytes(value) => data_value_label_len(BYTES_VALUE_PREFIX, value.len()),
            Self::Scalar(value) => {
                checked_add_len(decimal_len_i128(value.value()), value.ty().suffix().len())
            }
            Self::EnumVariant { variant, payload } => {
                checked_add_lens([variant.len(), 1, payload.label_len()?, 1])
            }
            Self::Record {
                constructor,
                fields,
            } => {
                let mut len = checked_add_len(constructor.len(), 2)?;
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        len = checked_add_len(len, 1)?;
                    }
                    len = checked_add_lens([len, field.name.len(), 1, field.value.label_len()?])?;
                }
                Ok(len)
            }
            Self::List(items) => {
                let mut len = "List[]".len();
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        len = checked_add_len(len, 1)?;
                    }
                    len = checked_add_len(len, item.label_len()?)?;
                }
                Ok(len)
            }
            Self::Map(entries) => {
                let mut len = "Map[]".len();
                for (index, entry) in entries.iter().enumerate() {
                    if index > 0 {
                        len = checked_add_len(len, 1)?;
                    }
                    len = checked_add_lens([
                        len,
                        entry.key.label_len()?,
                        2,
                        entry.value.label_len()?,
                    ])?;
                }
                Ok(len)
            }
            Self::ProcessRef { type_id, pid } => checked_add_lens([
                "type".len(),
                decimal_len_u32(type_id.as_u32()),
                1,
                decimal_len_u64(*pid),
            ]),
        }
    }

    pub(crate) fn label_matches(&self, label: &str) -> bool {
        let mut remaining = label;
        self.consume_label(&mut remaining) && remaining.is_empty()
    }

    fn consume_label(&self, input: &mut &str) -> bool {
        match self {
            Self::Atom(value) => consume_literal(input, value),
            Self::String(value) => {
                consume_data_value_label(input, STRING_VALUE_PREFIX, value.as_bytes())
            }
            Self::Bytes(value) => consume_data_value_label(input, BYTES_VALUE_PREFIX, value),
            Self::Scalar(value) => {
                consume_i128(input, value.value()) && consume_literal(input, value.ty().suffix())
            }
            Self::EnumVariant { variant, payload } => {
                consume_literal(input, variant)
                    && consume_literal(input, "(")
                    && payload.consume_label(input)
                    && consume_literal(input, ")")
            }
            Self::Record {
                constructor,
                fields,
            } => {
                if !consume_literal(input, constructor) || !consume_literal(input, "{") {
                    return false;
                }
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 && !consume_literal(input, ",") {
                        return false;
                    }
                    if !consume_literal(input, &field.name)
                        || !consume_literal(input, ":")
                        || !field.value.consume_label(input)
                    {
                        return false;
                    }
                }
                consume_literal(input, "}")
            }
            Self::List(items) => {
                if !consume_literal(input, "List[") {
                    return false;
                }
                for (index, item) in items.iter().enumerate() {
                    if index > 0 && !consume_literal(input, ",") {
                        return false;
                    }
                    if !item.consume_label(input) {
                        return false;
                    }
                }
                consume_literal(input, "]")
            }
            Self::Map(entries) => {
                if !consume_literal(input, "Map[") {
                    return false;
                }
                for (index, entry) in entries.iter().enumerate() {
                    if index > 0 && !consume_literal(input, ",") {
                        return false;
                    }
                    if !entry.key.consume_label(input)
                        || !consume_literal(input, "=>")
                        || !entry.value.consume_label(input)
                    {
                        return false;
                    }
                }
                consume_literal(input, "]")
            }
            Self::ProcessRef { type_id, pid } => {
                consume_literal(input, "type")
                    && consume_u128(input, u128::from(type_id.as_u32()))
                    && consume_literal(input, "#")
                    && consume_u128(input, u128::from(*pid))
            }
        }
    }

    pub(crate) fn validate_generated_label_len(&self, field: &str) -> Result<()> {
        let len = self.label_len()?;
        if len > crate::MAX_FIELD_VALUE_BYTES {
            return Err(Error::new(format!(
                "{field} exceeds maximum length of {} bytes",
                crate::MAX_FIELD_VALUE_BYTES
            )));
        }
        Ok(())
    }

    pub fn contains_process_ref(&self) -> bool {
        match self {
            Self::Atom(_) => false,
            Self::String(_) => false,
            Self::Bytes(_) => false,
            Self::Scalar(_) => false,
            Self::EnumVariant { payload, .. } => payload.contains_process_ref(),
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.contains_process_ref()),
            Self::List(items) => items.iter().any(ArtifactValue::contains_process_ref),
            Self::Map(entries) => entries.iter().any(|entry| {
                entry.key.contains_process_ref() || entry.value.contains_process_ref()
            }),
            Self::ProcessRef { .. } => true,
        }
    }

    pub fn project_enum_payload(&self, variant: &str) -> Result<Self> {
        let Self::EnumVariant {
            variant: actual,
            payload,
        } = self
        else {
            return Err(Error::new(format!(
                "enum payload projection requires an enum value, got {}",
                self.label()
            )));
        };
        if actual != variant {
            return Err(Error::new(format!(
                "enum payload projection expected variant {variant}, found {actual}"
            )));
        }
        Ok((**payload).clone())
    }

    pub fn project_record_field(&self, field: &str) -> Result<Self> {
        let Self::Record { fields, .. } = self else {
            return Err(Error::new(format!(
                "record projection requires a record value, got {}",
                self.label()
            )));
        };
        fields
            .iter()
            .find(|entry| entry.name == field)
            .map(|entry| entry.value.clone())
            .ok_or_else(|| {
                Error::new(format!(
                    "record projection field {field} is not present in {}",
                    self.label()
                ))
            })
    }

    pub fn project_list_element(&self, index: usize, len: usize) -> Result<Self> {
        let Self::List(items) = self else {
            return Err(Error::new(format!(
                "list projection requires a list value, got {}",
                self.label()
            )));
        };
        if items.len() != len {
            return Err(Error::new(format!(
                "list projection expected length {len}, found {} in {}",
                items.len(),
                self.label()
            )));
        }
        items.get(index).cloned().ok_or_else(|| {
            Error::new(format!(
                "list projection index {index} is outside length {len}"
            ))
        })
    }

    pub fn project_list_prefix_element(&self, index: usize, prefix_len: usize) -> Result<Self> {
        if prefix_len == 0 {
            return Err(Error::new(
                "list prefix projection requires at least one prefix element",
            ));
        }
        let Self::List(items) = self else {
            return Err(Error::new(format!(
                "list prefix projection requires a list value, got {}",
                self.label()
            )));
        };
        if items.len() < prefix_len {
            return Err(Error::new(format!(
                "list prefix projection requires at least {prefix_len} prefix elements, found {} in {}",
                items.len(),
                self.label()
            )));
        }
        if index >= prefix_len {
            return Err(Error::new(format!(
                "list prefix projection index {index} is outside prefix length {prefix_len}"
            )));
        }
        Ok(items[index].clone())
    }

    pub fn project_list_rest(&self, prefix_len: usize) -> Result<Self> {
        if prefix_len == 0 {
            return Err(Error::new(
                "list rest projection requires at least one prefix element",
            ));
        }
        let Self::List(items) = self else {
            return Err(Error::new(format!(
                "list rest projection requires a list value, got {}",
                self.label()
            )));
        };
        if items.len() < prefix_len {
            return Err(Error::new(format!(
                "list rest projection requires at least {prefix_len} prefix elements, found {} in {}",
                items.len(),
                self.label()
            )));
        }
        Ok(Self::List(items.iter().skip(prefix_len).cloned().collect()))
    }

    pub fn project_map_value(
        &self,
        key: &ArtifactValue,
        keys: &[ArtifactValue],
        projection: MapProjectionMode,
    ) -> Result<Self> {
        validate_projection_keys("map projection", key, keys)?;
        let Self::Map(entries) = self else {
            return Err(Error::new(format!(
                "map projection requires a map value, got {}",
                self.label()
            )));
        };
        match projection {
            MapProjectionMode::Exact => {
                if entries.len() != keys.len()
                    || !keys
                        .iter()
                        .all(|expected_key| entries.iter().any(|entry| entry.key == *expected_key))
                {
                    return Err(Error::new(format!(
                        "map projection expected exact keys [{}], found [{}]",
                        labels(keys),
                        entry_key_labels(entries)
                    )));
                }
            }
            MapProjectionMode::Subset => {
                for expected_key in keys {
                    if !entries.iter().any(|entry| entry.key == *expected_key) {
                        return Err(Error::new(format!(
                            "map projection expected key {}, found [{}]",
                            expected_key.label(),
                            entry_key_labels(entries)
                        )));
                    }
                }
            }
        }
        entries
            .iter()
            .find(|entry| entry.key == *key)
            .map(|entry| entry.value.clone())
            .ok_or_else(|| {
                Error::new(format!(
                    "map projection key {} is not present in {}",
                    key.label(),
                    self.label()
                ))
            })
    }

    pub fn project_map_rest(&self, excluded_keys: &[ArtifactValue]) -> Result<Self> {
        validate_projection_key_set(
            "map rest projection",
            excluded_keys,
            ProjectionKeySetKind::Excluded,
        )?;
        let Self::Map(entries) = self else {
            return Err(Error::new(format!(
                "map rest projection requires a map value, got {}",
                self.label()
            )));
        };
        for excluded_key in excluded_keys {
            if !entries.iter().any(|entry| entry.key == *excluded_key) {
                return Err(Error::new(format!(
                    "map rest projection expected excluded map key {}, found [{}]",
                    excluded_key.label(),
                    entry_key_labels(entries)
                )));
            }
        }
        Ok(Self::Map(
            entries
                .iter()
                .filter(|entry| excluded_keys.binary_search(&entry.key).is_err())
                .cloned()
                .collect(),
        ))
    }
}

fn entry_key_labels(entries: &[super::model::ArtifactMapEntry]) -> String {
    let capacity = entries
        .iter()
        .enumerate()
        .try_fold(0usize, |len, (index, entry)| {
            let len = if index > 0 {
                checked_add_len(len, 1)?
            } else {
                len
            };
            checked_add_len(len, entry.key.label_len()?)
        })
        .unwrap_or(0);
    let mut output = String::with_capacity(capacity);
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        entry.key.write_label(&mut output);
    }
    output
}

fn validate_primitive_data_len(field: &str, len: usize) -> Result<()> {
    if len > MAX_PRIMITIVE_DATA_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum primitive data length of {MAX_PRIMITIVE_DATA_BYTES} bytes"
        )));
    }
    Ok(())
}

fn write_data_value_label(output: &mut String, prefix: &str, bytes: &[u8]) {
    output.push_str(prefix);
    write_hex_bytes(output, bytes);
    output.push(')');
}

fn write_hex_bytes(output: &mut String, bytes: &[u8]) {
    for byte in bytes {
        output.push(HEX_TABLE[usize::from(byte >> 4)] as char);
        output.push(HEX_TABLE[usize::from(byte & 0x0f)] as char);
    }
}

fn data_value_label_len(prefix: &str, bytes_len: usize) -> Result<usize> {
    let hex_len = bytes_len
        .checked_mul(2)
        .ok_or_else(|| Error::new("artifact value label length overflowed"))?;
    checked_add_lens([prefix.len(), hex_len, 1])
}

fn checked_add_len(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .ok_or_else(|| Error::new("artifact value label length overflowed"))
}

fn checked_add_lens<const N: usize>(parts: [usize; N]) -> Result<usize> {
    let mut total = 0usize;
    for part in parts {
        total = checked_add_len(total, part)?;
    }
    Ok(total)
}

fn consume_literal(input: &mut &str, expected: &str) -> bool {
    let Some(remaining) = input.strip_prefix(expected) else {
        return false;
    };
    *input = remaining;
    true
}

fn consume_data_value_label(input: &mut &str, prefix: &str, bytes: &[u8]) -> bool {
    consume_literal(input, prefix) && consume_hex_bytes(input, bytes) && consume_literal(input, ")")
}

fn consume_hex_bytes(input: &mut &str, expected: &[u8]) -> bool {
    let Some(hex_len) = expected.len().checked_mul(2) else {
        return false;
    };
    if input.len() < hex_len {
        return false;
    }
    let (hex, remaining) = input.split_at(hex_len);
    if !hex
        .as_bytes()
        .chunks_exact(2)
        .zip(expected)
        .all(|(pair, &byte)| {
            hex_nibble(pair[0]) == Some(byte >> 4) && hex_nibble(pair[1]) == Some(byte & 0x0f)
        })
    {
        return false;
    }
    *input = remaining;
    true
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn consume_i128(input: &mut &str, value: i128) -> bool {
    if value < 0 {
        consume_literal(input, "-") && consume_u128(input, value.unsigned_abs())
    } else {
        consume_u128(input, value as u128)
    }
}

fn consume_u128(input: &mut &str, value: u128) -> bool {
    let len = decimal_len_u128(value);
    if input.len() < len {
        return false;
    }
    let (digits, remaining) = input.split_at(len);
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    match digits.parse::<u128>() {
        Ok(parsed) if parsed == value => {
            *input = remaining;
            true
        }
        _ => false,
    }
}

const fn decimal_len_i128(value: i128) -> usize {
    if value < 0 {
        1 + decimal_len_u128(value.unsigned_abs())
    } else {
        decimal_len_u128(value as u128)
    }
}

const fn decimal_len_u32(value: u32) -> usize {
    decimal_len_u64(value as u64)
}

const fn decimal_len_u64(mut value: u64) -> usize {
    let mut len = 1usize;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

const fn decimal_len_u128(mut value: u128) -> usize {
    let mut len = 1usize;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}
