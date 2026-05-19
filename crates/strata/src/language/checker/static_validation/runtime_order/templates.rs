use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField, ArtifactValue};

use super::super::super::super::checked::{
    CheckedPayloadValue, CheckedProcess, CheckedProcessRefId, CheckedValueBooleanOperator,
    CheckedValueEqualityOperator, CheckedValueTemplate,
};
use super::super::super::super::diagnostic::{Error, Result};
use super::{StaticLoopElementBinding, StaticProcessId, resolve_static_process_ref};

pub(super) fn evaluate_checked_runtime_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    loop_elements: &[StaticLoopElementBinding],
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
        CheckedValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_checked_runtime_template(
                value,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let variant = value.ty().enum_variant_label(*variant)?;
            let payload = checked_payload_value(&value)?
                .project_enum_payload(variant.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), payload))
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_checked_runtime_template(
                record,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let value = checked_payload_value(&record)?
                .project_record_field(field.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let value = checked_payload_value(&list)?
                .project_list_element(*index, *len)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let value = checked_payload_value(&list)?
                .project_list_prefix_element(*index, *prefix_len)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let value = checked_payload_value(&list)?
                .project_list_rest(*prefix_len)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let map = evaluate_checked_runtime_template(
                map,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let value = checked_payload_value(&map)?
                .project_map_value(key, keys, *projection)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            let map = evaluate_checked_runtime_template(
                map,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let value = checked_payload_value(&map)?
                .project_map_rest(excluded_keys)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
        } => {
            let pid = resolve_static_process_ref(process, process_refs, *process_ref)?;
            Ok(CheckedPayloadValue::process_ref(
                ty.clone(),
                format!("{ty}#{}", pid.as_u32()),
                *target,
                u64::from(pid.as_u32()),
            ))
        }
        CheckedValueTemplate::LoopElement { ty, element } => {
            let value = loop_elements
                .iter()
                .find(|binding| binding.id == *element)
                .map(|binding| &binding.value)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} references inactive loop element id {}",
                        process.debug_name(),
                        element.as_u32()
                    ))
                })?;
            if value.ty() != ty {
                return Err(Error::new(format!(
                    "loop element id {} has type {}, expected {}",
                    element.as_u32(),
                    value.ty(),
                    ty
                )));
            }
            Ok(value.clone())
        }
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_checked_runtime_template(
                payload,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::EnumVariant {
                    variant: ty.enum_variant_label(*variant)?.to_string(),
                    payload: Box::new(checked_payload_value(&payload)?),
                },
            ))
        }
        CheckedValueTemplate::Record { ty, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            let mut seen = BTreeSet::new();
            for field in fields {
                let value = evaluate_checked_runtime_template(
                    field.value(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                )?;
                if !seen.insert(field.name()) {
                    return Err(Error::new(format!(
                        "record template duplicates field {}",
                        field.name()
                    )));
                }
                values.push(ArtifactRecordField {
                    name: field.name().to_string(),
                    value: checked_payload_value(&value)?,
                });
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Record {
                    constructor: ty.label().to_string(),
                    fields: values,
                },
            ))
        }
        CheckedValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value = evaluate_checked_runtime_template(
                    item,
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                )?;
                values.push(checked_payload_value(&value)?);
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::List(values),
            ))
        }
        CheckedValueTemplate::Map { ty, entries } => {
            let mut values = Vec::with_capacity(entries.len());
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = evaluate_checked_runtime_template(
                    entry.key(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                )?;
                let value = evaluate_checked_runtime_template(
                    entry.value(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                )?;
                let key_value = checked_payload_value(&key)?;
                let item_value = checked_payload_value(&value)?;
                if !seen.insert(key_value.clone()) {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key_value.label()
                    )));
                }
                values.push(ArtifactMapEntry {
                    key: key_value,
                    value: item_value,
                });
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Map(values),
            ))
        }
        CheckedValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_runtime_template(
                left,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            if left.ty() != operand_ty {
                return Err(Error::new(format!(
                    "equality left operand has type {}, expected {}",
                    left.ty(),
                    operand_ty
                )));
            }
            let left_value = checked_payload_value_ref(&left)?;
            let right = evaluate_checked_runtime_template(
                right,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            if right.ty() != operand_ty {
                return Err(Error::new(format!(
                    "equality right operand has type {}, expected {}",
                    right.ty(),
                    operand_ty
                )));
            }
            let right_value = checked_payload_value_ref(&right)?;
            let is_equal = left_value == right_value;
            let selected = match operator {
                CheckedValueEqualityOperator::Equal => is_equal,
                CheckedValueEqualityOperator::NotEqual => !is_equal,
            };
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(selected)),
            ))
        }
        CheckedValueTemplate::BooleanNot { ty, operand } => {
            let value = evaluate_checked_runtime_template(
                operand,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(!checked_runtime_bool_value(&value)?)),
            ))
        }
        CheckedValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_runtime_template(
                left,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
            )?;
            let left = checked_runtime_bool_value(&left)?;
            let selected = match operator {
                CheckedValueBooleanOperator::And => {
                    left && checked_runtime_bool_value(&evaluate_checked_runtime_template(
                        right,
                        received_payload,
                        current_state_payload,
                        process,
                        process_refs,
                        loop_elements,
                    )?)?
                }
                CheckedValueBooleanOperator::Or => {
                    left || checked_runtime_bool_value(&evaluate_checked_runtime_template(
                        right,
                        received_payload,
                        current_state_payload,
                        process,
                        process_refs,
                        loop_elements,
                    )?)?
                }
            };
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(selected)),
            ))
        }
    }
}

fn bool_atom(value: bool) -> String {
    if value {
        "True".to_string()
    } else {
        "False".to_string()
    }
}

fn checked_payload_value_ref(payload: &CheckedPayloadValue) -> Result<&ArtifactValue> {
    payload
        .value()
        .ok_or_else(|| Error::new("process reference payloads are not valid state values"))
}

fn checked_runtime_bool_value(payload: &CheckedPayloadValue) -> Result<bool> {
    let value = checked_payload_value_ref(payload)?;
    match value {
        ArtifactValue::Atom(label) if label == "True" => Ok(true),
        ArtifactValue::Atom(label) if label == "False" => Ok(false),
        _ => Err(Error::new(format!(
            "boolean predicate operand produced non-Bool value {}",
            value.label()
        ))),
    }
}

pub(super) fn checked_payload_value(payload: &CheckedPayloadValue) -> Result<ArtifactValue> {
    payload
        .value()
        .cloned()
        .ok_or_else(|| Error::new("process reference payloads are not valid state values"))
}
