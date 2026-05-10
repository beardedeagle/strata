use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    project_canonical_list_element, project_canonical_map_value, project_canonical_record_field,
    validate_payload_value_label, validate_state_value_label,
};

use super::process_refs::{
    message_payload_type, process_ref_target, validate_process_ref_type_target,
};
use crate::language::checked::{
    CheckedMessageId, CheckedNextState, CheckedPayloadValue, CheckedProcess, CheckedProcessRefId,
    CheckedStateId, CheckedTypeKind, CheckedTypeRef, CheckedValueTemplate,
};
use crate::language::diagnostic::{Error, Result};

pub(super) fn validate_next_state(
    process: &CheckedProcess,
    current_message: CheckedMessageId,
    current_state: Option<CheckedStateId>,
    next_state: CheckedNextState,
) -> Result<()> {
    match next_state {
        CheckedNextState::Current => Ok(()),
        CheckedNextState::Value(state) => {
            if state.index() >= process.state_values().len() {
                return Err(Error::new(format!(
                    "process {} next_state id {} is not a valid state value",
                    process.debug_name(),
                    state.as_u32()
                )));
            }
            Ok(())
        }
        CheckedNextState::Template(template) => {
            if template.result_type() != process.state_type() {
                return Err(Error::new(format!(
                    "process {} next_state template has type {}, expected {}",
                    process.debug_name(),
                    template.result_type(),
                    process.state_type()
                )));
            }
            validate_value_template_binding_types(
                &template,
                message_payload_type(process, current_message)?,
                current_state_payload_type(process, current_state)?,
            )?;
            validate_value_template_payload_labels(&template)?;
            reject_process_ref_template_in_next_state(&template)?;
            if !checked_template_depends_on_received_payload(&template) {
                resolve_checked_template_state(
                    process,
                    &template,
                    None,
                    current_state
                        .and_then(|state| process.state_values().get(state.index()))
                        .and_then(|state| state.payload()),
                )?;
            }
            Ok(())
        }
    }
}

pub(super) fn current_state_payload_type(
    process: &CheckedProcess,
    current_state: Option<CheckedStateId>,
) -> Result<Option<&CheckedTypeRef>> {
    let Some(current_state) = current_state else {
        return Ok(None);
    };
    let state = process
        .state_values()
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} current_state id {} is not a valid state value",
                process.debug_name(),
                current_state.as_u32()
            ))
        })?;
    Ok(state.payload().map(CheckedPayloadValue::ty))
}

