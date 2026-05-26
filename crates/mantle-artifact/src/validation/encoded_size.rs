use crate::{
    ARTIFACT_MAGIC, ArtifactAction, ArtifactSendTarget, ArtifactTypeKind, ArtifactValueShape,
    Error, MAX_ARTIFACT_BYTES, MantleArtifact, NextState, Result,
};

mod templates;

use templates::add_value_template_bytes;

#[derive(Clone, Copy)]
pub(super) struct KeyLen(usize);

impl KeyLen {
    const fn new(len: usize) -> Self {
        Self(len)
    }

    fn root_indexed(segment: &str, index: usize) -> Self {
        Self(segment.len() + 1 + decimal_len_usize(index))
    }

    fn child(self, segment: &str) -> Self {
        Self(self.0 + 1 + segment.len())
    }

    fn indexed_child(self, segment: &str, index: usize) -> Self {
        Self(self.0 + 1 + segment.len() + 1 + decimal_len_usize(index))
    }
}

pub(crate) fn validate_encoded_artifact_size(artifact: &MantleArtifact) -> Result<()> {
    let mut encoded_len = 0usize;
    add_encoded_bytes(&mut encoded_len, ARTIFACT_MAGIC.len() + 1)?;
    add_field_bytes(
        &mut encoded_len,
        KeyLen::new("format".len()),
        &artifact.format,
    )?;
    add_field_bytes(
        &mut encoded_len,
        KeyLen::new("schema_version".len()),
        &artifact.schema_version,
    )?;
    add_field_bytes(
        &mut encoded_len,
        KeyLen::new("source_language".len()),
        &artifact.source_language,
    )?;
    add_field_bytes(
        &mut encoded_len,
        KeyLen::new("module".len()),
        &artifact.module,
    )?;
    add_field_u32(
        &mut encoded_len,
        KeyLen::new("entry_process".len()),
        artifact.entry_process.as_u32(),
    )?;
    add_field_u32(
        &mut encoded_len,
        KeyLen::new("entry_message".len()),
        artifact.entry_message.as_u32(),
    )?;
    add_field_usize(
        &mut encoded_len,
        KeyLen::new("type_count".len()),
        artifact.types.len(),
    )?;
    for (type_index, ty) in artifact.types.iter().enumerate() {
        let prefix = KeyLen::root_indexed("type", type_index);
        add_field_bytes(&mut encoded_len, prefix.child("label"), &ty.label)?;
        add_field_bytes(&mut encoded_len, prefix.child("kind"), ty.kind.as_str())?;
        match ty.kind {
            ArtifactTypeKind::Value => {
                let shape = ty.shape.as_ref().ok_or_else(|| {
                    Error::new(format!(
                        "type.{type_index} value type must declare a value shape"
                    ))
                })?;
                add_type_shape_bytes(&mut encoded_len, &prefix, shape)?;
            }
            ArtifactTypeKind::ProcessRef { target } => {
                add_field_u32(
                    &mut encoded_len,
                    prefix.child("target_process"),
                    target.as_u32(),
                )?;
            }
        }
    }
    add_field_usize(
        &mut encoded_len,
        KeyLen::new("output_count".len()),
        artifact.outputs.len(),
    )?;
    for (output_index, output) in artifact.outputs.iter().enumerate() {
        add_field_bytes(
            &mut encoded_len,
            KeyLen::root_indexed("output", output_index),
            output,
        )?;
    }
    add_field_usize(
        &mut encoded_len,
        KeyLen::new("process_count".len()),
        artifact.processes.len(),
    )?;

    for (process_index, process) in artifact.processes.iter().enumerate() {
        let prefix = KeyLen::root_indexed("process", process_index);
        add_field_bytes(
            &mut encoded_len,
            prefix.child("debug_name"),
            &process.debug_name,
        )?;
        add_field_u32(
            &mut encoded_len,
            prefix.child("state_type_id"),
            process.state_type.as_u32(),
        )?;
        add_field_usize(
            &mut encoded_len,
            prefix.child("state_value_count"),
            process.state_values.len(),
        )?;
        for (value_index, value) in process.state_values.iter().enumerate() {
            let value_prefix = prefix.indexed_child("state_value", value_index);
            add_field_u32(
                &mut encoded_len,
                value_prefix.child("type_id"),
                value.ty.as_u32(),
            )?;
            add_field_value_label_len(&mut encoded_len, value_prefix.child("value"), &value.value)?;
            add_field_bytes(&mut encoded_len, value_prefix.child("label"), &value.label)?;
            if let Some(payload) = &value.payload {
                add_field_u32(
                    &mut encoded_len,
                    value_prefix.child("payload_type_id"),
                    payload.ty.as_u32(),
                )?;
                add_field_value_label_len(
                    &mut encoded_len,
                    value_prefix.child("payload_value"),
                    &payload.value,
                )?;
            }
        }
        add_field_u32(
            &mut encoded_len,
            prefix.child("message_type_id"),
            process.message_type.as_u32(),
        )?;
        add_field_usize(
            &mut encoded_len,
            prefix.child("message_count"),
            process.message_variants.len(),
        )?;
        for (message_index, message) in process.message_variants.iter().enumerate() {
            let message_prefix = prefix.indexed_child("message", message_index);
            add_field_bytes(&mut encoded_len, message_prefix, &message.label)?;
            if let Some(payload_type) = message.payload_type {
                add_field_u32(
                    &mut encoded_len,
                    message_prefix.child("payload_type_id"),
                    payload_type.as_u32(),
                )?;
            }
        }
        add_field_usize(
            &mut encoded_len,
            prefix.child("process_ref_count"),
            process.process_refs.len(),
        )?;
        for (process_ref_index, process_ref) in process.process_refs.iter().enumerate() {
            let process_ref_prefix = prefix.indexed_child("process_ref", process_ref_index);
            add_field_bytes(
                &mut encoded_len,
                process_ref_prefix.child("debug_name"),
                &process_ref.debug_name,
            )?;
            add_field_u32(
                &mut encoded_len,
                process_ref_prefix.child("target_process"),
                process_ref.target.as_u32(),
            )?;
        }
        add_field_usize(
            &mut encoded_len,
            prefix.child("mailbox_bound"),
            process.mailbox_bound,
        )?;
        add_field_u32(
            &mut encoded_len,
            prefix.child("init_state"),
            process.init_state.as_u32(),
        )?;
        add_field_usize(
            &mut encoded_len,
            prefix.child("transition_count"),
            process.transitions.len(),
        )?;
        for (transition_index, transition) in process.transitions.iter().enumerate() {
            let transition_prefix = prefix.indexed_child("transition", transition_index);
            add_field_u32(
                &mut encoded_len,
                transition_prefix.child("message"),
                transition.message.as_u32(),
            )?;
            if let Some(current_state) = transition.current_state {
                add_field_u32(
                    &mut encoded_len,
                    transition_prefix.child("current_state"),
                    current_state.as_u32(),
                )?;
            }
            if let Some(payload_guard) = &transition.payload_guard {
                add_field_u32(
                    &mut encoded_len,
                    transition_prefix.child("payload_guard_type_id"),
                    payload_guard.ty.as_u32(),
                )?;
                add_field_value_label_len(
                    &mut encoded_len,
                    transition_prefix.child("payload_guard_value"),
                    &payload_guard.value,
                )?;
            }
            add_field_bytes(
                &mut encoded_len,
                transition_prefix.child("step_result"),
                transition.step_result.as_str(),
            )?;
            add_field_bytes(
                &mut encoded_len,
                transition_prefix.child("next_state"),
                transition.next_state.kind_str(),
            )?;
            add_next_state_bytes(&mut encoded_len, &transition_prefix, &transition.next_state)?;
            add_field_usize(
                &mut encoded_len,
                transition_prefix.child("effect_count"),
                transition.effects.len(),
            )?;
            for (effect_index, effect) in transition.effects.iter().enumerate() {
                add_field_bytes(
                    &mut encoded_len,
                    transition_prefix.indexed_child("effect", effect_index),
                    effect.as_str(),
                )?;
            }
            add_field_usize(
                &mut encoded_len,
                transition_prefix.child("action_count"),
                transition.actions.len(),
            )?;
            for (action_index, action) in transition.actions.iter().enumerate() {
                let action_prefix = transition_prefix.indexed_child("action", action_index);
                add_action_bytes(&mut encoded_len, &action_prefix, action)?;
            }
        }
    }

    add_field_bytes(
        &mut encoded_len,
        KeyLen::new("source_hash_fnv1a64".len()),
        &artifact.source_hash_fnv1a64,
    )?;
    Ok(())
}

