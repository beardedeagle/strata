use crate::{
    ARTIFACT_MAGIC, ArtifactAction, ArtifactSendTarget, ArtifactTypeKind, ArtifactValueShape,
    Error, MAX_ARTIFACT_BYTES, MantleArtifact, NextState, Result, TypeId,
};

mod templates;

use templates::add_value_template_bytes;

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
        match ty.kind {
            ArtifactTypeKind::Value => {
                let shape = ty.shape.as_ref().ok_or_else(|| {
                    Error::new(format!("{prefix} value type must declare a value shape"))
                })?;
                add_type_shape_bytes(&mut encoded_len, &prefix, shape)?;
            }
            ArtifactTypeKind::ProcessRef { target } => {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{prefix}.target_process"),
                    &target.as_u32().to_string(),
                )?;
            }
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
                &value.value.label(),
            )?;
            add_field_bytes(
                &mut encoded_len,
                &format!("{value_prefix}.label"),
                &value.label,
            )?;
            if let Some(payload) = &value.payload {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{value_prefix}.payload_type_id"),
                    &type_id_string(payload.ty),
                )?;
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{value_prefix}.payload_value"),
                    &payload.value.label(),
                )?;
            }
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
            if let Some(current_state) = transition.current_state {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{transition_prefix}.current_state"),
                    &current_state.as_u32().to_string(),
                )?;
            }
            if let Some(payload_guard) = &transition.payload_guard {
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{transition_prefix}.payload_guard_type_id"),
                    &payload_guard.ty.as_u32().to_string(),
                )?;
                add_field_bytes(
                    &mut encoded_len,
                    &format!("{transition_prefix}.payload_guard_value"),
                    &payload_guard.value.label(),
                )?;
            }
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
            add_next_state_bytes(&mut encoded_len, &transition_prefix, &transition.next_state)?;
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
                add_action_bytes(&mut encoded_len, &action_prefix, action)?;
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

fn add_type_shape_bytes(total: &mut usize, prefix: &str, shape: &ArtifactValueShape) -> Result<()> {
    match shape {
        ArtifactValueShape::Atom => add_field_bytes(total, &format!("{prefix}.shape"), "atom"),
        ArtifactValueShape::Scalar { scalar } => {
            add_field_bytes(total, &format!("{prefix}.shape"), "scalar")?;
            add_field_bytes(
                total,
                &format!("{prefix}.scalar_type"),
                scalar.artifact_name(),
            )
        }
        ArtifactValueShape::Record { fields } => {
            add_field_bytes(total, &format!("{prefix}.shape"), "record")?;
            add_field_bytes(
                total,
                &format!("{prefix}.field_count"),
                &fields.len().to_string(),
            )?;
            for (field_index, field) in fields.iter().enumerate() {
                add_field_bytes(
                    total,
                    &format!("{prefix}.field.{field_index}.name"),
                    &field.name,
                )?;
                add_field_bytes(
                    total,
                    &format!("{prefix}.field.{field_index}.type_id"),
                    &type_id_string(field.ty),
                )?;
            }
            Ok(())
        }
        ArtifactValueShape::Enum { variants } => {
            add_field_bytes(total, &format!("{prefix}.shape"), "enum")?;
            add_field_bytes(
                total,
                &format!("{prefix}.enum_variant_count"),
                &variants.len().to_string(),
            )?;
            for (variant_index, variant) in variants.iter().enumerate() {
                add_field_bytes(
                    total,
                    &format!("{prefix}.enum_variant.{variant_index}"),
                    &variant.label,
                )?;
                if let Some(payload_type) = variant.payload_type {
                    add_field_bytes(
                        total,
                        &format!("{prefix}.enum_variant.{variant_index}.payload_type_id"),
                        &type_id_string(payload_type),
                    )?;
                }
            }
            Ok(())
        }
        ArtifactValueShape::List { element, capacity } => {
            add_field_bytes(total, &format!("{prefix}.shape"), "list")?;
            add_field_bytes(
                total,
                &format!("{prefix}.element_type_id"),
                &type_id_string(*element),
            )?;
            add_field_bytes(total, &format!("{prefix}.capacity"), &capacity.to_string())
        }
        ArtifactValueShape::Map {
            key,
            value,
            capacity,
        } => {
            add_field_bytes(total, &format!("{prefix}.shape"), "map")?;
            add_field_bytes(
                total,
                &format!("{prefix}.key_type_id"),
                &type_id_string(*key),
            )?;
            add_field_bytes(
                total,
                &format!("{prefix}.value_type_id"),
                &type_id_string(*value),
            )?;
            add_field_bytes(total, &format!("{prefix}.capacity"), &capacity.to_string())
        }
    }
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

fn add_next_state_bytes(total: &mut usize, prefix: &str, next_state: &NextState) -> Result<()> {
    match next_state {
        NextState::Current => Ok(()),
        NextState::Value(state) => add_field_bytes(
            total,
            &format!("{prefix}.next_state_value"),
            &state.as_u32().to_string(),
        ),
        NextState::Template(template) => {
            add_value_template_bytes(total, &format!("{prefix}.next_state_template"), template)
        }
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            add_value_template_bytes(total, &format!("{prefix}.next_state_condition"), condition)?;
            add_field_bytes(
                total,
                &format!("{prefix}.next_state_then.next_state"),
                then_state.kind_str(),
            )?;
            add_next_state_bytes(total, &format!("{prefix}.next_state_then"), then_state)?;
            add_field_bytes(
                total,
                &format!("{prefix}.next_state_else.next_state"),
                else_state.kind_str(),
            )?;
            add_next_state_bytes(total, &format!("{prefix}.next_state_else"), else_state)
        }
    }
}