pub(super) fn validate_value_template_binding_types(
    template: &CheckedValueTemplate,
    received_payload_type: Option<&CheckedTypeRef>,
    current_state_payload_type: Option<&CheckedTypeRef>,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_) => Ok(()),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            let Some(received_payload_type) = received_payload_type else {
                return Err(Error::new(
                    "received payload template requires a payload-bearing message",
                ));
            };
            if ty != received_payload_type {
                return Err(Error::new(format!(
                    "received payload template has type {}, expected {}",
                    ty, received_payload_type
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            let Some(current_state_payload_type) = current_state_payload_type else {
                return Err(Error::new(
                    "current state payload template requires a payload-bearing state",
                ));
            };
            if ty != current_state_payload_type {
                return Err(Error::new(format!(
                    "current state payload template has type {}, expected {}",
                    ty, current_state_payload_type
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::RecordField { record, .. } => validate_value_template_binding_types(
            record,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::ListElement { list, .. } => validate_value_template_binding_types(
            list,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::MapValue { map, .. } => validate_value_template_binding_types(
            map,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::EnumVariant { payload, .. } => validate_value_template_binding_types(
            payload,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_binding_types(
                    field.value(),
                    received_payload_type,
                    current_state_payload_type,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                validate_value_template_binding_types(
                    item,
                    received_payload_type,
                    current_state_payload_type,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                validate_value_template_binding_types(
                    entry.key(),
                    received_payload_type,
                    current_state_payload_type,
                )?;
                validate_value_template_binding_types(
                    entry.value(),
                    received_payload_type,
                    current_state_payload_type,
                )?;
            }
            Ok(())
        }
    }
}

pub(super) fn validate_value_template_payload_labels(
    template: &CheckedValueTemplate,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(value) => {
            validate_payload_value_label(value.label()).map_err(|err| Error::new(err.to_string()))
        }
        CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. } => Ok(()),
        CheckedValueTemplate::RecordField { record, .. } => {
            validate_value_template_payload_labels(record)
        }
        CheckedValueTemplate::ListElement { list, .. } => {
            validate_value_template_payload_labels(list)
        }
        CheckedValueTemplate::MapValue { map, key, keys, .. } => {
            validate_payload_value_label(key).map_err(|err| Error::new(err.to_string()))?;
            for expected_key in keys {
                validate_payload_value_label(expected_key)
                    .map_err(|err| Error::new(err.to_string()))?;
            }
            validate_value_template_payload_labels(map)
        }
        CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            validate_value_template_payload_labels(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_payload_labels(field.value())?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                validate_value_template_payload_labels(item)?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                validate_value_template_payload_labels(entry.key())?;
                validate_value_template_payload_labels(entry.value())?;
            }
            Ok(())
        }
    }
}

pub(super) fn validate_value_template_process_refs(
    processes: &[CheckedProcess],
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
    allow_direct_process_ref: bool,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. } => Ok(()),
        CheckedValueTemplate::RecordField { record, .. } => {
            validate_value_template_process_refs(processes, process, record, spawned_refs, false)
        }
        CheckedValueTemplate::ListElement { list, .. } => {
            validate_value_template_process_refs(processes, process, list, spawned_refs, false)
        }
        CheckedValueTemplate::MapValue { map, .. } => {
            validate_value_template_process_refs(processes, process, map, spawned_refs, false)
        }
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
            ..
        } => {
            if !allow_direct_process_ref {
                return Err(Error::new(
                    "process reference payload templates must be direct message payloads",
                ));
            }
            validate_process_ref_type_target(processes, ty, *target)?;
            let declared_target = process_ref_target(process, *process_ref)?;
            if declared_target != *target {
                return Err(Error::new(format!(
                    "process {} process reference payload id {} targets process id {}, expected {}",
                    process.debug_name(),
                    process_ref.as_u32(),
                    declared_target.as_u32(),
                    target.as_u32()
                )));
            }
            if !spawned_refs.contains(process_ref) {
                return Err(Error::new(format!(
                    "process {} sends unbound process reference id {} as payload",
                    process.debug_name(),
                    process_ref.as_u32()
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            validate_value_template_process_refs(processes, process, payload, spawned_refs, false)
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_process_refs(
                    processes,
                    process,
                    field.value(),
                    spawned_refs,
                    false,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                validate_value_template_process_refs(
                    processes,
                    process,
                    item,
                    spawned_refs,
                    false,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                validate_value_template_process_refs(
                    processes,
                    process,
                    entry.key(),
                    spawned_refs,
                    false,
                )?;
                validate_value_template_process_refs(
                    processes,
                    process,
                    entry.value(),
                    spawned_refs,
                    false,
                )?;
            }
            Ok(())
        }
    }
}

fn reject_process_ref_template_in_next_state(template: &CheckedValueTemplate) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(value) => {
            if value.process_ref_payload().is_some() {
                return Err(process_ref_next_state_error());
            }
            Ok(())
        }
        CheckedValueTemplate::ReceivedPayload { ty } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            Ok(())
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            Ok(())
        }
        CheckedValueTemplate::RecordField { ty, record, .. } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            reject_process_ref_template_in_next_state(record)
        }
        CheckedValueTemplate::ListElement { ty, list, .. } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            reject_process_ref_template_in_next_state(list)
        }
        CheckedValueTemplate::MapValue { ty, map, .. } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            reject_process_ref_template_in_next_state(map)
        }
        CheckedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference templates are not valid next-state values",
        )),
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            reject_process_ref_template_in_next_state(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                reject_process_ref_template_in_next_state(field.value())?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                reject_process_ref_template_in_next_state(item)?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                reject_process_ref_template_in_next_state(entry.key())?;
                reject_process_ref_template_in_next_state(entry.value())?;
            }
            Ok(())
        }
    }
}

fn process_ref_next_state_error() -> Error {
    Error::new("process reference templates are not valid next-state values")
}