fn add_type_shape_bytes(
    total: &mut usize,
    prefix: &KeyLen,
    shape: &ArtifactValueShape,
) -> Result<()> {
    match shape {
        ArtifactValueShape::Atom => add_field_bytes(total, prefix.child("shape"), "atom"),
        ArtifactValueShape::Scalar { scalar } => {
            add_field_bytes(total, prefix.child("shape"), "scalar")?;
            add_field_bytes(total, prefix.child("scalar_type"), scalar.artifact_name())
        }
        ArtifactValueShape::Record { fields } => {
            add_field_bytes(total, prefix.child("shape"), "record")?;
            add_field_usize(total, prefix.child("field_count"), fields.len())?;
            for (field_index, field) in fields.iter().enumerate() {
                let field_prefix = prefix.indexed_child("field", field_index);
                add_field_bytes(total, field_prefix.child("name"), &field.name)?;
                add_field_u32(total, field_prefix.child("type_id"), field.ty.as_u32())?;
            }
            Ok(())
        }
        ArtifactValueShape::Enum { variants } => {
            add_field_bytes(total, prefix.child("shape"), "enum")?;
            add_field_usize(total, prefix.child("enum_variant_count"), variants.len())?;
            for (variant_index, variant) in variants.iter().enumerate() {
                let variant_prefix = prefix.indexed_child("enum_variant", variant_index);
                add_field_bytes(total, variant_prefix, &variant.label)?;
                if let Some(payload_type) = variant.payload_type {
                    add_field_u32(
                        total,
                        variant_prefix.child("payload_type_id"),
                        payload_type.as_u32(),
                    )?;
                }
            }
            Ok(())
        }
        ArtifactValueShape::List { element, capacity } => {
            add_field_bytes(total, prefix.child("shape"), "list")?;
            add_field_u32(total, prefix.child("element_type_id"), element.as_u32())?;
            add_field_usize(total, prefix.child("capacity"), *capacity)
        }
        ArtifactValueShape::Map {
            key,
            value,
            capacity,
        } => {
            add_field_bytes(total, prefix.child("shape"), "map")?;
            add_field_u32(total, prefix.child("key_type_id"), key.as_u32())?;
            add_field_u32(total, prefix.child("value_type_id"), value.as_u32())?;
            add_field_usize(total, prefix.child("capacity"), *capacity)
        }
    }
}

