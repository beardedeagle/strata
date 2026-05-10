use std::collections::{BTreeMap, VecDeque};

use mantle_artifact::{
    project_canonical_list_element, project_canonical_map_value, project_canonical_record_field,
    validate_state_value_label,
};

use super::super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedPayloadValue, CheckedProcess, CheckedProcessId,
    CheckedProcessRefId, CheckedSendTarget, CheckedStateId, CheckedStepResult, CheckedTransition,
    CheckedValueTemplate,
};
use super::super::super::diagnostic::{Error, Result};
use super::super::super::{STATIC_RUNTIME_DISPATCH_LIMIT, STATIC_RUNTIME_PROCESS_LIMIT};
use super::process_refs::{process_by_id, process_label, process_ref_target};
use super::templates::resolve_checked_next_state;

fn evaluate_checked_runtime_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
) -> Result<CheckedPayloadValue> {
    match template {
        CheckedValueTemplate::Literal(value) => Ok(value.clone()),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "received payload has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            Ok(payload.clone())
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            let payload = current_state_payload.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "current state payload has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            Ok(payload.clone())
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_checked_runtime_template(
                record,
                received_payload,
                current_state_payload,
                process,
                process_refs,
            )?;
            let label = project_canonical_record_field(record.label(), field.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
            )?;
            let label = project_canonical_list_element(list.label(), *index, *len)
                .map_err(|err| Error::new(err.to_string()))?;
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::MapValue { ty, map, key, keys } => {
            let map = evaluate_checked_runtime_template(
                map,
                received_payload,
                current_state_payload,
                process,
                process_refs,
            )?;
            let label = project_canonical_map_value(map.label(), key, keys)
                .map_err(|err| Error::new(err.to_string()))?;
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
        } => {
            let pid = resolve_static_process_ref(process, process_refs, *process_ref)?;
            Ok(CheckedPayloadValue::process_ref(
                ty.clone(),
                format!("{ty}#{}", pid.as_u32()),
                *target,
                u64::from(pid.as_u32()),
            ))
        }
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_checked_runtime_template(
                payload,
                received_payload,
                current_state_payload,
                process,
                process_refs,
            )?;
            let label = format!("{variant}({})", payload.label());
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::Record { ty, fields } => {
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = evaluate_checked_runtime_template(
                    field.value(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                )?;
                parts.push(format!("{}:{}", field.name(), value.label()));
            }
            let label = format!("{ty}{{{}}}", parts.join(","));
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::List { ty, items } => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                let value = evaluate_checked_runtime_template(
                    item,
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                )?;
                parts.push(value.label().to_string());
            }
            let label = format!("List[{}]", parts.join(","));
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
        CheckedValueTemplate::Map { ty, entries } => {
            let mut parts = BTreeMap::new();
            for entry in entries {
                let key = evaluate_checked_runtime_template(
                    entry.key(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                )?;
                let value = evaluate_checked_runtime_template(
                    entry.value(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                )?;
                if parts
                    .insert(key.label().to_string(), value.label().to_string())
                    .is_some()
                {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key.label()
                    )));
                }
            }
            let label = format!(
                "Map[{}]",
                parts
                    .into_iter()
                    .map(|(key, value)| format!("{key}=>{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StaticProcessStatus {
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct StaticProcessId(u32);

impl StaticProcessId {
    pub(super) const FIRST: Self = Self(1);

    pub(super) fn checked_next(self) -> Result<Self> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or_else(|| Error::new("static runtime process id overflowed"))
    }

    pub(super) fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaticProcessInstance {
    pub(super) pid: StaticProcessId,
    pub(super) process_id: CheckedProcessId,
    pub(super) state: CheckedStateId,
    pub(super) status: StaticProcessStatus,
    pub(super) mailbox: VecDeque<StaticMessageEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaticMessageEnvelope {
    message: CheckedMessageId,
    payload: Option<CheckedPayloadValue>,
}

impl StaticMessageEnvelope {
    fn new(message: CheckedMessageId, payload: Option<CheckedPayloadValue>) -> Self {
        Self { message, payload }
    }
}

pub(super) fn bind_static_process_ref(
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

pub(super) fn resolve_static_process_ref(
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

fn resolve_static_send_target(
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    target: &CheckedSendTarget,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<StaticProcessId> {
    match target {
        CheckedSendTarget::ProcessRef(process_ref) => {
            resolve_static_process_ref(process, process_refs, *process_ref)
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

pub(super) fn static_process_index_for_pid(
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

pub(super) fn ensure_static_process_capacity(instance_count: usize) -> Result<()> {
    if instance_count >= STATIC_RUNTIME_PROCESS_LIMIT {
        return Err(Error::new(format!(
            "static runtime process instance limit exceeded at {STATIC_RUNTIME_PROCESS_LIMIT} process instance(s)"
        )));
    }
    Ok(())
}

pub(super) fn validate_static_runtime_order(
    processes: &[CheckedProcess],
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
) -> Result<()> {
    let entry_definition = process_by_id(processes, entry_process)?;
    let mut instances = vec![StaticProcessInstance {
        pid: StaticProcessId::FIRST,
        process_id: entry_process,
        state: entry_definition.init_state(),
        status: StaticProcessStatus::Running,
        mailbox: VecDeque::from([StaticMessageEnvelope::new(entry_message, None)]),
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
        let envelope = instances[process_index]
            .mailbox
            .pop_front()
            .ok_or_else(|| Error::new("static runtime mailbox changed during dequeue"))?;
        let current_state = instances[process_index].state;
        let current_state_payload = process
            .state_values()
            .get(current_state.index())
            .and_then(|state| state.payload());
        let transition = transition_for_message(process, envelope.message, current_state)?;
        let final_state = resolve_checked_next_state(
            process,
            current_state,
            transition.next_state(),
            envelope.payload.as_ref(),
        )?;
        let mut local_process_refs = BTreeMap::new();

        for action in transition.actions() {
            match action {
                CheckedAction::Emit { .. } => {}
                CheckedAction::Spawn {
                    target,
                    process_ref,
                } => {
                    let target_process = process_by_id(processes, *target)?;
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
                        state: target_process.init_state(),
                        status: StaticProcessStatus::Running,
                        mailbox: VecDeque::new(),
                    });
                }
                CheckedAction::Send {
                    target,
                    message,
                    payload,
                } => {
                    let target_pid = resolve_static_send_target(
                        process,
                        &local_process_refs,
                        target,
                        envelope.payload.as_ref(),
                    )
                    .map_err(|err| Error::new(format!("process {} {err}", process.debug_name())))?;
                    let target_index = static_process_index_for_pid(&instances, target_pid)
                        .map_err(|err| {
                            Error::new(format!(
                                "process {} sends through process reference to {err}",
                                process.debug_name()
                            ))
                        })?;
                    let target_process =
                        process_by_id(processes, instances[target_index].process_id)?;
                    if message.index() >= target_process.message_cases().len() {
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
                    let payload = match payload {
                        Some(payload) => Some(evaluate_checked_runtime_template(
                            payload,
                            envelope.payload.as_ref(),
                            current_state_payload,
                            process,
                            &local_process_refs,
                        )?),
                        None => None,
                    };
                    instances[target_index]
                        .mailbox
                        .push_back(StaticMessageEnvelope::new(*message, payload));
                }
            }
        }

        instances[process_index].state = final_state;
        match transition.step_result() {
            CheckedStepResult::Continue => {}
            CheckedStepResult::Stop => {
                instances[process_index].status = StaticProcessStatus::Stopped;
            }
            CheckedStepResult::Panic => {
                instances[process_index].status = StaticProcessStatus::Failed;
                return Ok(());
            }
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
    current_state: CheckedStateId,
) -> Result<&CheckedTransition> {
    process
        .transitions()
        .iter()
        .find(|transition| {
            transition.message() == message && transition.current_state() == Some(current_state)
        })
        .or_else(|| {
            process.transitions().iter().find(|transition| {
                transition.message() == message && transition.current_state().is_none()
            })
        })
        .ok_or_else(|| {
            Error::new(format!(
                "process {} has no transition for message id {} current_state id {}",
                process.debug_name(),
                message.as_u32(),
                current_state.as_u32()
            ))
        })
}