fn checked_template_depends_on_received_payload(template: &CheckedValueTemplate) -> bool {
    match template {
        CheckedValueTemplate::Literal(_) => false,
        CheckedValueTemplate::ReceivedPayload { .. } => true,
        CheckedValueTemplate::CurrentStatePayload { .. } => false,
        CheckedValueTemplate::RecordField { record, .. } => {
            checked_template_depends_on_received_payload(record)
        }
        CheckedValueTemplate::ListElement { list, .. } => {
            checked_template_depends_on_received_payload(list)
        }
        CheckedValueTemplate::MapValue { map, .. } => {
            checked_template_depends_on_received_payload(map)
        }
        CheckedValueTemplate::ProcessRef { .. } => false,
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            checked_template_depends_on_received_payload(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_received_payload(field.value())),
        CheckedValueTemplate::List { items, .. } => items
            .iter()
            .any(checked_template_depends_on_received_payload),
        CheckedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            checked_template_depends_on_received_payload(entry.key())
                || checked_template_depends_on_received_payload(entry.value())
        }),
    }
}

fn evaluate_checked_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedPayloadValue> {
    match template {
        CheckedValueTemplate::Literal(value) => Ok(value.clone()),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "received payload has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            if payload.process_ref_payload().is_some() {
                return Err(Error::new(
                    "process reference payloads are not valid state values",
                ));
            }
            Ok(payload.clone())
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            let payload = current_state_payload.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "current state payload has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            Ok(payload.clone())
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            let record =
                evaluate_checked_template(record, received_payload, current_state_payload)?;
            let label = project_canonical_record_field(record.label(), field.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_checked_template(list, received_payload, current_state_payload)?;
            let label = project_canonical_list_element(list.label(), *index, *len)
                .map_err(|err| Error::new(err.to_string()))?;
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::MapValue { ty, map, key, keys } => {
            let map = evaluate_checked_template(map, received_payload, current_state_payload)?;
            let label = project_canonical_map_value(map.label(), key, keys)
                .map_err(|err| Error::new(err.to_string()))?;
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference template requires static runtime process reference bindings",
        )),
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload =
                evaluate_checked_template(payload, received_payload, current_state_payload)?;
            let label = format!("{variant}({})", payload.label());
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::Record { ty, fields } => {
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = evaluate_checked_template(
                    field.value(),
                    received_payload,
                    current_state_payload,
                )?;
                parts.push(format!("{}:{}", field.name(), value.label()));
            }
            let label = format!("{ty}{{{}}}", parts.join(","));
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::List { ty, items } => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                let value =
                    evaluate_checked_template(item, received_payload, current_state_payload)?;
                parts.push(value.label().to_string());
            }
            let label = format!("List[{}]", parts.join(","));
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::Map { ty, entries } => {
            let mut parts = BTreeMap::new();
            for entry in entries {
                let key = evaluate_checked_template(
                    entry.key(),
                    received_payload,
                    current_state_payload,
                )?;
                let value = evaluate_checked_template(
                    entry.value(),
                    received_payload,
                    current_state_payload,
                )?;
                if parts
                    .insert(key.label().to_string(), value.label().to_string())
                    .is_some()
                {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key.label()
                    )));
                }
            }
            let label = format!(
                "Map[{}]",
                parts
                    .into_iter()
                    .map(|(key, value)| format!("{key}=>{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
    }
}

fn resolve_checked_template_state(
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedStateId> {
    let value = evaluate_checked_template(template, received_payload, current_state_payload)?;
    let state_index = process
        .state_values()
        .iter()
        .position(|state| state.has_same_identity_as_payload(&value))
        .ok_or_else(|| {
            Error::new(format!(
                "process {} next_state template produced value {} not admitted by state table",
                process.debug_name(),
                value.label()
            ))
        })?;
    CheckedStateId::from_index(state_index)
}

pub(super) fn resolve_checked_next_state(
    process: &CheckedProcess,
    current_state: CheckedStateId,
    next_state: CheckedNextState,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedStateId> {
    let current_state_payload = process
        .state_values()
        .get(current_state.index())
        .and_then(|state| state.payload());
    match next_state {
        CheckedNextState::Current => Ok(current_state),
        CheckedNextState::Value(state) => Ok(state),
        CheckedNextState::Template(template) => resolve_checked_template_state(
            process,
            &template,
            received_payload,
            current_state_payload,
        ),
    }
}