pub(super) fn add_field_bytes(total: &mut usize, key: KeyLen, value: &str) -> Result<()> {
    add_field_value_len(total, key, value.len())
}

pub(super) fn add_field_value_label_len(
    total: &mut usize,
    key: KeyLen,
    value: &crate::ArtifactValue,
) -> Result<()> {
    add_field_value_len(total, key, value.label_len()?)
}

pub(super) fn add_field_u32(total: &mut usize, key: KeyLen, value: u32) -> Result<()> {
    add_field_value_len(total, key, decimal_len_u32(value))
}

pub(super) fn add_field_usize(total: &mut usize, key: KeyLen, value: usize) -> Result<()> {
    add_field_value_len(total, key, decimal_len_usize(value))
}

fn add_field_value_len(total: &mut usize, key: KeyLen, value_len: usize) -> Result<()> {
    add_encoded_bytes(total, key.0)?;
    add_encoded_bytes(total, 1)?;
    add_encoded_bytes(total, value_len)?;
    add_encoded_bytes(total, 1)
}

pub(super) const fn decimal_len_u32(value: u32) -> usize {
    decimal_len_u64(value as u64)
}

pub(super) const fn decimal_len_usize(value: usize) -> usize {
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

fn add_next_state_bytes(total: &mut usize, prefix: &KeyLen, next_state: &NextState) -> Result<()> {
    match next_state {
        NextState::Current => Ok(()),
        NextState::Value(state) => {
            add_field_u32(total, prefix.child("next_state_value"), state.as_u32())
        }
        NextState::Template(template) => {
            add_value_template_bytes(total, prefix.child("next_state_template"), template)
        }
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            add_value_template_bytes(total, prefix.child("next_state_condition"), condition)?;
            let then_prefix = prefix.child("next_state_then");
            add_field_bytes(
                total,
                then_prefix.child("next_state"),
                then_state.kind_str(),
            )?;
            add_next_state_bytes(total, &then_prefix, then_state)?;
            let else_prefix = prefix.child("next_state_else");
            add_field_bytes(
                total,
                else_prefix.child("next_state"),
                else_state.kind_str(),
            )?;
            add_next_state_bytes(total, &else_prefix, else_state)
        }
    }
}

