use std::collections::{BTreeMap, VecDeque};

use super::step_results::apply_static_step_result;
use super::supervision::{
    StaticSupervisorChildKey, StaticSupervisorKey, static_spawn_capacity_available,
};
use super::{
    StaticActionContext, StaticActionState, StaticMessageEnvelope, StaticProcessId,
    StaticProcessInstance, StaticProcessStatus, execute_static_action,
};
use crate::language::checked::{
    CheckedAction, CheckedEffectOutcomeId, CheckedMessageId, CheckedPayloadValue, CheckedProcess,
    CheckedProcessId, CheckedSpawnSiteId, CheckedStepResult, CheckedSupervisorChildId,
    CheckedSupervisorId, CheckedTypeRef,
};
use crate::language::checker::static_validation::process_refs::process_by_id;
use crate::language::diagnostic::{Error, Result};

pub(in crate::language::checker::static_validation) struct StaticSpawnOutcomeExecution {
    pub(in crate::language::checker::static_validation) instance_count: usize,
    pub(in crate::language::checker::static_validation) next_pid: StaticProcessId,
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
        supervisor_parent: None,
        mailbox: VecDeque::new(),
    });

    pid = pid.checked_next()?;
    instances.push(StaticProcessInstance {
        pid,
        process_id: child_process_id,
        state: child_process.init_state(),
        status: StaticProcessStatus::Running,
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
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        });
        pid = pid.checked_next()?;
    }
    Ok((instances, pid))
}
