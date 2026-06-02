use std::collections::{BTreeMap, VecDeque};

use super::step_results::apply_static_step_result;
use super::supervision::{
    StaticSupervisorChildKey, StaticSupervisorKey, static_spawn_capacity_available,
};
use super::{
    StaticActionContext, StaticActionState, StaticMailboxState, StaticMessageEnvelope,
    StaticProcessId, StaticProcessInstance, StaticProcessStatus, StaticStopReason,
    execute_static_action,
};
use crate::language::ast::Identifier;
use crate::language::checked::{
    CheckedAction, CheckedEffectOutcomeId, CheckedMessageId, CheckedPayloadValue, CheckedProcess,
    CheckedProcessId, CheckedProcessRefId, CheckedSendTarget, CheckedSpawnSiteId,
    CheckedStepResult, CheckedSupervisorChildId, CheckedSupervisorId, CheckedTypeKind,
    CheckedTypeRef, CheckedValueShape,
};
use crate::language::checker::static_validation::process_refs::process_by_id;
use crate::language::diagnostic::{Error, Result};

pub(in crate::language::checker::static_validation) struct StaticSpawnOutcomeExecution {
    pub(in crate::language::checker::static_validation) instance_count: usize,
    pub(in crate::language::checker::static_validation) next_pid: StaticProcessId,
    pub(in crate::language::checker::static_validation) outcome: CheckedPayloadValue,
}

pub(in crate::language::checker::static_validation) struct StaticSendOutcomeExecution {
    pub(in crate::language::checker::static_validation) target_mailbox_len: usize,
    pub(in crate::language::checker::static_validation) outcome: CheckedPayloadValue,
}

pub(in crate::language::checker::static_validation) fn static_spawn_capacity_available_for_test(
    processes: &[CheckedProcess],
    instance_count: usize,
    process_id: CheckedProcessId,
) -> Result<bool> {
    static_spawn_capacity_available(processes, instance_count, process_id)
}

pub(in crate::language::checker::static_validation) fn static_spawn_outcome_execution_for_test(
    processes: &[CheckedProcess],
    current_process_id: CheckedProcessId,
    instance_count: usize,
    target: CheckedProcessId,
    outcome_ty: CheckedTypeRef,
) -> Result<StaticSpawnOutcomeExecution> {
    let current_process = process_by_id(processes, current_process_id)?;
    let (mut instances, mut next_pid) =
        static_instances_for_test(current_process_id, current_process, instance_count)?;
    let mut local_process_refs = BTreeMap::new();
    let mut supervisor_children = BTreeMap::new();
    let mut effect_outcomes = Vec::new();
    let envelope = StaticMessageEnvelope::new(CheckedMessageId::from_index(0)?, None);
    let action = CheckedAction::SpawnOutcome {
        outcome: CheckedEffectOutcomeId::from_index(0)?,
        outcome_ty,
        target,
        spawn_site: CheckedSpawnSiteId::from_index(0)?,
    };

    let context = StaticActionContext {
        processes,
        process: current_process,
        current_pid: StaticProcessId::FIRST,
        envelope: &envelope,
        current_state_payload: None,
        loop_elements: &[],
    };
    let mut state = StaticActionState {
        instances: &mut instances,
        next_pid: &mut next_pid,
        local_process_refs: &mut local_process_refs,
        supervisor_children: &mut supervisor_children,
        effect_outcomes: &mut effect_outcomes,
    };
    execute_static_action(context, &mut state, &action)?;

    let outcome = effect_outcomes
        .pop()
        .ok_or_else(|| Error::new("spawn outcome test did not bind an effect outcome"))?;
    Ok(StaticSpawnOutcomeExecution {
        instance_count: instances.len(),
        next_pid,
        outcome: outcome.value,
    })
}

pub(in crate::language::checker::static_validation) fn static_process_ref_send_outcome_for_test(
    processes: &[CheckedProcess],
    status: StaticProcessStatus,
    stop_reason: Option<StaticStopReason>,
    mailbox_state: StaticMailboxState,
) -> Result<StaticSendOutcomeExecution> {
    let current_process_id = CheckedProcessId::from_index(0)?;
    let target_process_id = CheckedProcessId::from_index(1)?;
    let current_process = process_by_id(processes, current_process_id)?;
    let target_process = process_by_id(processes, target_process_id)?;
    let target_pid = StaticProcessId::FIRST.checked_next()?;
    let mut instances = vec![
        StaticProcessInstance {
            pid: StaticProcessId::FIRST,
            process_id: current_process_id,
            state: current_process.init_state(),
            status: StaticProcessStatus::Running,
            stop_reason: None,
            mailbox_state: StaticMailboxState::Open,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        },
        StaticProcessInstance {
            pid: target_pid,
            process_id: target_process_id,
            state: target_process.init_state(),
            status,
            stop_reason,
            mailbox_state,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        },
    ];
    let mut local_process_refs = BTreeMap::new();
    local_process_refs.insert(CheckedProcessRefId::from_index(0)?, target_pid);
    execute_static_send_outcome_for_test(
        processes,
        current_process,
        &mut instances,
        &mut local_process_refs,
        &mut BTreeMap::new(),
        CheckedSendTarget::ProcessRef(CheckedProcessRefId::from_index(0)?),
        static_send_outcome_type(target_process.message_type()),
    )
}

