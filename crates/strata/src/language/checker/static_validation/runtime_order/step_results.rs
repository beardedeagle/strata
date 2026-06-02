use std::collections::BTreeMap;

use crate::language::checked::{
    CheckedProcess, CheckedProcessId, CheckedStepResult, CheckedSupervisorChildId,
    CheckedSupervisorChildMode, CheckedSupervisorId, CheckedSupervisorRestartIntensity,
};
use crate::language::checker::static_validation::process_refs::process_by_id;
use crate::language::diagnostic::{Error, Result};

use super::supervision::{
    StaticSupervisorChildKey, StaticSupervisorKey, spawn_static_instance,
    static_spawn_capacity_available,
};
use super::{
    StaticMailboxState, StaticProcessId, StaticProcessInstance, StaticProcessStatus,
    StaticStopReason, static_process_index_for_pid,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticSupervisorExitReason {
    Normal,
    Panic,
}

#[derive(Debug, Clone, Copy)]
struct StaticSupervisorChild {
    target: CheckedProcessId,
    mode: CheckedSupervisorChildMode,
    intensity: CheckedSupervisorRestartIntensity,
}

pub(super) fn apply_static_step_result(
    processes: &[CheckedProcess],
    instances: &mut Vec<StaticProcessInstance>,
    next_pid: &mut StaticProcessId,
    supervisor_children: &mut BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    supervisor_restart_counts: &mut BTreeMap<StaticSupervisorKey, u32>,
    process_index: usize,
    step_result: CheckedStepResult,
) -> Result<bool> {
    match step_result {
        CheckedStepResult::Continue => Ok(true),
        CheckedStepResult::Stop => {
            let pid = instances[process_index].pid;
            stop_static_supervised_children(
                processes,
                instances,
                supervisor_children,
                pid,
                StaticStopReason::SupervisorAction,
            )?;
            instances[process_index].status = StaticProcessStatus::Stopped;
            instances[process_index].stop_reason = Some(StaticStopReason::Normal);
            instances[process_index].mailbox_state = StaticMailboxState::Closed;
            handle_static_supervised_exit(
                processes,
                instances,
                next_pid,
                supervisor_children,
                supervisor_restart_counts,
                process_index,
                StaticSupervisorExitReason::Normal,
            )
        }
        CheckedStepResult::Panic => {
            let pid = instances[process_index].pid;
            stop_static_supervised_children(
                processes,
                instances,
                supervisor_children,
                pid,
                StaticStopReason::SupervisorAction,
            )?;
            instances[process_index].status = StaticProcessStatus::Failed;
            instances[process_index].stop_reason = None;
            instances[process_index].mailbox_state = StaticMailboxState::Closed;
            handle_static_supervised_exit(
                processes,
                instances,
                next_pid,
                supervisor_children,
                supervisor_restart_counts,
                process_index,
                StaticSupervisorExitReason::Panic,
            )
        }
    }
}

fn stop_static_supervised_children(
    processes: &[CheckedProcess],
    instances: &mut [StaticProcessInstance],
    supervisor_children: &BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    supervisor_pid: StaticProcessId,
    stop_reason: StaticStopReason,
) -> Result<()> {
    let supervisor_index = static_process_index_for_pid(instances, supervisor_pid)?;
    if instances[supervisor_index].status != StaticProcessStatus::Running {
        return Ok(());
    }
    let supervisor_process = process_by_id(processes, instances[supervisor_index].process_id)?;
    if supervisor_process.supervisor_plans().is_empty() {
        return Ok(());
    }

    for supervisor_plan_index in (0..supervisor_process.supervisor_plans().len()).rev() {
        let plan = &supervisor_process.supervisor_plans()[supervisor_plan_index];
        let supervisor_id = CheckedSupervisorId::from_index(supervisor_plan_index)?;
        for child_index in (0..plan.children().len()).rev() {
            let child_id = CheckedSupervisorChildId::from_index(child_index)?;
            let Some(child_pid) = supervisor_children
                .get(&(supervisor_pid, supervisor_id, child_id))
                .copied()
            else {
                continue;
            };
            let child_index = static_process_index_for_pid(instances, child_pid)?;
            if instances[child_index].status != StaticProcessStatus::Running {
                continue;
            }
            stop_static_supervised_children(
                processes,
                instances,
                supervisor_children,
                child_pid,
                stop_reason,
            )?;
            instances[child_index].status = StaticProcessStatus::Stopped;
            instances[child_index].stop_reason = Some(stop_reason);
            instances[child_index].mailbox_state = StaticMailboxState::Closed;
        }
    }
    Ok(())
}

fn handle_static_supervised_exit(
    processes: &[CheckedProcess],
    instances: &mut Vec<StaticProcessInstance>,
    next_pid: &mut StaticProcessId,
    supervisor_children: &mut BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    supervisor_restart_counts: &mut BTreeMap<StaticSupervisorKey, u32>,
    process_index: usize,
    reason: StaticSupervisorExitReason,
) -> Result<bool> {
    let Some(supervisor_child_key) = instances[process_index].supervisor_parent else {
        return Ok(reason == StaticSupervisorExitReason::Normal);
    };

    let child_pid = instances[process_index].pid;
    match supervisor_children.remove(&supervisor_child_key) {
        Some(current_pid) if current_pid == child_pid => {}
        Some(current_pid) => {
            return Err(Error::new(format!(
                "static runtime supervisor child slot for pid {} points at pid {}",
                child_pid.as_u32(),
                current_pid.as_u32()
            )));
        }
        None => {}
    }

    let child = static_supervisor_child(processes, instances, supervisor_child_key)?;
    if !should_static_restart_child(child.mode, reason) {
        return Ok(true);
    }
    let supervisor_key = (supervisor_child_key.0, supervisor_child_key.1);
    let restart_count = supervisor_restart_counts
        .get(&supervisor_key)
        .copied()
        .unwrap_or(0);
    if restart_count >= child.intensity.max_restarts() {
        return Err(Error::new(format!(
            "static runtime supervisor restart intensity exceeded for supervisor id {} child id {}",
            supervisor_child_key.1.as_u32(),
            supervisor_child_key.2.as_u32()
        )));
    }
    if restart_count > 0 {
        return Err(Error::new(format!(
            "static runtime supervisor restart throttled for supervisor id {} child id {}",
            supervisor_child_key.1.as_u32(),
            supervisor_child_key.2.as_u32()
        )));
    }
    if !static_spawn_capacity_available(processes, instances.len(), child.target)? {
        return Err(Error::new(format!(
            "static runtime supervisor restart capacity exceeded for target process id {}",
            child.target.as_u32()
        )));
    }

    let restarted_pid = *next_pid;
    *next_pid = next_pid.checked_next()?;
    spawn_static_instance(
        processes,
        instances,
        next_pid,
        supervisor_children,
        child.target,
        restarted_pid,
        Some(supervisor_child_key),
    )?;
    if supervisor_children
        .insert(supervisor_child_key, restarted_pid)
        .is_some()
    {
        return Err(Error::new(
            "static runtime supervisor child slot was already restarted",
        ));
    }
    let next_restart_count = restart_count
        .checked_add(1)
        .ok_or_else(|| Error::new("static runtime supervisor restart count overflowed"))?;
    supervisor_restart_counts.insert(supervisor_key, next_restart_count);
    Ok(true)
}

fn static_supervisor_child(
    processes: &[CheckedProcess],
    instances: &[StaticProcessInstance],
    key: StaticSupervisorChildKey,
) -> Result<StaticSupervisorChild> {
    let supervisor_index = static_process_index_for_pid(instances, key.0)?;
    let supervisor_process = process_by_id(processes, instances[supervisor_index].process_id)?;
    let plan = supervisor_process
        .supervisor_plans()
        .get(key.1.index())
        .ok_or_else(|| {
            Error::new(format!(
                "static runtime references undefined supervisor id {}",
                key.1.as_u32()
            ))
        })?;
    let child = plan.children().get(key.2.index()).ok_or_else(|| {
        Error::new(format!(
            "static runtime references undefined supervisor child id {}",
            key.2.as_u32()
        ))
    })?;
    Ok(StaticSupervisorChild {
        target: child.target(),
        mode: child.mode(),
        intensity: plan.intensity(),
    })
}

fn should_static_restart_child(
    mode: CheckedSupervisorChildMode,
    reason: StaticSupervisorExitReason,
) -> bool {
    match (mode, reason) {
        (CheckedSupervisorChildMode::Permanent, _) => true,
        (CheckedSupervisorChildMode::Transient, StaticSupervisorExitReason::Panic) => true,
        (CheckedSupervisorChildMode::Transient, StaticSupervisorExitReason::Normal)
        | (CheckedSupervisorChildMode::Temporary, _) => false,
    }
}
