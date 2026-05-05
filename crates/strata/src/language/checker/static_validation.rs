use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedProcess, CheckedProcessId,
    CheckedProcessRefId, CheckedStepResult, CheckedTransition,
};
use super::super::diagnostic::{Error, Result};
use super::super::{STATIC_RUNTIME_DISPATCH_LIMIT, STATIC_RUNTIME_PROCESS_LIMIT};

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
    let mut spawned_refs = BTreeSet::new();

    for action in transition.actions() {
        match action {
            CheckedAction::Emit { .. } => {}
            CheckedAction::Spawn {
                target,
                process_ref,
            } => {
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
                let declared_target = process_ref_target(process, *process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn process reference id {} targets process id {}, expected {}",
                        process.debug_name(),
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                if !spawned_refs.insert(*process_ref) {
                    return Err(Error::new(format!(
                        "process {} duplicates process reference id {} within message transition {}",
                        process.debug_name(),
                        process_ref.as_u32(),
                        transition.message().as_u32()
                    )));
                }
            }
            CheckedAction::Send { target, message } => {
                let target_process_id = process_ref_target(process, *target)?;
                if !spawned_refs.contains(target) {
                    return Err(Error::new(format!(
                        "process {} sends through unbound process reference id {} within message transition {}",
                        process.debug_name(),
                        target.as_u32(),
                        transition.message().as_u32()
                    )));
                }
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

fn process_ref_target(
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

    fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticProcessInstance {
    pid: StaticProcessId,
    process_id: CheckedProcessId,
    status: StaticProcessStatus,
    mailbox: VecDeque<CheckedMessageId>,
}

fn bind_static_process_ref(
    process: &CheckedProcess,
    process_refs: &mut BTreeMap<CheckedProcessRefId, StaticProcessId>,
    process_ref: CheckedProcessRefId,
    pid: StaticProcessId,
) -> Result<()> {
    process_ref_target(process, process_ref)?;
    if process_refs.insert(process_ref, pid).is_some() {
        return Err(Error::new(format!(
            "rebinds process reference id {}",
            process_ref.as_u32()
        )));
    }
    Ok(())
}

fn resolve_static_process_ref(
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    process_ref: CheckedProcessRefId,
) -> Result<StaticProcessId> {
    process_ref_target(process, process_ref)?;
    process_refs.get(&process_ref).copied().ok_or_else(|| {
        Error::new(format!(
            "sends to unbound process reference id {}",
            process_ref.as_u32()
        ))
    })
}

fn static_process_index_for_pid(
    instances: &[StaticProcessInstance],
    pid: StaticProcessId,
) -> Result<usize> {
    let raw_index = pid
        .as_u32()
        .checked_sub(1)
        .ok_or_else(|| Error::new("static runtime process id index underflowed"))?;
    let process_index = usize::try_from(raw_index).map_err(|_| {
        Error::new(format!(
            "static runtime process id {} cannot be indexed on this platform",
            pid.as_u32()
        ))
    })?;
    let instance = instances.get(process_index).ok_or_else(|| {
        Error::new(format!(
            "static runtime process id {} is not spawned",
            pid.as_u32()
        ))
    })?;
    if instance.pid != pid {
        return Err(Error::new(format!(
            "static runtime process index for pid {} is inconsistent",
            pid.as_u32()
        )));
    }
    Ok(process_index)
}

fn ensure_static_process_capacity(instance_count: usize) -> Result<()> {
    if instance_count >= STATIC_RUNTIME_PROCESS_LIMIT {
        return Err(Error::new(format!(
            "static runtime process instance limit exceeded at {STATIC_RUNTIME_PROCESS_LIMIT} process instance(s)"
        )));
    }
    Ok(())
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
        let mut local_process_refs = BTreeMap::new();

        for action in transition.actions() {
            match action {
                CheckedAction::Emit { .. } => {}
                CheckedAction::Spawn {
                    target,
                    process_ref,
                } => {
                    process_by_id(processes, *target)?;
                    ensure_static_process_capacity(instances.len())?;
                    let spawned_pid = next_pid;
                    next_pid = next_pid.checked_next()?;
                    bind_static_process_ref(
                        process,
                        &mut local_process_refs,
                        *process_ref,
                        spawned_pid,
                    )
                    .map_err(|err| Error::new(format!("process {} {err}", process.debug_name())))?;
                    instances.push(StaticProcessInstance {
                        pid: spawned_pid,
                        process_id: *target,
                        status: StaticProcessStatus::Running,
                        mailbox: VecDeque::new(),
                    });
                }
                CheckedAction::Send { target, message } => {
                    let target_pid =
                        resolve_static_process_ref(process, &local_process_refs, *target).map_err(
                            |err| Error::new(format!("process {} {err}", process.debug_name())),
                        )?;
                    let target_index = static_process_index_for_pid(&instances, target_pid)
                        .map_err(|err| {
                            Error::new(format!(
                                "process {} sends through process reference id {} to {err}",
                                process.debug_name(),
                                target.as_u32()
                            ))
                        })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::ast::{Identifier, TypeRef};
    use crate::language::checked::{CheckedProcessParts, CheckedProcessRef, CheckedStateId};

    #[test]
    fn static_process_refs_bind_sparsely_within_transition_scope() {
        let process = checked_process_with_declared_refs(2);
        let mut process_refs = BTreeMap::new();
        let process_ref = checked_process_ref_id(1);
        let pid = StaticProcessId::FIRST
            .checked_next()
            .expect("next static pid should exist");

        bind_static_process_ref(&process, &mut process_refs, process_ref, pid)
            .expect("declared process reference should bind");

        assert_eq!(process_refs.len(), 1);
        assert_eq!(
            resolve_static_process_ref(&process, &process_refs, process_ref)
                .expect("bound sparse process reference should resolve"),
            pid
        );
        let err = resolve_static_process_ref(&process, &process_refs, checked_process_ref_id(0))
            .expect_err("declared but unbound sparse process reference should fail");
        assert!(err
            .to_string()
            .contains("sends to unbound process reference id 0"));
    }

    #[test]
    fn static_process_lookup_indexes_by_pid() {
        let instances = vec![
            StaticProcessInstance {
                pid: StaticProcessId::FIRST,
                process_id: checked_process_id(0),
                status: StaticProcessStatus::Running,
                mailbox: VecDeque::new(),
            },
            StaticProcessInstance {
                pid: StaticProcessId::FIRST
                    .checked_next()
                    .expect("next static pid should exist"),
                process_id: checked_process_id(1),
                status: StaticProcessStatus::Running,
                mailbox: VecDeque::new(),
            },
        ];

        assert_eq!(
            static_process_index_for_pid(&instances, StaticProcessId::FIRST)
                .expect("first static pid should resolve"),
            0
        );
        assert_eq!(
            static_process_index_for_pid(&instances, instances[1].pid)
                .expect("second static pid should resolve"),
            1
        );
    }

    #[test]
    fn static_process_lookup_rejects_unspawned_pid() {
        let instances = vec![StaticProcessInstance {
            pid: StaticProcessId::FIRST,
            process_id: checked_process_id(0),
            status: StaticProcessStatus::Running,
            mailbox: VecDeque::new(),
        }];
        let missing_pid = StaticProcessId::FIRST
            .checked_next()
            .expect("next static pid should exist");

        let err = static_process_index_for_pid(&instances, missing_pid)
            .expect_err("unspawned static pid should be rejected");

        assert!(err
            .to_string()
            .contains("static runtime process id 2 is not spawned"));
    }

    #[test]
    fn static_process_capacity_rejects_instance_limit() {
        ensure_static_process_capacity(STATIC_RUNTIME_PROCESS_LIMIT - 1)
            .expect("capacity should allow the final process slot");

        let err = ensure_static_process_capacity(STATIC_RUNTIME_PROCESS_LIMIT)
            .expect_err("capacity should reject a new process beyond the limit");

        assert!(err.to_string().contains(
            "static runtime process instance limit exceeded at 10000 process instance(s)"
        ));
    }

    fn checked_process_with_declared_refs(process_ref_count: usize) -> CheckedProcess {
        CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: TypeRef::Named(ident("MainState")),
            state_values: vec!["MainState".to_string()],
            message_type: TypeRef::Named(ident("MainMsg")),
            message_variants: vec![ident("Start")],
            process_refs: (0..process_ref_count)
                .map(|index| {
                    CheckedProcessRef::new(ident(&format!("worker_{index}")), checked_process_id(1))
                })
                .collect(),
            mailbox_bound: 1,
            init_state: CheckedStateId::from_index(0).expect("valid checked state id"),
            transitions: Vec::new(),
        })
    }

    fn ident(value: &str) -> Identifier {
        Identifier::new(value).expect("test identifier should be valid")
    }

    fn checked_process_id(index: usize) -> CheckedProcessId {
        CheckedProcessId::from_index(index).expect("valid checked process id")
    }

    fn checked_process_ref_id(index: usize) -> CheckedProcessRefId {
        CheckedProcessRefId::from_index(index).expect("valid checked process reference id")
    }
}
