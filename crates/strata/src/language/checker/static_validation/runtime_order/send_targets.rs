use std::collections::BTreeMap;

use super::supervision::StaticSupervisorChildKey;
use super::{
    StaticMailboxState, StaticProcessId, StaticProcessInstance, StaticProcessStatus,
    StaticStopReason, resolve_static_process_ref,
};
use crate::language::checked::{
    CheckedPayloadValue, CheckedProcess, CheckedProcessId, CheckedProcessRefId, CheckedSendTarget,
    CheckedSupervisorChildId, CheckedSupervisorId,
};
use crate::language::diagnostic::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StaticSendOutcomeTarget {
    Active(StaticProcessId),
    InactiveSupervisorChild {
        target_process: CheckedProcessId,
        failure: StaticSendOutcomeFailure,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StaticSendOutcomeFailure {
    Full,
    Stopped,
    Crashed,
    MailboxClosed,
}

impl StaticSendOutcomeFailure {
    pub(super) const fn send_error_variant(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Stopped => "Stopped",
            Self::Crashed => "Crashed",
            Self::MailboxClosed => "MailboxClosed",
        }
    }
}

pub(super) fn resolve_static_send_target(
    process: &CheckedProcess,
    current_pid: StaticProcessId,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    supervisor_children: &BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    target: &CheckedSendTarget,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<StaticProcessId> {
    match target {
        CheckedSendTarget::ProcessRef(process_ref) => {
            resolve_static_process_ref(process, process_refs, *process_ref)
        }
        CheckedSendTarget::SupervisorChild {
            supervisor,
            child,
            target,
        } => {
            validate_static_supervisor_child_target(process, *supervisor, *child, *target)?;
            supervisor_children
                .get(&(current_pid, *supervisor, *child))
                .copied()
                .ok_or_else(|| {
                    Error::new(format!(
                        "sends to unstarted supervisor child id {}",
                        child.as_u32()
                    ))
                })
        }
        CheckedSendTarget::ReceivedPayload { ty, target } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received process reference send target requires a payload")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "received process reference send target has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            let process_ref = payload
                .process_ref_payload()
                .ok_or_else(|| Error::new("received payload is not a process reference value"))?;
            if process_ref.target() != *target {
                return Err(Error::new(format!(
                    "received process reference targets process id {}, expected {}",
                    process_ref.target().as_u32(),
                    target.as_u32()
                )));
            }
            let pid = u32::try_from(process_ref.pid()).map_err(|_| {
                Error::new(format!(
                    "received process reference pid {} cannot be represented by static validation",
                    process_ref.pid()
                ))
            })?;
            Ok(StaticProcessId(pid))
        }
    }
}

pub(super) fn resolve_static_send_outcome_target(
    process: &CheckedProcess,
    current_pid: StaticProcessId,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    supervisor_children: &BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    instances: &[StaticProcessInstance],
    target: &CheckedSendTarget,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<StaticSendOutcomeTarget> {
    match target {
        CheckedSendTarget::SupervisorChild {
            supervisor,
            child,
            target,
        } => {
            validate_static_supervisor_child_target(process, *supervisor, *child, *target)?;
            let key = (current_pid, *supervisor, *child);
            match supervisor_children.get(&key).copied() {
                Some(pid) => Ok(StaticSendOutcomeTarget::Active(pid)),
                None => Ok(StaticSendOutcomeTarget::InactiveSupervisorChild {
                    target_process: *target,
                    failure: inactive_static_supervisor_child_failure(instances, key, *target)?,
                }),
            }
        }
        CheckedSendTarget::ProcessRef(_) | CheckedSendTarget::ReceivedPayload { .. } => {
            Ok(StaticSendOutcomeTarget::Active(resolve_static_send_target(
                process,
                current_pid,
                process_refs,
                supervisor_children,
                target,
                received_payload,
            )?))
        }
    }
}

fn validate_static_supervisor_child_target(
    process: &CheckedProcess,
    supervisor: CheckedSupervisorId,
    child: CheckedSupervisorChildId,
    target: CheckedProcessId,
) -> Result<()> {
    let plan = process
        .supervisor_plans()
        .get(supervisor.index())
        .ok_or_else(|| {
            Error::new(format!(
                "references undefined supervisor id {}",
                supervisor.as_u32()
            ))
        })?;
    let child_plan = plan.children().get(child.index()).ok_or_else(|| {
        Error::new(format!(
            "references undefined supervisor child id {}",
            child.as_u32()
        ))
    })?;
    if child_plan.target() != target {
        return Err(Error::new(format!(
            "supervisor child id {} targets process id {}, expected {}",
            child.as_u32(),
            child_plan.target().as_u32(),
            target.as_u32()
        )));
    }
    Ok(())
}

fn inactive_static_supervisor_child_failure(
    instances: &[StaticProcessInstance],
    key: StaticSupervisorChildKey,
    target: CheckedProcessId,
) -> Result<StaticSendOutcomeFailure> {
    let Some(instance) = instances
        .iter()
        .rev()
        .find(|instance| instance.supervisor_parent == Some(key))
    else {
        return Ok(StaticSendOutcomeFailure::Stopped);
    };
    if instance.process_id != target {
        return Err(Error::new(format!(
            "inactive supervisor child id {} targets process id {}, expected {}",
            key.2.as_u32(),
            instance.process_id.as_u32(),
            target.as_u32()
        )));
    }
    match instance.status {
        StaticProcessStatus::Running => Err(Error::new(format!(
            "inactive supervisor child id {} still has running pid {}",
            key.2.as_u32(),
            instance.pid.as_u32()
        ))),
        StaticProcessStatus::Stopped => Ok(static_stopped_process_failure(
            instance.stop_reason,
            instance.mailbox_state,
        )),
        StaticProcessStatus::Failed => Ok(StaticSendOutcomeFailure::Crashed),
    }
}

pub(super) const fn static_stopped_process_failure(
    stop_reason: Option<StaticStopReason>,
    mailbox_state: StaticMailboxState,
) -> StaticSendOutcomeFailure {
    match stop_reason {
        Some(StaticStopReason::Normal) => StaticSendOutcomeFailure::Stopped,
        Some(StaticStopReason::SupervisorAction) => StaticSendOutcomeFailure::MailboxClosed,
        None if matches!(mailbox_state, StaticMailboxState::Closed) => {
            StaticSendOutcomeFailure::MailboxClosed
        }
        None => StaticSendOutcomeFailure::Stopped,
    }
}
