use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, Error, MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES,
    Result,
};

pub(super) fn validate_loaded_artifact_identity(format: &str, schema_version: &str) -> Result<()> {
    validate_loaded_identity_field("loaded artifact format", format)?;
    validate_loaded_identity_field("loaded artifact schema_version", schema_version)?;
    if format != ARTIFACT_FORMAT {
        return Err(Error::new(format!(
            "loaded artifact format {format:?}; expected {ARTIFACT_FORMAT:?}"
        )));
    }
    if schema_version != ARTIFACT_SCHEMA_VERSION {
        return Err(Error::new(format!(
            "loaded artifact schema_version {schema_version:?}; expected {ARTIFACT_SCHEMA_VERSION:?}"
        )));
    }
    Ok(())
}

fn validate_loaded_identity_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::new(format!(
            "{field} must be non-empty and contain no control characters, got {value:?}"
        )));
    }
    Ok(())
}

pub(super) fn validate_loaded_output_text(output: &str) -> Result<()> {
    if output.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "loaded output exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if output.is_empty() || output.chars().any(char::is_control) {
        return Err(Error::new(
            "loaded output must be non-empty and contain no control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_loaded_ident_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if is_artifact_ident(value) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} must be an identifier, got {value:?}"
        )))
    }
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
