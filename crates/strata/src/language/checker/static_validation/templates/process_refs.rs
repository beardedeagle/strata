use super::*;

pub(in crate::language::checker::static_validation) fn validate_value_template_process_refs(
    processes: &[CheckedProcess],
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
    allow_direct_process_ref: bool,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(value) => {
            if value.process_ref_payload().is_some() {
                return Err(Error::new(
                    "process reference payload templates must be direct message payloads",
                ));
            }
            Ok(())
        }
        CheckedValueTemplate::ReceivedPayload { ty }
        | CheckedValueTemplate::CurrentStatePayload { ty } => {
            if !allow_direct_process_ref {
                reject_projected_process_ref_payload_type(ty)?;
            }
            Ok(())
        }
        CheckedValueTemplate::LoopElement { ty, .. } => {
            reject_projected_process_ref_payload_type(ty)
        }
        CheckedValueTemplate::EnumPayload { ty, value, .. } => {
            reject_projected_process_ref_payload_type(ty)?;
            validate_value_template_process_refs(processes, process, value, spawned_refs, false)
        }
        CheckedValueTemplate::RecordField { ty, record, .. } => {
            reject_projected_process_ref_payload_type(ty)?;
            validate_value_template_process_refs(processes, process, record, spawned_refs, false)
        }
        CheckedValueTemplate::ListElement { ty, list, .. }
        | CheckedValueTemplate::ListPrefixElement { ty, list, .. }
        | CheckedValueTemplate::ListRest { ty, list, .. } => {
            reject_projected_process_ref_payload_type(ty)?;
            validate_value_template_process_refs(processes, process, list, spawned_refs, false)
        }
        CheckedValueTemplate::MapValue { ty, map, .. } => {
            reject_projected_process_ref_payload_type(ty)?;
            validate_value_template_process_refs(processes, process, map, spawned_refs, false)
        }
        CheckedValueTemplate::MapRest { ty, map, .. } => {
            reject_projected_process_ref_payload_type(ty)?;
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
        CheckedValueTemplate::Equality { left, right, .. } => {
            validate_value_template_process_refs(processes, process, left, spawned_refs, false)?;
            validate_value_template_process_refs(processes, process, right, spawned_refs, false)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            validate_value_template_process_refs(processes, process, operand, spawned_refs, false)
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            validate_value_template_process_refs(processes, process, left, spawned_refs, false)?;
            validate_value_template_process_refs(processes, process, right, spawned_refs, false)
        }
    }
}

fn reject_projected_process_ref_payload_type(ty: &CheckedTypeRef) -> Result<()> {
    if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
        return Err(Error::new(
            "process reference payload templates must be direct message payloads",
        ));
    }
    Ok(())
}

pub(in crate::language::checker::static_validation) fn reject_process_ref_template_in_next_state(
    template: &CheckedValueTemplate,
) -> Result<()> {
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
        CheckedValueTemplate::EnumPayload { ty, value, .. } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            reject_process_ref_template_in_next_state(value)
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
        CheckedValueTemplate::ListPrefixElement { ty, list, .. } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            reject_process_ref_template_in_next_state(list)
        }
        CheckedValueTemplate::ListRest { ty, list, .. } => {
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
        CheckedValueTemplate::MapRest { ty, map, .. } => {
            if matches!(ty.kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(process_ref_next_state_error());
            }
            reject_process_ref_template_in_next_state(map)
        }
        CheckedValueTemplate::Equality {
            operand_ty,
            left,
            right,
            ..
        } => {
            reject_projected_process_ref_payload_type(operand_ty)?;
            reject_process_ref_template_in_next_state(left)?;
            reject_process_ref_template_in_next_state(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            reject_process_ref_template_in_next_state(operand)
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            reject_process_ref_template_in_next_state(left)?;
            reject_process_ref_template_in_next_state(right)
        }
        CheckedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference templates are not valid next-state values",
        )),
        CheckedValueTemplate::LoopElement { .. } => Err(Error::new(
            "loop element templates are not valid next-state values",
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