fn add_action_bytes(
    total: &mut usize,
    action_prefix: &KeyLen,
    action: &ArtifactAction,
) -> Result<()> {
    match action {
        ArtifactAction::Emit { output } => {
            add_field_bytes(total, action_prefix.child("kind"), "emit")?;
            add_field_u32(total, action_prefix.child("output"), output.as_u32())
        }
        ArtifactAction::Spawn {
            target,
            process_ref,
        } => {
            add_field_bytes(total, action_prefix.child("kind"), "spawn")?;
            add_field_u32(
                total,
                action_prefix.child("target_process"),
                target.as_u32(),
            )?;
            add_field_u32(
                total,
                action_prefix.child("process_ref"),
                process_ref.as_u32(),
            )
        }
        ArtifactAction::SpawnOutcome {
            outcome,
            outcome_ty,
            target,
        } => {
            add_field_bytes(total, action_prefix.child("kind"), "spawn_outcome")?;
            add_field_u32(total, action_prefix.child("outcome"), outcome.as_u32())?;
            add_field_u32(
                total,
                action_prefix.child("outcome_type_id"),
                outcome_ty.as_u32(),
            )?;
            add_field_u32(
                total,
                action_prefix.child("target_process"),
                target.as_u32(),
            )
        }
        ArtifactAction::Send {
            target,
            message,
            payload,
        } => {
            add_field_bytes(total, action_prefix.child("kind"), "send")?;
            add_send_target_bytes(total, action_prefix, target)?;
            add_field_u32(total, action_prefix.child("message"), message.as_u32())?;
            add_field_bytes(
                total,
                action_prefix.child("payload"),
                if payload.is_some() {
                    "template"
                } else {
                    "none"
                },
            )?;
            if let Some(payload) = payload {
                add_value_template_bytes(total, action_prefix.child("payload_template"), payload)?;
            }
            Ok(())
        }
        ArtifactAction::SendOutcome {
            outcome,
            outcome_ty,
            target,
            message,
            payload,
        } => {
            add_field_bytes(total, action_prefix.child("kind"), "send_outcome")?;
            add_field_u32(total, action_prefix.child("outcome"), outcome.as_u32())?;
            add_field_u32(
                total,
                action_prefix.child("outcome_type_id"),
                outcome_ty.as_u32(),
            )?;
            add_send_target_bytes(total, action_prefix, target)?;
            add_field_u32(total, action_prefix.child("message"), message.as_u32())?;
            add_field_bytes(
                total,
                action_prefix.child("payload"),
                if payload.is_some() {
                    "template"
                } else {
                    "none"
                },
            )?;
            if let Some(payload) = payload {
                add_value_template_bytes(total, action_prefix.child("payload_template"), payload)?;
            }
            Ok(())
        }
        ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            add_field_bytes(total, action_prefix.child("kind"), "if_else")?;
            add_value_template_bytes(total, action_prefix.child("condition"), condition)?;
            add_field_usize(
                total,
                action_prefix.child("then_action_count"),
                then_actions.len(),
            )?;
            for (action_index, action) in then_actions.iter().enumerate() {
                add_action_bytes(
                    total,
                    &action_prefix.indexed_child("then_action", action_index),
                    action,
                )?;
            }
            add_field_usize(
                total,
                action_prefix.child("else_action_count"),
                else_actions.len(),
            )?;
            for (action_index, action) in else_actions.iter().enumerate() {
                add_action_bytes(
                    total,
                    &action_prefix.indexed_child("else_action", action_index),
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
            add_field_bytes(total, action_prefix.child("kind"), "for_each")?;
            add_field_u32(
                total,
                action_prefix.child("loop_element"),
                element.id.as_u32(),
            )?;
            add_field_u32(
                total,
                action_prefix.child("element_type_id"),
                element.ty.as_u32(),
            )?;
            add_field_usize(total, action_prefix.child("max_items"), *max_items)?;
            add_value_template_bytes(total, action_prefix.child("collection"), collection)?;
            add_field_usize(total, action_prefix.child("body_action_count"), body.len())?;
            for (action_index, action) in body.iter().enumerate() {
                add_action_bytes(
                    total,
                    &action_prefix.indexed_child("body_action", action_index),
                    action,
                )?;
            }
            Ok(())
        }
    }
}

fn add_send_target_bytes(
    total: &mut usize,
    action_prefix: &KeyLen,
    target: &ArtifactSendTarget,
) -> Result<()> {
    match target {
        ArtifactSendTarget::ProcessRef(process_ref) => {
            add_field_bytes(total, action_prefix.child("target"), "process_ref")?;
            add_field_u32(
                total,
                action_prefix.child("target_process_ref"),
                process_ref.as_u32(),
            )?;
        }
        ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
            add_field_bytes(total, action_prefix.child("target"), "received_payload")?;
            add_field_u32(
                total,
                action_prefix.child("target_payload_type_id"),
                ty.as_u32(),
            )?;
            add_field_u32(
                total,
                action_prefix.child("target_process"),
                target_process.as_u32(),
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
