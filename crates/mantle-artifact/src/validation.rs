use std::collections::BTreeSet;

use crate::{
    ARTIFACT_MAGIC, ArtifactAction, ArtifactMessageVariant, ArtifactSendTarget, ArtifactStateValue,
    ArtifactTypeKind, ArtifactValueTemplate, Error, MAX_ARTIFACT_BYTES, MAX_FIELD_VALUE_BYTES,
    MAX_IDENTIFIER_BYTES, MantleArtifact, NextState, Result, TypeId,
};

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
    let mut seen = BTreeSet::new();
    for value in values {
        validate_message_label(&value.label)?;
        if !seen.insert(value.label.as_str()) {
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
    let mut seen = BTreeSet::new();
    for value in values {
        validate_value_label("state value", &value.value)?;
        validate_state_value_label(&value.label)?;
        if !seen.insert((value.ty, value.value.as_str())) {
            return Err(Error::new(format!(
                "duplicate state value {} with type id {}",
                value.value,
                value.ty.as_u32()
            )));
        }
    }
    Ok(())
}

/// Validates display metadata labels used for artifact state values.
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

pub(crate) fn validate_encoded_artifact_size(artifact: &MantleArtifact) -> Result<()> {
    let mut encoded_len = 0usize;
    add_encoded_bytes(&mut encoded_len, ARTIFACT_MAGIC.len() + 1)?;
    add_field_bytes(&mut encoded_len, "format", &artifact.format)?;
    add_field_bytes(&mut encoded_len, "schema_version", &artifact.schema_version)?;
    add_field_bytes(
        &mut encoded_len,
        "source_language",
        &artifact.source_language,
    )?;
    add_field_bytes(&mut encoded_len, "module", &artifact.module)?;
    add_field_bytes(
        &mut encoded_len,
        "entry_process",
        &artifact.entry_process.as_u32().to_string(),
    )?;
    add_field_bytes(
        &mut encoded_len,
        "entry_message",
        &artifact.entry_message.as_u32().to_string(),
    )?;
    add_field_bytes(
        &mut encoded_len,
        "type_count",
        &artifact.types.len().to_string(),
    )?;
    for (type_index, ty) in artifact.types.iter().enumerate() {
        let prefix = format!("type.{type_index}");
        add_field_bytes(&mut encoded_len, &format!("{prefix}.label"), &ty.label)?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.kind"),
            ty.kind.as_str(),
        )?;
        if let ArtifactTypeKind::ProcessRef { target } = ty.kind {
            add_field_bytes(
                &mut encoded_len,
                &format!("{prefix}.target_process"),
                &target.as_u32().to_string(),
            )?;
        }
    }
    add_field_bytes(
        &mut encoded_len,
        "output_count",
        &artifact.outputs.len().to_string(),
    )?;
    for (output_index, output) in artifact.outputs.iter().enumerate() {
        add_field_bytes(&mut encoded_len, &format!("output.{output_index}"), output)?;
    }
    add_field_bytes(
        &mut encoded_len,
        "process_count",
        &artifact.processes.len().to_string(),
    )?;

    for (process_index, process) in artifact.processes.iter().enumerate() {
        let prefix = format!("process.{process_index}");
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.debug_name"),
            &process.debug_name,
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.state_type_id"),
            &type_id_string(process.state_type),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.state_value_count"),
            &process.state_values.len().to_string(),
        )?;
        for (value_index, value) in process.state_values.iter().enumerate() {
            let value_prefix = format!("{prefix}.state_value.{value_index}");
            add_field_bytes(
                &mut encoded_len,
                &format!("{value_prefix}.type_id"),
                &type_id_string(value.ty),
            )?;
            add_field_bytes(
                &mut encoded_len,
                &format!("{value_prefix}.value"),
                &value.value,
            )?;
            add_field_bytes(
                &mut encoded_len,
                &format!("{value_prefix}.label"),
                &value.label,
            )?;
        }
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.message_type_id"),
            &type_id_string(process.message_type),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.message_count"),
            &process.message_variants.len().to_string(),
        )?;
        for (message_index, message) in process.message_variants.iter().enumerate() {
            add_field_bytes(
                &mut encoded_len,
                &format!("{prefix}.message.{message_index}"),
                &message.label,
            )?;
            if let Some(payload_type) = message.payload_type {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{prefix}.message.{message_index}.payload_type_id"),
                    &type_id_string(payload_type),
                )?;
            }
        }
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.process_ref_count"),
            &process.process_refs.len().to_string(),
        )?;
        for (process_ref_index, process_ref) in process.process_refs.iter().enumerate() {
            let process_ref_prefix = format!("{prefix}.process_ref.{process_ref_index}");
            add_field_bytes(
                &mut encoded_len,
                &format!("{process_ref_prefix}.debug_name"),
                &process_ref.debug_name,
            )?;
            add_field_bytes(
                &mut encoded_len,
                &format!("{process_ref_prefix}.target_process"),
                &process_ref.target.as_u32().to_string(),
            )?;
        }
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.mailbox_bound"),
            &process.mailbox_bound.to_string(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.init_state"),
            &process.init_state.as_u32().to_string(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.transition_count"),
            &process.transitions.len().to_string(),
        )?;
        for (transition_index, transition) in process.transitions.iter().enumerate() {
            let transition_prefix = format!("{prefix}.transition.{transition_index}");
            add_field_bytes(
                &mut encoded_len,
                &format!("{transition_prefix}.message"),
                &transition.message.as_u32().to_string(),
            )?;
            add_field_bytes(
                &mut encoded_len,
                &format!("{transition_prefix}.step_result"),
                transition.step_result.as_str(),
            )?;
            add_field_bytes(
                &mut encoded_len,
                &format!("{transition_prefix}.next_state"),
                transition.next_state.kind_str(),
            )?;
            if let NextState::Value(state) = &transition.next_state {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{transition_prefix}.next_state_value"),
                    &state.as_u32().to_string(),
                )?;
            }
            if let NextState::Template(template) = &transition.next_state {
                add_value_template_bytes(
                    &mut encoded_len,
                    &format!("{transition_prefix}.next_state_template"),
                    template,
                )?;
            }
            add_field_bytes(
                &mut encoded_len,
                &format!("{transition_prefix}.effect_count"),
                &transition.effects.len().to_string(),
            )?;
            for (effect_index, effect) in transition.effects.iter().enumerate() {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{transition_prefix}.effect.{effect_index}"),
                    effect.as_str(),
                )?;
            }
            add_field_bytes(
                &mut encoded_len,
                &format!("{transition_prefix}.action_count"),
                &transition.actions.len().to_string(),
            )?;
            for (action_index, action) in transition.actions.iter().enumerate() {
                let action_prefix = format!("{transition_prefix}.action.{action_index}");
                match action {
                    ArtifactAction::Emit { output } => {
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.kind"),
                            "emit",
                        )?;
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.output"),
                            &output.as_u32().to_string(),
                        )?;
                    }
                    ArtifactAction::Spawn {
                        target,
                        process_ref,
                    } => {
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.kind"),
                            "spawn",
                        )?;
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.target_process"),
                            &target.as_u32().to_string(),
                        )?;
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.process_ref"),
                            &process_ref.as_u32().to_string(),
                        )?;
                    }
                    ArtifactAction::Send {
                        target,
                        message,
                        payload,
                    } => {
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.kind"),
                            "send",
                        )?;
                        add_send_target_bytes(&mut encoded_len, &action_prefix, target)?;
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.message"),
                            &message.as_u32().to_string(),
                        )?;
                        add_field_bytes(
                            &mut encoded_len,
                            &format!("{action_prefix}.payload"),
                            if payload.is_some() {
                                "template"
                            } else {
                                "none"
                            },
                        )?;
                        if let Some(payload) = payload {
                            add_value_template_bytes(
                                &mut encoded_len,
                                &format!("{action_prefix}.payload_template"),
                                payload,
                            )?;
                        }
                    }
                }
            }
        }
    }

    add_field_bytes(
        &mut encoded_len,
        "source_hash_fnv1a64",
        &artifact.source_hash_fnv1a64,
    )?;
    Ok(())
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