pub(in crate::language::checker::static_validation) fn static_supervisor_child_send_outcome_for_test(
    processes: &[CheckedProcess],
    status: StaticProcessStatus,
    stop_reason: Option<StaticStopReason>,
    mailbox_state: StaticMailboxState,
) -> Result<StaticSendOutcomeExecution> {
    let supervisor_process_id = CheckedProcessId::from_index(0)?;
    let child_process_id = CheckedProcessId::from_index(1)?;
    let supervisor_process = process_by_id(processes, supervisor_process_id)?;
    let child_process = process_by_id(processes, child_process_id)?;
    let child_pid = StaticProcessId::FIRST.checked_next()?;
    let child_key = (
        StaticProcessId::FIRST,
        CheckedSupervisorId::from_index(0)?,
        CheckedSupervisorChildId::from_index(0)?,
    );
    let mut instances = vec![
        StaticProcessInstance {
            pid: StaticProcessId::FIRST,
            process_id: supervisor_process_id,
            state: supervisor_process.init_state(),
            status: StaticProcessStatus::Running,
            stop_reason: None,
            mailbox_state: StaticMailboxState::Open,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        },
        StaticProcessInstance {
            pid: child_pid,
            process_id: child_process_id,
            state: child_process.init_state(),
            status,
            stop_reason,
            mailbox_state,
            supervisor_parent: Some(child_key),
            mailbox: VecDeque::new(),
        },
    ];
    execute_static_send_outcome_for_test(
        processes,
        supervisor_process,
        &mut instances,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        CheckedSendTarget::SupervisorChild {
            supervisor: CheckedSupervisorId::from_index(0)?,
            child: CheckedSupervisorChildId::from_index(0)?,
            target: child_process_id,
        },
        static_send_outcome_type(child_process.message_type()),
    )
}

pub(in crate::language::checker::static_validation) fn static_supervised_restart_exit_for_test(
    processes: &[CheckedProcess],
    instance_count: usize,
    prior_restart_count: u32,
) -> Result<()> {
    let supervisor_id = CheckedSupervisorId::from_index(0)?;
    let child_id = CheckedSupervisorChildId::from_index(0)?;
    let supervisor_pid = StaticProcessId::FIRST;
    let child_pid = supervisor_pid.checked_next()?;
    let child_key = (supervisor_pid, supervisor_id, child_id);
    let (mut instances, mut next_pid) =
        static_supervised_restart_instances_for_test(processes, instance_count, child_key)?;
    let mut supervisor_children = BTreeMap::new();
    supervisor_children.insert(child_key, child_pid);
    let mut supervisor_restart_counts = BTreeMap::new();
    if prior_restart_count > 0 {
        let supervisor_key: StaticSupervisorKey = (supervisor_pid, supervisor_id);
        supervisor_restart_counts.insert(supervisor_key, prior_restart_count);
    }

    apply_static_step_result(
        processes,
        &mut instances,
        &mut next_pid,
        &mut supervisor_children,
        &mut supervisor_restart_counts,
        1,
        CheckedStepResult::Panic,
    )?;

    Ok(())
}

fn execute_static_send_outcome_for_test(
    processes: &[CheckedProcess],
    current_process: &CheckedProcess,
    instances: &mut Vec<StaticProcessInstance>,
    local_process_refs: &mut BTreeMap<CheckedProcessRefId, StaticProcessId>,
    supervisor_children: &mut BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    target: CheckedSendTarget,
    outcome_ty: CheckedTypeRef,
) -> Result<StaticSendOutcomeExecution> {
    let mut next_pid = StaticProcessId::FIRST;
    let mut effect_outcomes = Vec::new();
    let envelope = StaticMessageEnvelope::new(CheckedMessageId::from_index(0)?, None);
    let action = CheckedAction::SendOutcome {
        outcome: CheckedEffectOutcomeId::from_index(0)?,
        outcome_ty,
        target,
        port: None,
        message: CheckedMessageId::from_index(0)?,
        payload: None,
    };
    let context = StaticActionContext {
        processes,
        process: current_process,
        current_pid: StaticProcessId::FIRST,
        envelope: &envelope,
        current_state_payload: None,
        loop_elements: &[],
    };
    let mut state = StaticActionState {
        instances,
        next_pid: &mut next_pid,
        local_process_refs,
        supervisor_children,
        effect_outcomes: &mut effect_outcomes,
    };
    execute_static_action(context, &mut state, &action)?;

    let target_mailbox_len = state
        .instances
        .get(1)
        .map_or(0, |target| target.mailbox.len());
    let outcome = state
        .effect_outcomes
        .pop()
        .ok_or_else(|| Error::new("send outcome test did not bind an effect outcome"))?;
    Ok(StaticSendOutcomeExecution {
        target_mailbox_len,
        outcome: outcome.value,
    })
}

