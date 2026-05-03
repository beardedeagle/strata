use std::collections::BTreeMap;

use super::super::ast::Process;
use super::super::checked::{CheckedAction, CheckedProcess, CheckedProcessId, CheckedStepResult};
use super::super::diagnostic::{Error, Result};
use super::super::STATIC_RUNTIME_DISPATCH_LIMIT;

pub(super) fn validate_action_references(
    processes: &[CheckedProcess],
    entry_process: &CheckedProcessId,
) -> Result<()> {
    let mut spawned_targets = BTreeMap::new();
    for (process_index, process) in processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(process_index)?;
        for action in &process.actions {
            match action {
                CheckedAction::Emit { .. } => {}
                CheckedAction::Spawn { target } => {
                    if target.index() >= processes.len() {
                        return Err(Error::new(format!(
                            "process {} spawns undefined process id {}",
                            process.debug_name,
                            target.as_u32()
                        )));
                    }
                    if target == entry_process {
                        return Err(Error::new(format!(
                            "process {} spawns entry process {}, which is already started",
                            process.debug_name,
                            process_label(processes, *target)?
                        )));
                    }
                    if *target == process_id {
                        return Err(Error::new(format!(
                            "process {} spawns itself, which is not supported in this source slice",
                            process.debug_name
                        )));
                    }
                    if let Some(previous_process) = spawned_targets.insert(*target, process_id) {
                        return Err(Error::new(format!(
                            "process {} duplicates spawn target {} already spawned by {}",
                            process.debug_name,
                            process_label(processes, *target)?,
                            process_label(processes, previous_process)?
                        )));
                    }
                }
                CheckedAction::Send { target, message } => {
                    let Some(target_process) = processes.get(target.index()) else {
                        return Err(Error::new(format!(
                            "process {} sends to undefined process id {}",
                            process.debug_name,
                            target.as_u32()
                        )));
                    };
                    if message.index() >= target_process.message_variants.len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by {}",
                            process.debug_name,
                            message.as_u32(),
                            target_process.debug_name
                        )));
                    }
                }
            }
        }
    }
    validate_static_runtime_order(processes, *entry_process)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticProcessStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StaticProcessInstance {
    process_id: CheckedProcessId,
    status: StaticProcessStatus,
    mailbox_depth: usize,
}

fn validate_static_runtime_order(
    processes: &[CheckedProcess],
    entry_process: CheckedProcessId,
) -> Result<()> {
    let mut instances = vec![StaticProcessInstance {
        process_id: entry_process,
        status: StaticProcessStatus::Running,
        mailbox_depth: 1,
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
        instances[process_index].mailbox_depth -= 1;

        for action in &process.actions {
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
                            process.debug_name, target_process.debug_name
                        )));
                    }
                    instances.push(StaticProcessInstance {
                        process_id: *target,
                        status: StaticProcessStatus::Running,
                        mailbox_depth: 0,
                    });
                }
                CheckedAction::Send { target, message } => {
                    let target_process = process_by_id(processes, *target)?;
                    if message.index() >= target_process.message_variants.len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by {}",
                            process.debug_name,
                            message.as_u32(),
                            target_process.debug_name
                        )));
                    }

                    let Some(target_index) = instances
                        .iter()
                        .position(|instance| instance.process_id == *target)
                    else {
                        return Err(Error::new(format!(
                            "process {} sends to {} before it is spawned",
                            process.debug_name, target_process.debug_name
                        )));
                    };

                    if instances[target_index].status != StaticProcessStatus::Running {
                        return Err(Error::new(format!(
                            "process {} sends to {}, which is not running",
                            process.debug_name, target_process.debug_name
                        )));
                    }
                    if instances[target_index].mailbox_depth >= target_process.mailbox_bound {
                        return Err(Error::new(format!(
                            "process {} sends to {}, but its mailbox would exceed bound {}",
                            process.debug_name,
                            target_process.debug_name,
                            target_process.mailbox_bound
                        )));
                    }
                    instances[target_index].mailbox_depth += 1;
                }
            }
        }

        if process.step_result == CheckedStepResult::Stop {
            instances[process_index].status = StaticProcessStatus::Stopped;
        }
        dispatches += 1;
    }

    for instance in &instances {
        if instance.mailbox_depth != 0 {
            return Err(Error::new(format!(
                "process {} would retain {} unhandled message(s)",
                process_label(processes, instance.process_id)?,
                instance.mailbox_depth
            )));
        }
    }

    Ok(())
}

fn next_static_runnable(instances: &[StaticProcessInstance]) -> Option<usize> {
    instances.iter().position(|instance| {
        instance.status == StaticProcessStatus::Running && instance.mailbox_depth > 0
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
        .map(|process| process.debug_name.as_str())
        .ok_or_else(|| Error::new(format!("process id {} is not defined", process_id.as_u32())))
}

pub(super) fn reject_unsupported_self_send(
    process: &Process,
    process_id: CheckedProcessId,
    actions: &[CheckedAction],
) -> Result<()> {
    if actions.iter().any(|action| {
        matches!(
            action,
            CheckedAction::Send { target, .. } if *target == process_id
        )
    }) {
        return Err(Error::new(format!(
            "process {} sends to itself, which is not supported in this source slice",
            process.name
        )));
    }

    Ok(())
}