fn add_field_bytes(total: &mut usize, key: &str, value: &str) -> Result<()> {
    add_encoded_bytes(total, key.len())?;
    add_encoded_bytes(total, 1)?;
    add_encoded_bytes(total, value.len())?;
    add_encoded_bytes(total, 1)
}

fn type_id_string(ty: TypeId) -> String {
    ty.as_u32().to_string()
}

fn add_value_template_bytes(
    total: &mut usize,
    prefix: &str,
    template: &ArtifactValueTemplate,
) -> Result<()> {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "literal")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.value"), value)?;
        }
        ArtifactValueTemplate::ReceivedPayload { ty } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "received_payload")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
        }
        ArtifactValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "process_ref")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.target_process"),
                &target_process.as_u32().to_string(),
            )?;
            add_field_bytes(
                total,
                &format!("{prefix}.process_ref"),
                &process_ref.as_u32().to_string(),
            )?;
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "record")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.field_count"),
                &fields.len().to_string(),
            )?;
            for (field_index, field) in fields.iter().enumerate() {
                let field_prefix = format!("{prefix}.field.{field_index}");
                add_field_bytes(total, &format!("{field_prefix}.name"), &field.name)?;
                add_value_template_bytes(total, &format!("{field_prefix}.value"), &field.value)?;
            }
        }
    }
    Ok(())
}

fn add_send_target_bytes(
    total: &mut usize,
    action_prefix: &str,
    target: &ArtifactSendTarget,
) -> Result<()> {
    match target {
        ArtifactSendTarget::ProcessRef(process_ref) => {
            add_field_bytes(total, &format!("{action_prefix}.target"), "process_ref")?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.target_process_ref"),
                &process_ref.as_u32().to_string(),
            )?;
        }
        ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
            add_field_bytes(
                total,
                &format!("{action_prefix}.target"),
                "received_payload",
            )?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.target_payload_type_id"),
                &type_id_string(*ty),
            )?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.target_process"),
                &target_process.as_u32().to_string(),
            )?;
        }
    }
    Ok(())
}

fn add_encoded_bytes(total: &mut usize, count: usize) -> Result<()> {
    *total = total
        .checked_add(count)
        .ok_or_else(|| Error::new("encoded artifact size overflowed"))?;
    if *total > MAX_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "encoded artifact exceeds maximum size of {MAX_ARTIFACT_BYTES} bytes"
        )));
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
