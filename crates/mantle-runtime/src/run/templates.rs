use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField, Error, ProcessRefId, Result};

use super::model::ActiveStep;
use crate::event::RuntimeProcessId;
use crate::program::{LoadedProgram, LoadedValueTemplate, RuntimePayload, RuntimeValue};

pub(super) fn evaluate_runtime_template(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
    received_payload: Option<&RuntimePayload>,
    step: &ActiveStep,
    process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
) -> Result<RuntimePayload> {
    match template {
        LoadedValueTemplate::Literal { ty, value } => RuntimePayload::value(*ty, value.clone()),
        LoadedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "received payload has type id {}, expected {}",
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        LoadedValueTemplate::CurrentStatePayload { ty } => {
            let payload = step.current_state_payload(program)?.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "current state payload has type id {}, expected {}",
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        LoadedValueTemplate::RecordField { ty, record, field } => {
            let record =
                evaluate_runtime_template(program, record, received_payload, step, process_refs)?;
            RuntimePayload::value(*ty, record.value.project_record_field(field)?)
        }
        LoadedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list =
                evaluate_runtime_template(program, list, received_payload, step, process_refs)?;
            RuntimePayload::value(*ty, list.value.project_list_element(*index, *len)?)
        }
        LoadedValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let map =
                evaluate_runtime_template(program, map, received_payload, step, process_refs)?;
            RuntimePayload::value(*ty, map.value.project_map_value(key, keys, *projection)?)
        }
        LoadedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            let map =
                evaluate_runtime_template(program, map, received_payload, step, process_refs)?;
            RuntimePayload::value(*ty, map.value.project_map_rest(excluded_keys)?)
        }
        LoadedValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            let pid = process_refs.get(process_ref).copied().ok_or_else(|| {
                Error::new(format!(
                    "process {} sends unbound process reference id {} as payload",
                    step.process_name,
                    process_ref.as_u32()
                ))
            })?;
            RuntimePayload::from_process_ref(*ty, *target_process, pid)
        }
        LoadedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload =
                evaluate_runtime_template(program, payload, received_payload, step, process_refs)?;
            RuntimePayload::value(
                *ty,
                RuntimeValue::EnumVariant {
                    variant: variant.clone(),
                    payload: Box::new(payload.value),
                },
            )
        }
        LoadedValueTemplate::Record { ty, fields } => {
            let type_label = program.type_label(*ty)?;
            let mut values = Vec::with_capacity(fields.len());
            let mut seen = BTreeSet::new();
            for field in fields {
                let value = evaluate_runtime_template(
                    program,
                    &field.value,
                    received_payload,
                    step,
                    process_refs,
                )?;
                if !seen.insert(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record template duplicates field {}",
                        field.name
                    )));
                }
                values.push(ArtifactRecordField {
                    name: field.name.clone(),
                    value: value.value,
                });
            }
            RuntimePayload::value(
                *ty,
                RuntimeValue::Record {
                    constructor: type_label.to_string(),
                    fields: values,
                },
            )
        }
        LoadedValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value =
                    evaluate_runtime_template(program, item, received_payload, step, process_refs)?;
                values.push(value.value);
            }
            RuntimePayload::value(*ty, RuntimeValue::List(values))
        }
        LoadedValueTemplate::Map { ty, entries } => {
            let mut values = Vec::with_capacity(entries.len());
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = evaluate_runtime_template(
                    program,
                    &entry.key,
                    received_payload,
                    step,
                    process_refs,
                )?;
                let value = evaluate_runtime_template(
                    program,
                    &entry.value,
                    received_payload,
                    step,
                    process_refs,
                )?;
                if !seen.insert(key.value.clone()) {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key.value.label()
                    )));
                }
                values.push(ArtifactMapEntry {
                    key: key.value,
                    value: value.value,
                });
            }
            RuntimePayload::value(*ty, RuntimeValue::Map(values))
        }
    }
}
