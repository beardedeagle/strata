use crate::{
    ArtifactMessageVariant, ArtifactStateValue, ArtifactValue, Error, MAX_FIELD_VALUE_BYTES,
    MAX_IDENTIFIER_BYTES, Result,
};

mod encoded_size;

pub(crate) use encoded_size::{encoded_artifact_len, validate_encoded_artifact_size};

pub(crate) fn validate_ident_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "artifact field {field} exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if is_artifact_ident(value) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "artifact field {field} must be an identifier, got {value:?}"
        )))
    }
}

pub(crate) fn validate_unique_message_variant_list(
    values: &[ArtifactMessageVariant],
) -> Result<()> {
    if values.is_empty() {
        return Err(Error::new("message label list must not be empty"));
    }
    for (index, value) in values.iter().enumerate() {
        validate_message_label(&value.label)?;
        if values[..index]
            .iter()
            .any(|previous| previous.label == value.label)
        {
            return Err(Error::new(format!(
                "duplicate message label {}",
                value.label
            )));
        }
    }
    Ok(())
}

pub fn validate_message_label(value: &str) -> Result<()> {
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "message label exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::new(
            "message labels must be non-empty and contain no control characters",
        ));
    }
    Ok(())
}

pub(crate) fn validate_unique_state_value_list(values: &[ArtifactStateValue]) -> Result<()> {
    if values.is_empty() {
        return Err(Error::new("state value list must not be empty"));
    }
    for (index, value) in values.iter().enumerate() {
        value.value.validate_without_process_ref("state value")?;
        validate_state_value_identity_label(&value.value, &value.label)?;
        if values[..index]
            .iter()
            .any(|previous| previous.ty == value.ty && previous.value == value.value)
        {
            return Err(Error::new(format!(
                "duplicate state value {} with type id {}",
                value.value.label(),
                value.ty.as_u32()
            )));
        }
    }
    Ok(())
}

/// Validates metadata labels used for artifact state values.
pub fn validate_state_value_label(value: &str) -> Result<()> {
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "state value exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::new(
            "state values must be non-empty and contain no control characters",
        ));
    }
    Ok(())
}

pub fn validate_state_value_identity_label(value: &ArtifactValue, label: &str) -> Result<()> {
    validate_state_value_label(label)?;
    if !value.label_matches(label) {
        let expected_label = value.label();
        return Err(Error::new(format!(
            "state value label {label} does not match ordered value label {expected_label}"
        )));
    }
    Ok(())
}

pub(crate) fn validate_output_text(output: &str) -> Result<()> {
    if output.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "emitted output exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if output.is_empty() || output.chars().any(char::is_control) {
        return Err(Error::new(
            "emitted outputs must be non-empty and contain no control characters",
        ));
    }
    Ok(())
}

pub(crate) fn validate_value_label(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::new(format!(
            "{field} must be non-empty and contain no control characters"
        )));
    }
    Ok(())
}

pub fn validate_payload_value_label(value: &str) -> Result<()> {
    validate_value_label("payload value", value)
}

pub(crate) fn validate_count(field: &str, value: usize, min: usize, max: usize) -> Result<()> {
    if value < min {
        if min == 1 {
            return Err(Error::new(format!("{field} must be greater than zero")));
        }
        return Err(Error::new(format!("{field} must be at least {min}")));
    }
    if value > max {
        return Err(Error::new(format!("{field} must be no greater than {max}")));
    }
    Ok(())
}

pub(crate) fn validate_source_hash(value: &str) -> Result<()> {
    if value.len() != 16 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(Error::new(
            "source_hash_fnv1a64 must be 16 hexadecimal digits",
        ));
    }
    Ok(())
}

fn is_artifact_ident(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