fn static_send_outcome_type(message_ty: &CheckedTypeRef) -> CheckedTypeRef {
    let unit_ty = CheckedTypeRef::test_value("Unit");
    let send_error_ty = CheckedTypeRef::new(
        CheckedTypeRef::test_value("SendError").id(),
        format!("SendError<{message_ty}>"),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum {
                variants: ["Full", "Stopped", "Crashed", "MailboxClosed"]
                    .into_iter()
                    .map(|variant| crate::language::checked::CheckedEnumVariant {
                        name: Identifier::new(variant).expect("test variant should be valid"),
                        payload_type: Some(message_ty.id()),
                    })
                    .collect(),
            },
        },
    );
    CheckedTypeRef::new(
        CheckedTypeRef::test_value("SendOutcome").id(),
        format!("Result<Unit,SendError<{message_ty}>>"),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum {
                variants: vec![
                    crate::language::checked::CheckedEnumVariant {
                        name: Identifier::new("Ok").expect("test variant should be valid"),
                        payload_type: Some(unit_ty.id()),
                    },
                    crate::language::checked::CheckedEnumVariant {
                        name: Identifier::new("Err").expect("test variant should be valid"),
                        payload_type: Some(send_error_ty.id()),
                    },
                ],
            },
        },
    )
}

fn static_instances_for_test(
    process_id: CheckedProcessId,
    process: &CheckedProcess,
    instance_count: usize,
) -> Result<(Vec<StaticProcessInstance>, StaticProcessId)> {
    let mut instances = Vec::with_capacity(instance_count);
    let mut pid = StaticProcessId::FIRST;
    for _ in 0..instance_count {
        instances.push(StaticProcessInstance {
            pid,
            process_id,
            state: process.init_state(),
            status: StaticProcessStatus::Running,
            stop_reason: None,
            mailbox_state: StaticMailboxState::Open,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        });
        pid = pid.checked_next()?;
    }
    Ok((instances, pid))
}

fn static_supervised_restart_instances_for_test(
    processes: &[CheckedProcess],
    instance_count: usize,
    child_key: StaticSupervisorChildKey,
) -> Result<(Vec<StaticProcessInstance>, StaticProcessId)> {
    if instance_count < 2 {
        return Err(Error::new(
            "supervised restart test requires at least two static process instances",
        ));
    }
    let supervisor_process_id = CheckedProcessId::from_index(0)?;
    let supervisor_process = process_by_id(processes, supervisor_process_id)?;
    let child_process_id = supervisor_process
        .supervisor_plans()
        .first()
        .and_then(|plan| plan.children().first())
        .map(|child| child.target())
        .ok_or_else(|| Error::new("supervised restart test requires one supervisor child"))?;
    let child_process = process_by_id(processes, child_process_id)?;

    let mut instances = Vec::with_capacity(instance_count);
    let mut pid = StaticProcessId::FIRST;
    instances.push(StaticProcessInstance {
        pid,
        process_id: supervisor_process_id,
        state: supervisor_process.init_state(),
        status: StaticProcessStatus::Running,
        stop_reason: None,
        mailbox_state: StaticMailboxState::Open,
        supervisor_parent: None,
        mailbox: VecDeque::new(),
    });

    pid = pid.checked_next()?;
    instances.push(StaticProcessInstance {
        pid,
        process_id: child_process_id,
        state: child_process.init_state(),
        status: StaticProcessStatus::Running,
        stop_reason: None,
        mailbox_state: StaticMailboxState::Open,
        supervisor_parent: Some(child_key),
        mailbox: VecDeque::new(),
    });

    pid = pid.checked_next()?;
    while instances.len() < instance_count {
        instances.push(StaticProcessInstance {
            pid,
            process_id: supervisor_process_id,
            state: supervisor_process.init_state(),
            status: StaticProcessStatus::Failed,
            stop_reason: None,
            mailbox_state: StaticMailboxState::Closed,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        });
        pid = pid.checked_next()?;
    }
    Ok((instances, pid))
}
