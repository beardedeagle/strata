use mantle_artifact::MAX_FIELD_VALUE_BYTES;

use super::super::super::composition_artifact::codec::JsonObject;
use super::super::super::diagnostic::{Error, Result};
use super::{FNV1A64_FINGERPRINT_HEX_LEN, SpawnKindFact};

pub(super) fn validate_metadata_string(object: &JsonObject<'_>, field: &str) -> Result<()> {
    let value = object.required_string(field)?;
    if value.is_empty() || value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "field {field:?} has invalid metadata length"
        )));
    }
    Ok(())
}

pub(super) fn validate_source_fingerprint(object: &JsonObject<'_>) -> Result<()> {
    let value = object.required_string("source_fingerprint")?;
    if value.len() != FNV1A64_FINGERPRINT_HEX_LEN
        || !value
            .as_bytes()
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(Error::new(
            "field \"source_fingerprint\" must be a 16-character lowercase hexadecimal FNV-1a hash",
        ));
    }
    Ok(())
}

pub(super) fn validate_spawn_kind(object: &JsonObject<'_>, field: &str) -> Result<SpawnKindFact> {
    let value = object.required_string(field)?;
    match value.as_ref() {
        "dynamic_local" => Ok(SpawnKindFact::DynamicLocal),
        "lexical_supervisor_child" => Ok(SpawnKindFact::LexicalSupervisorChild),
        other => Err(Error::new(format!("unsupported spawn kind {other:?}"))),
    }
}

pub(super) fn validate_empty_array(object: &JsonObject<'_>, field: &str) -> Result<()> {
    if object.required_array(field)?.count_values()? == 0 {
        Ok(())
    } else {
        Err(Error::new(format!(
            "field {field:?} is not implemented and must be empty"
        )))
    }
}

pub(super) fn require_schema_version_eq(
    object: &JsonObject<'_>,
    field: &str,
    expected: u32,
) -> Result<()> {
    let actual = object.required_u32(field)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Error::new(format!(
            "field {field:?} must be schema version {expected}, got {actual}"
        )))
    }
}

pub(super) fn validate_count_field(
    object: &JsonObject<'_>,
    field: &str,
    min: usize,
    max: usize,
) -> Result<usize> {
    let value = object.required_u32(field)?;
    let count = usize::try_from(value).map_err(|_| {
        Error::new(format!(
            "field {field:?} count {value} exceeds supported usize range"
        ))
    })?;
    validate_count(field, count, min, max)?;
    Ok(count)
}

pub(super) fn validate_indexed_id(
    object: &JsonObject<'_>,
    field: &str,
    expected_index: usize,
) -> Result<()> {
    let actual = object.required_u32(field)?;
    if usize::try_from(actual).ok() == Some(expected_index) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} {actual} at array index {expected_index} is not canonical"
        )))
    }
}

pub(super) fn validate_existing_id(
    object: &JsonObject<'_>,
    field: &str,
    count: usize,
    noun: &str,
) -> Result<u32> {
    let id = object.required_u32(field)?;
    validate_existing_raw_id(id, count, noun)?;
    Ok(id)
}

pub(super) fn validate_existing_raw_id(id: u32, count: usize, noun: &str) -> Result<()> {
    if usize::try_from(id).is_ok_and(|index| index < count) {
        Ok(())
    } else {
        Err(Error::new(format!("references unknown {noun} id {id}")))
    }
}

pub(super) fn validate_count(name: &str, count: usize, min: usize, max: usize) -> Result<()> {
    if count < min || count > max {
        return Err(Error::new(format!(
            "{name} {count} is outside supported range {min}..={max}"
        )));
    }
    Ok(())
}

pub(super) fn validate_exact_count(name: &str, count: usize, expected: usize) -> Result<()> {
    if count == expected {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{name} {count} does not match declared count {expected}"
        )))
    }
}
