use std::collections::BTreeMap;

use mantle_artifact::{
    ArtifactPayload, ArtifactProcessRefPayload, ArtifactValueTemplate, Error, ProcessRefId, Result,
    project_canonical_list_element, project_canonical_map_value, project_canonical_record_field,
    validate_payload_value_label,
};

use super::model::ActiveStep;
use crate::event::RuntimeProcessId;
use crate::program::LoadedProgram;

pub(super) fn evaluate_runtime_template(
    program: &LoadedProgram,
    template: &ArtifactValueTemplate,
    received_payload: Option<&ArtifactPayload>,
    step: &ActiveStep,
    process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
) -> Result<ArtifactPayload> {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => Ok(ArtifactPayload {
            ty: *ty,
            value: value.clone(),
            process_ref: None,
        }),
        ArtifactValueTemplate::ReceivedPayload { ty } => {
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
        ArtifactValueTemplate::CurrentStatePayload { ty } => {
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
        ArtifactValueTemplate::RecordField { ty, record, field } => {
            let record =
                evaluate_runtime_template(program, record, received_payload, step, process_refs)?;
            let value = project_canonical_record_field(&record.value, field)?;
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list =
                evaluate_runtime_template(program, list, received_payload, step, process_refs)?;
            let value = project_canonical_list_element(&list.value, *index, *len)?;
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::MapValue { ty, map, key, keys } => {
            let map =
                evaluate_runtime_template(program, map, received_payload, step, process_refs)?;
            let value = project_canonical_map_value(&map.value, key, keys)?;
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::ProcessRef {
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
            Ok(ArtifactPayload {
                ty: *ty,
                value: format!("type{}#{}", ty.as_u32(), pid.as_u64()),
                process_ref: Some(ArtifactProcessRefPayload {
                    target_process: *target_process,
                    pid: pid.as_u64(),
                }),
            })
        }
        ArtifactValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload =
                evaluate_runtime_template(program, payload, received_payload, step, process_refs)?;
            let value = format!("{variant}({})", payload.value);
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            let type_label = program.type_label(*ty)?;
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = evaluate_runtime_template(
                    program,
                    &field.value,
                    received_payload,
                    step,
                    process_refs,
                )?;
                parts.push(format!("{}:{}", field.name, value.value));
            }
            let value = format!("{type_label}{{{}}}", parts.join(","));
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::List { ty, items } => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                let value =
                    evaluate_runtime_template(program, item, received_payload, step, process_refs)?;
                parts.push(value.value);
            }
            let value = format!("List[{}]", parts.join(","));
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::Map { ty, entries } => {
            let mut parts = BTreeMap::new();
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
                if parts.insert(key.value.clone(), value.value).is_some() {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key.value
                    )));
                }
            }
            let value = format!(
                "Map[{}]",
                parts
                    .into_iter()
                    .map(|(key, value)| format!("{key}=>{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
    }
}
