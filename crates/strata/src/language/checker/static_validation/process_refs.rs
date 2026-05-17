use std::collections::BTreeSet;

use crate::language::checked::{
    CheckedMessageId, CheckedProcess, CheckedProcessId, CheckedProcessRefId, CheckedSendTarget,
    CheckedTypeKind, CheckedTypeRef,
};
use crate::language::diagnostic::{Error, Result};

pub(super) fn message_payload_type(
    process: &CheckedProcess,
    message: CheckedMessageId,
) -> Result<Option<&CheckedTypeRef>> {
    process
        .message_cases()
        .get(message.index())
        .map(|message| message.payload_type())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} message id {} is not accepted",
                process.debug_name(),
                message.as_u32()
            ))
        })
}

pub(super) fn process_ref_target(
    process: &CheckedProcess,
    process_ref: CheckedProcessRefId,
) -> Result<CheckedProcessId> {
    process
        .process_refs()
        .get(process_ref.index())
        .map(|process_ref| process_ref.target())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} references undefined process reference id {}",
                process.debug_name(),
                process_ref.as_u32()
            ))
        })
}

pub(super) fn validate_send_target(
    processes: &[CheckedProcess],
    process: &CheckedProcess,
    current_message: CheckedMessageId,
    target: &CheckedSendTarget,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
) -> Result<CheckedProcessId> {
    match target {
        CheckedSendTarget::ProcessRef(process_ref) => {
            let target_process_id = process_ref_target(process, *process_ref)?;
            if !spawned_refs.contains(process_ref) {
                return Err(Error::new(format!(
                    "process {} sends through unbound process reference id {} within message transition {}",
                    process.debug_name(),
                    process_ref.as_u32(),
                    current_message.as_u32()
                )));
            }
            Ok(target_process_id)
        }
        CheckedSendTarget::ReceivedPayload { ty, target } => {
            validate_process_ref_type_target(processes, ty, *target)?;
            let Some(received_type) = message_payload_type(process, current_message)? else {
                return Err(Error::new(format!(
                    "process {} send target requires a payload-bearing message",
                    process.debug_name()
                )));
            };
            if received_type != ty {
                let target_type = checked_type_diagnostic(processes, ty)?;
                let received_type = checked_type_diagnostic(processes, received_type)?;
                return Err(Error::new(format!(
                    "process {} send target has process reference type {}, but current message carries {}",
                    process.debug_name(),
                    target_type,
                    received_type
                )));
            }
            Ok(*target)
        }
    }
}

pub(super) fn validate_process_ref_type_target(
    processes: &[CheckedProcess],
    ty: &CheckedTypeRef,
    target: CheckedProcessId,
) -> Result<()> {
    let expected_process = process_by_id(processes, target)?;
    match ty.kind() {
        CheckedTypeKind::ProcessRef {
            target: type_target,
        } if *type_target == target => Ok(()),
        CheckedTypeKind::ProcessRef {
            target: type_target,
        } => {
            let type_process = process_by_id(processes, *type_target)?;
            let type_name = checked_type_diagnostic(processes, ty)?;
            Err(Error::new(format!(
                "process reference payload type {type_name} targets {} (process id {}), expected {} (process id {})",
                type_process.debug_name(),
                type_target.as_u32(),
                expected_process.debug_name(),
                target.as_u32()
            )))
        }
        CheckedTypeKind::Value { .. } => {
            let type_name = checked_type_diagnostic(processes, ty)?;
            Err(Error::new(format!(
                "process reference payload type {type_name} must be a process reference type"
            )))
        }
    }
}

fn checked_type_diagnostic(processes: &[CheckedProcess], ty: &CheckedTypeRef) -> Result<String> {
    match ty.kind() {
        CheckedTypeKind::Value { .. } => Ok(ty.label().to_string()),
        CheckedTypeKind::ProcessRef { target } => {
            let process = process_by_id(processes, *target)?;
            Ok(format!("ProcessRef<{}>", process.debug_name()))
        }
    }
}

pub(super) fn process_by_id(
    processes: &[CheckedProcess],
    process_id: CheckedProcessId,
) -> Result<&CheckedProcess> {
    processes
        .get(process_id.index())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}

pub(super) fn process_label(
    processes: &[CheckedProcess],
    process_id: CheckedProcessId,
) -> Result<&str> {
    processes
        .get(process_id.index())
        .map(|process| process.debug_name().as_str())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}
