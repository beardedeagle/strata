use std::collections::{BTreeSet, VecDeque};

use super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedProcess, CheckedProcessId,
    CheckedStepResult, CheckedTransition,
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
    let mut spawned_targets = BTreeSet::new();

    for action in transition.actions() {
        match action {
            CheckedAction::Emit { .. } => {}
            CheckedAction::Spawn { target } => {
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
                if !spawned_targets.insert(*target) {
                    return Err(Error::new(format!(
                        "process {} duplicates spawn target {} within message transition {}",
                        process.debug_name(),
                        process_label(processes, *target)?,
                        transition.message().as_u32()
                    )));
                }
            }
            CheckedAction::Send { target, message } => {
                let Some(target_process) = processes.get(target.index()) else {
                    return Err(Error::new(format!(
                        "process {} sends to undefined process id {}",
                        process.debug_name(),
                        target.as_u32()
                    )));
                };
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticProcessStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticProcessInstance {
    process_id: CheckedProcessId,
    status: StaticProcessStatus,
    mailbox: VecDeque<CheckedMessageId>,
}

fn validate_static_runtime_order(
    processes: &[CheckedProcess],
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
) -> Result<()> {
    let mut instances = vec![StaticProcessInstance {
        process_id: entry_process,
        status: StaticProcessStatus::Running,
        mailbox: VecDeque::from([entry_message]),
    }];
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
                CheckedAction::Spawn { target } => {
                    let target_process = process_by_id(processes, *target)?;
                    if instances
                        .iter()
                        .any(|instance| instance.process_id == *target)
                    {
                        return Err(Error::new(format!(
                            "process {} spawns process {}, which is already spawned",
                            process.debug_name(),
                            target_process.debug_name()
                        )));
                    }
                    instances.push(StaticProcessInstance {
                        process_id: *target,
                        status: StaticProcessStatus::Running,
                        mailbox: VecDeque::new(),
                    });
                }
                CheckedAction::Send { target, message } => {
                    let target_process = process_by_id(processes, *target)?;
                    if message.index() >= target_process.message_variants().len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by {}",
                            process.debug_name(),
                            message.as_u32(),
                            target_process.debug_name()
                        )));
                    }

                    let Some(target_index) = instances
                        .iter()
                        .position(|instance| instance.process_id == *target)
                    else {
                        return Err(Error::new(format!(
                            "process {} sends to {} before it is spawned",
                            process.debug_name(),
                            target_process.debug_name()
                        )));
                    };

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