fn add_action_bytes(total: &mut usize, action_prefix: &str, action: &ArtifactAction) -> Result<()> {
    match action {
        ArtifactAction::Emit { output } => {
            add_field_bytes(total, &format!("{action_prefix}.kind"), "emit")?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.output"),
                &output.as_u32().to_string(),
            )
        }
        ArtifactAction::Spawn {
            target,
            process_ref,
        } => {
            add_field_bytes(total, &format!("{action_prefix}.kind"), "spawn")?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.target_process"),
                &target.as_u32().to_string(),
            )?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.process_ref"),
                &process_ref.as_u32().to_string(),
            )
        }
        ArtifactAction::Send {
            target,
            message,
            payload,
        } => {
            add_field_bytes(total, &format!("{action_prefix}.kind"), "send")?;
            add_send_target_bytes(total, action_prefix, target)?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.message"),
                &message.as_u32().to_string(),
            )?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.payload"),
                if payload.is_some() {
                    "template"
                } else {
                    "none"
                },
            )?;
            if let Some(payload) = payload {
                add_value_template_bytes(
                    total,
                    &format!("{action_prefix}.payload_template"),
                    payload,
                )?;
            }
            Ok(())
        }
        ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            add_field_bytes(total, &format!("{action_prefix}.kind"), "if_else")?;
            add_value_template_bytes(total, &format!("{action_prefix}.condition"), condition)?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.then_action_count"),
                &then_actions.len().to_string(),
            )?;
            for (action_index, action) in then_actions.iter().enumerate() {
                add_action_bytes(
                    total,
                    &format!("{action_prefix}.then_action.{action_index}"),
                    action,
                )?;
            }
            add_field_bytes(
                total,
                &format!("{action_prefix}.else_action_count"),
                &else_actions.len().to_string(),
            )?;
            for (action_index, action) in else_actions.iter().enumerate() {
                add_action_bytes(
                    total,
                    &format!("{action_prefix}.else_action.{action_index}"),
                    action,
                )?;
            }
            Ok(())
        }
        ArtifactAction::ForEach {
            element,
            collection,
            max_items,
            body,
        } => {
            add_field_bytes(total, &format!("{action_prefix}.kind"), "for_each")?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.loop_element"),
                &element.id.as_u32().to_string(),
            )?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.element_type_id"),
                &type_id_string(element.ty),
            )?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.max_items"),
                &max_items.to_string(),
            )?;
            add_value_template_bytes(total, &format!("{action_prefix}.collection"), collection)?;
            add_field_bytes(
                total,
                &format!("{action_prefix}.body_action_count"),
                &body.len().to_string(),
            )?;
            for (action_index, action) in body.iter().enumerate() {
                add_action_bytes(
                    total,
                    &format!("{action_prefix}.body_action.{action_index}"),
                    action,
                )?;
            }
            Ok(())
        }
    }
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
