use std::collections::{BTreeSet, VecDeque};

use super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedProcess, CheckedProcessHandleId,
    CheckedProcessId, CheckedStepResult, CheckedTransition,
};
use super::super::diagnostic::{Error, Result};
use super::super::STATIC_RUNTIME_DISPATCH_LIMIT;

pub(super) fn validate_action_references(
    processes: &[CheckedProcess],
    entry_process: &CheckedProcessId,
    entry_message: &CheckedMessageId,
) -> Result<()> {
    for (process_index, process) in processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(process_index)?;
        for transition in process.transitions() {
            validate_transition(processes, process, process_id, *entry_process, transition)?;
        }
    }
    validate_static_runtime_order(processes, *entry_process, *entry_message)?;
    Ok(())
}

fn validate_transition(
    processes: &[CheckedProcess],
    process: &CheckedProcess,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    transition: &CheckedTransition,
) -> Result<()> {
    if transition.message().index() >= process.message_variants().len() {
        return Err(Error::new(format!(
            "process {} transition message id {} is not accepted",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    validate_next_state(process, transition.next_state())?;
    let mut spawned_handles = BTreeSet::new();

    for action in transition.actions() {
        match action {
            CheckedAction::Emit { .. } => {}
            CheckedAction::Spawn { target, handle } => {
                if target.index() >= processes.len() {
                    return Err(Error::new(format!(
                        "process {} spawns undefined process id {}",
                        process.debug_name(),
                        target.as_u32()
                    )));
                }
                if *target == entry_process {
                    return Err(Error::new(format!(
                        "process {} spawns entry process {}, which is already started",
                        process.debug_name(),
                        process_label(processes, *target)?
                    )));
                }
                if *target == process_id {
                    return Err(Error::new(format!(
                        "process {} spawns itself, which is not supported",
                        process.debug_name()
                    )));
                }
                let declared_target = process_handle_target(process, *handle)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn handle id {} targets process id {}, expected {}",
                        process.debug_name(),
                        handle.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                if !spawned_handles.insert(*handle) {
                    return Err(Error::new(format!(
                        "process {} duplicates process handle id {} within message transition {}",
                        process.debug_name(),
                        handle.as_u32(),
                        transition.message().as_u32()
                    )));
                }
            }
            CheckedAction::Send { target, message } => {
                let target_process_id = process_handle_target(process, *target)?;
                let target_process = process_by_id(processes, target_process_id)?;
                if message.index() >= target_process.message_variants().len() {
                    return Err(Error::new(format!(
                        "process {} sends message id {} not accepted by {}",
                        process.debug_name(),
                        message.as_u32(),
                        target_process.debug_name()
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_next_state(process: &CheckedProcess, next_state: CheckedNextState) -> Result<()> {
    if let CheckedNextState::Value(state) = next_state {
        if state.index() >= process.state_values().len() {
            return Err(Error::new(format!(
                "process {} next_state id {} is not a valid state value",
                process.debug_name(),
                state.as_u32()
            )));
        }
    }
    Ok(())
}

fn process_handle_target(
    process: &CheckedProcess,
    handle: CheckedProcessHandleId,
) -> Result<CheckedProcessId> {
    process
        .process_handles()
        .get(handle.index())
        .map(|process_handle| process_handle.target())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} references undefined process handle id {}",
                process.debug_name(),
                handle.as_u32()
            ))
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticProcessStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct StaticProcessId(u32);

impl StaticProcessId {
    const FIRST: Self = Self(1);

    fn checked_next(self) -> Result<Self> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or_else(|| Error::new("static runtime process id overflowed"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticProcessInstance {
    pid: StaticProcessId,
    process_id: CheckedProcessId,
    status: StaticProcessStatus,
    handles: Vec<Option<StaticProcessId>>,
    mailbox: VecDeque<CheckedMessageId>,
}

fn process_handles_for_static_instance(
    processes: &[CheckedProcess],
    process_id: CheckedProcessId,
) -> Result<Vec<Option<StaticProcessId>>> {
    let process = process_by_id(processes, process_id)?;
    Ok(vec![None; process.process_handles().len()])
}

fn bind_static_process_handle(
    instance: &mut StaticProcessInstance,
    handle: CheckedProcessHandleId,
    pid: StaticProcessId,
) -> Result<()> {
    let slot = instance.handles.get_mut(handle.index()).ok_or_else(|| {
        Error::new(format!(
            "references undefined process handle id {}",
            handle.as_u32()
        ))
    })?;
    if slot.replace(pid).is_some() {
        return Err(Error::new(format!(
            "rebinds process handle id {}",
            handle.as_u32()
        )));
    }
    Ok(())
}

fn resolve_static_process_handle(
    instance: &StaticProcessInstance,
    handle: CheckedProcessHandleId,
) -> Result<StaticProcessId> {
    instance
        .handles
        .get(handle.index())
        .copied()
        .ok_or_else(|| {
            Error::new(format!(
                "references undefined process handle id {}",
                handle.as_u32()
            ))
        })?
        .ok_or_else(|| {
            Error::new(format!(
                "sends to unbound process handle id {}",
                handle.as_u32()
            ))
        })
}

fn validate_static_runtime_order(
    processes: &[CheckedProcess],
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
) -> Result<()> {
    let mut instances = vec![StaticProcessInstance {
        pid: StaticProcessId::FIRST,
        process_id: entry_process,
        status: StaticProcessStatus::Running,
        handles: process_handles_for_static_instance(processes, entry_process)?,
        mailbox: VecDeque::from([entry_message]),
    }];
    let mut next_pid = StaticProcessId::FIRST.checked_next()?;
    let mut dispatches = 0usize;

    while let Some(process_index) = next_static_runnable(&instances) {
        if dispatches >= STATIC_RUNTIME_DISPATCH_LIMIT {
            return Err(Error::new(format!(
                "static runtime validation exceeded {STATIC_RUNTIME_DISPATCH_LIMIT} process step(s)"
            )));
        }

        let process_id = instances[process_index].process_id;
        let process = process_by_id(processes, process_id)?;
        let message = instances[process_index]
            .mailbox
            .pop_front()
            .ok_or_else(|| Error::new("static runtime mailbox changed during dequeue"))?;
        let transition = transition_for_message(process, message)?;

        for action in transition.actions() {
            match action {
                CheckedAction::Emit { .. } => {}
                CheckedAction::Spawn { target, handle } => {
                    process_by_id(processes, *target)?;
                    let spawned_pid = next_pid;
                    next_pid = next_pid.checked_next()?;
                    bind_static_process_handle(&mut instances[process_index], *handle, spawned_pid)
                        .map_err(|err| {
                            Error::new(format!("process {} {err}", process.debug_name()))
                        })?;
                    instances.push(StaticProcessInstance {
                        pid: spawned_pid,
                        process_id: *target,
                        status: StaticProcessStatus::Running,
                        handles: process_handles_for_static_instance(processes, *target)?,
                        mailbox: VecDeque::new(),
                    });
                }
                CheckedAction::Send { target, message } => {
                    let target_pid =
                        resolve_static_process_handle(&instances[process_index], *target).map_err(
                            |err| Error::new(format!("process {} {err}", process.debug_name())),
                        )?;
                    let Some(target_index) = instances
                        .iter()
                        .position(|instance| instance.pid == target_pid)
                    else {
                        return Err(Error::new(format!(
                            "process {} sends to missing runtime process handle id {}",
                            process.debug_name(),
                            target.as_u32()
                        )));
                    };
                    let target_process =
                        process_by_id(processes, instances[target_index].process_id)?;
                    if message.index() >= target_process.message_variants().len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by {}",
                            process.debug_name(),
                            message.as_u32(),
                            target_process.debug_name()
                        )));
                    }

                    if instances[target_index].status != StaticProcessStatus::Running {
                        return Err(Error::new(format!(
                            "process {} sends to {}, which is not running",
                            process.debug_name(),
                            target_process.debug_name()
                        )));
                    }
                    if instances[target_index].mailbox.len() >= target_process.mailbox_bound() {
                        return Err(Error::new(format!(
                            "process {} sends to {}, but its mailbox would exceed bound {}",
                            process.debug_name(),
                            target_process.debug_name(),
                            target_process.mailbox_bound()
                        )));
                    }
                    instances[target_index].mailbox.push_back(*message);
                }
            }
        }

        if transition.step_result() == CheckedStepResult::Stop {
            instances[process_index].status = StaticProcessStatus::Stopped;
        }
        dispatches += 1;
    }

    for instance in &instances {
        if !instance.mailbox.is_empty() {
            return Err(Error::new(format!(
                "process {} would retain {} unhandled message(s)",
                process_label(processes, instance.process_id)?,
                instance.mailbox.len()
            )));
        }
    }

    Ok(())
}

fn next_static_runnable(instances: &[StaticProcessInstance]) -> Option<usize> {
    instances.iter().position(|instance| {
        instance.status == StaticProcessStatus::Running && !instance.mailbox.is_empty()
    })
}

fn transition_for_message(
    process: &CheckedProcess,
    message: CheckedMessageId,
) -> Result<&CheckedTransition> {
    process
        .transitions()
        .iter()
        .find(|transition| transition.message() == message)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} has no transition for message id {}",
                process.debug_name(),
                message.as_u32()
            ))
        })
}

fn process_by_id(
    processes: &[CheckedProcess],
    process_id: CheckedProcessId,
) -> Result<&CheckedProcess> {
    processes
        .get(process_id.index())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}

fn process_label(processes: &[CheckedProcess], process_id: CheckedProcessId) -> Result<&str> {
    processes
        .get(process_id.index())
        .map(|process| process.debug_name().as_str())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}
