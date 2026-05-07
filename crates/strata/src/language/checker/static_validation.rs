use std::collections::{BTreeMap, BTreeSet, VecDeque};

use mantle_artifact::{validate_payload_value_label, validate_state_value_label};

use super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedPayloadValue, CheckedProcess,
    CheckedProcessId, CheckedProcessRefId, CheckedSendTarget, CheckedStateId, CheckedStepResult,
    CheckedTransition, CheckedTypeKind, CheckedTypeRef, CheckedValueTemplate,
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
    if transition.message().index() >= process.message_cases().len() {
        return Err(Error::new(format!(
            "process {} transition message id {} is not accepted",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    validate_next_state(process, transition.message(), transition.next_state())?;
    validate_transition_effects(process, transition)?;
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
            CheckedAction::Send {
                target,
                message,
                payload,
            } => {
                let target_process_id = validate_send_target(
                    processes,
                    process,
                    transition.message(),
                    target,
                    &spawned_refs,
                )?;
                let target_process = process_by_id(processes, target_process_id)?;
                if message.index() >= target_process.message_cases().len() {
                    return Err(Error::new(format!(
                        "process {} sends message id {} not accepted by {}",
                        process.debug_name(),
                        message.as_u32(),
                        target_process.debug_name()
                    )));
                }
                validate_send_payload_shape(
                    process,
                    transition.message(),
                    target_process,
                    *message,
                    payload.as_ref(),
                    &spawned_refs,
                    processes,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_transition_effects(
    process: &CheckedProcess,
    transition: &CheckedTransition,
) -> Result<()> {
    let mut declared_effects = BTreeSet::new();
    for &effect in transition.effects() {
        if !declared_effects.insert(effect) {
            return Err(Error::new(format!(
                "process {} transition {} declares duplicate effect {effect}",
                process.debug_name(),
                transition.message().as_u32()
            )));
        }
    }

    let mut used_effects = BTreeSet::new();
    for action in transition.actions() {
        let effect = action.effect();
        if !declared_effects.contains(&effect) {
            return Err(Error::new(format!(
                "process {} transition {} uses effect {effect} but does not declare it",
                process.debug_name(),
                transition.message().as_u32()
            )));
        }
        used_effects.insert(effect);
    }

    for effect in &declared_effects {
        if !used_effects.contains(effect) {
            return Err(Error::new(format!(
                "process {} transition {} declares effect {effect} but no action uses it",
                process.debug_name(),
                transition.message().as_u32()
            )));
        }
    }
    Ok(())
}

fn validate_send_payload_shape(
    process: &CheckedProcess,
    current_message: CheckedMessageId,
    target_process: &CheckedProcess,
    target_message: CheckedMessageId,
    payload: Option<&CheckedValueTemplate>,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
    processes: &[CheckedProcess],
) -> Result<()> {
    let current_payload_type = message_payload_type(process, current_message)?;
    let target_payload_type = message_payload_type(target_process, target_message)?;
    match (target_payload_type, payload) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(Error::new(format!(
            "process {} sends payload to message id {}, which does not accept one",
            process.debug_name(),
            target_message.as_u32()
        ))),
        (Some(_), None) => Err(Error::new(format!(
            "process {} sends message id {} without required payload",
            process.debug_name(),
            target_message.as_u32()
        ))),
        (Some(expected_type), Some(payload)) => {
            validate_value_template_received_type(payload, current_payload_type)?;
            validate_value_template_payload_labels(payload)?;
            validate_value_template_process_refs(processes, process, payload, spawned_refs, true)?;
            if payload.result_type() != expected_type {
                return Err(Error::new(format!(
                    "process {} sends payload of type {}, expected {}",
                    process.debug_name(),
                    payload.result_type(),
                    expected_type
                )));
            }
            Ok(())
        }
    }
}

fn validate_next_state(
    process: &CheckedProcess,
    current_message: CheckedMessageId,
    next_state: CheckedNextState,
) -> Result<()> {
    match next_state {
        CheckedNextState::Current => Ok(()),
        CheckedNextState::Value(state) => {
            if state.index() >= process.state_values().len() {
                return Err(Error::new(format!(
                    "process {} next_state id {} is not a valid state value",
                    process.debug_name(),
                    state.as_u32()
                )));
            }
            Ok(())
        }
        CheckedNextState::Template(template) => {
            if template.result_type() != process.state_type() {
                return Err(Error::new(format!(
                    "process {} next_state template has type {}, expected {}",
                    process.debug_name(),
                    template.result_type(),
                    process.state_type()
                )));
            }
            validate_value_template_received_type(
                &template,
                message_payload_type(process, current_message)?,
            )?;
            validate_value_template_payload_labels(&template)?;
            reject_process_ref_template_in_next_state(&template)?;
            if !checked_template_depends_on_received_payload(&template) {
                resolve_checked_template_state(process, &template, None)?;
            }
            Ok(())
        }
    }
}

fn message_payload_type(
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

fn validate_value_template_received_type(
    template: &CheckedValueTemplate,
    received_payload_type: Option<&CheckedTypeRef>,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_) => Ok(()),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            let Some(received_payload_type) = received_payload_type else {
                return Err(Error::new(
                    "received payload template requires a payload-bearing message",
                ));
            };
            if ty != received_payload_type {
                return Err(Error::new(format!(
                    "received payload template has type {}, expected {}",
                    ty, received_payload_type
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_received_type(field.value(), received_payload_type)?;
            }
            Ok(())
        }
    }
}

fn validate_value_template_payload_labels(template: &CheckedValueTemplate) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(value) => {
            validate_payload_value_label(value.label()).map_err(|err| Error::new(err.to_string()))
        }
        CheckedValueTemplate::ReceivedPayload { .. } => Ok(()),
        CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_payload_labels(field.value())?;
            }
            Ok(())
        }
    }
}

fn validate_value_template_process_refs(
    processes: &[CheckedProcess],
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
    allow_direct_process_ref: bool,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_) | CheckedValueTemplate::ReceivedPayload { .. } => Ok(()),
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
            ..
        } => {
            if !allow_direct_process_ref {
                return Err(Error::new(
                    "process reference payload templates must be direct message payloads",
                ));
            }
            validate_process_ref_type_target(processes, ty, *target)?;
            let declared_target = process_ref_target(process, *process_ref)?;
            if declared_target != *target {
                return Err(Error::new(format!(
                    "process {} process reference payload id {} targets process id {}, expected {}",
                    process.debug_name(),
                    process_ref.as_u32(),
                    declared_target.as_u32(),
                    target.as_u32()
                )));
            }
            if !spawned_refs.contains(process_ref) {
                return Err(Error::new(format!(
                    "process {} sends unbound process reference id {} as payload",
                    process.debug_name(),
                    process_ref.as_u32()
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_process_refs(
                    processes,
                    process,
                    field.value(),
                    spawned_refs,
                    false,
                )?;
            }
            Ok(())
        }
    }
}

fn reject_process_ref_template_in_next_state(template: &CheckedValueTemplate) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_) | CheckedValueTemplate::ReceivedPayload { .. } => Ok(()),
        CheckedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference templates are not valid next-state values",
        )),
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                reject_process_ref_template_in_next_state(field.value())?;
            }
            Ok(())
        }
    }
}

fn checked_template_depends_on_received_payload(template: &CheckedValueTemplate) -> bool {
    match template {
        CheckedValueTemplate::Literal(_) => false,
        CheckedValueTemplate::ReceivedPayload { .. } => true,
        CheckedValueTemplate::ProcessRef { .. } => false,
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_received_payload(field.value())),
    }
}

fn evaluate_checked_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
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
        CheckedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference template requires static runtime process reference bindings",
        )),
        CheckedValueTemplate::Record { ty, fields } => {
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = evaluate_checked_template(field.value(), received_payload)?;
                parts.push(format!("{}:{}", field.name(), value.label()));
            }
            let label = format!("{ty}{{{}}}", parts.join(","));
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
    }
}

fn evaluate_checked_runtime_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
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
        CheckedValueTemplate::Record { ty, fields } => {
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = evaluate_checked_runtime_template(
                    field.value(),
                    received_payload,
                    process,
                    process_refs,
                )?;
                parts.push(format!("{}:{}", field.name(), value.label()));
            }
            let label = format!("{ty}{{{}}}", parts.join(","));
            validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), label))
        }
    }
}

fn resolve_checked_template_state(
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedStateId> {
    let value = evaluate_checked_template(template, received_payload)?;
    let state_index = process
        .state_values()
        .iter()
        .position(|state| state.has_same_identity_as_payload(&value))
        .ok_or_else(|| {
            Error::new(format!(
                "process {} next_state template produced value {} not admitted by state table",
                process.debug_name(),
                value.label()
            ))
        })?;
    CheckedStateId::from_index(state_index)
}

fn resolve_checked_next_state(
    process: &CheckedProcess,
    current_state: CheckedStateId,
    next_state: CheckedNextState,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedStateId> {
    match next_state {
        CheckedNextState::Current => Ok(current_state),
        CheckedNextState::Value(state) => Ok(state),
        CheckedNextState::Template(template) => {
            resolve_checked_template_state(process, &template, received_payload)
        }
    }
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

fn validate_send_target(
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

fn validate_process_ref_type_target(
    processes: &[CheckedProcess],
    ty: &CheckedTypeRef,
    target: CheckedProcessId,
) -> Result<()> {
    let expected_process = process_by_id(processes, target)?;
    match ty.kind() {
        CheckedTypeKind::ProcessRef {
            target: type_target,
        } if type_target == target => Ok(()),
        CheckedTypeKind::ProcessRef {
            target: type_target,
        } => {
            let type_process = process_by_id(processes, type_target)?;
            let type_name = checked_type_diagnostic(processes, ty)?;
            Err(Error::new(format!(
                "process reference payload type {type_name} targets {} (process id {}), expected {} (process id {})",
                type_process.debug_name(),
                type_target.as_u32(),
                expected_process.debug_name(),
                target.as_u32()
            )))
        }
        CheckedTypeKind::Value => {
            let type_name = checked_type_diagnostic(processes, ty)?;
            Err(Error::new(format!(
                "process reference payload type {type_name} must be a process reference type"
            )))
        }
    }
}

fn checked_type_diagnostic(processes: &[CheckedProcess], ty: &CheckedTypeRef) -> Result<String> {
    match ty.kind() {
        CheckedTypeKind::Value => Ok(ty.label().to_string()),
        CheckedTypeKind::ProcessRef { target } => {
            let process = process_by_id(processes, target)?;
            Ok(format!("ProcessRef<{}>", process.debug_name()))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticProcessStatus {
    Running,
    Stopped,
    Failed,
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
    state: CheckedStateId,
    status: StaticProcessStatus,
    mailbox: VecDeque<StaticMessageEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticMessageEnvelope {
    message: CheckedMessageId,
    payload: Option<CheckedPayloadValue>,
}

impl StaticMessageEnvelope {
    fn new(message: CheckedMessageId, payload: Option<CheckedPayloadValue>) -> Self {
        Self { message, payload }
    }
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
        let transition = transition_for_message(process, envelope.message)?;
        let final_state = resolve_checked_next_state(
            process,
            instances[process_index].state,
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
    use crate::language::ast::{Effect, Identifier};
    use crate::language::checked::{
        CheckedMessageCase, CheckedMessageVariantId, CheckedOutputId, CheckedProcessParts,
        CheckedProcessRef, CheckedStateId, CheckedStateValue, CheckedTransitionParts,
        CheckedValueTemplateField,
    };

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
        assert!(
            err.to_string()
                .contains("sends to unbound process reference id 0")
        );
    }

    #[test]
    fn static_process_lookup_indexes_by_pid() {
        let instances = vec![
            StaticProcessInstance {
                pid: StaticProcessId::FIRST,
                process_id: checked_process_id(0),
                state: checked_state_id(0),
                status: StaticProcessStatus::Running,
                mailbox: VecDeque::new(),
            },
            StaticProcessInstance {
                pid: StaticProcessId::FIRST
                    .checked_next()
                    .expect("next static pid should exist"),
                process_id: checked_process_id(1),
                state: checked_state_id(0),
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
            state: checked_state_id(0),
            status: StaticProcessStatus::Running,
            mailbox: VecDeque::new(),
        }];
        let missing_pid = StaticProcessId::FIRST
            .checked_next()
            .expect("next static pid should exist");

        let err = static_process_index_for_pid(&instances, missing_pid)
            .expect_err("unspawned static pid should be rejected");

        assert!(
            err.to_string()
                .contains("static runtime process id 2 is not spawned")
        );
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

    #[test]
    fn static_validation_rejects_next_state_received_payload_template_for_unit_message() {
        let process = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: CheckedStateId::from_index(0).expect("valid checked state id"),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Template(CheckedValueTemplate::ReceivedPayload {
                    ty: value_type("MainState"),
                }),
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err =
            validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
                .expect_err("received payload template on unit message should fail");

        assert!(
            err.to_string()
                .contains("received payload template requires a payload-bearing message")
        );
    }

    #[test]
    fn static_validation_rejects_static_next_state_template_outside_state_table() {
        let process = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Template(CheckedValueTemplate::Literal(
                    CheckedPayloadValue::new(
                        value_type("MainState"),
                        "UnadmittedState".to_string(),
                    ),
                )),
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err =
            validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
                .expect_err("unadmitted static template state should fail");

        assert!(err.to_string().contains(
            "process Main next_state template produced value UnadmittedState not admitted by state table"
        ));
    }

    #[test]
    fn static_validation_rejects_action_without_declared_effect() {
        let process = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: vec![CheckedAction::Emit {
                    output: CheckedOutputId::from_index(0).expect("valid checked output id"),
                }],
            })],
        });

        let err =
            validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
                .expect_err("missing checked transition effect should fail");

        assert!(
            err.to_string()
                .contains("process Main transition 0 uses effect emit but does not declare it")
        );
    }

    #[test]
    fn static_validation_rejects_declared_effect_without_action() {
        let process = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Emit],
                actions: Vec::new(),
            })],
        });

        let err =
            validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
                .expect_err("unused checked transition effect should fail");

        assert!(
            err.to_string()
                .contains("process Main transition 0 declares effect emit but no action uses it")
        );
    }

    #[test]
    fn static_validation_rejects_duplicate_transition_effect() {
        let process = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Emit, Effect::Emit],
                actions: Vec::new(),
            })],
        });

        let err =
            validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
                .expect_err("duplicate checked transition effect should fail");

        assert!(
            err.to_string()
                .contains("process Main transition 0 declares duplicate effect emit")
        );
    }

    #[test]
    fn static_validation_rejects_literal_send_payload_with_invalid_label() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: vec![CheckedProcessRef::new(
                ident("worker"),
                checked_process_id(1),
            )],
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Spawn, Effect::Send],
                actions: vec![
                    CheckedAction::Spawn {
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    },
                    CheckedAction::Send {
                        target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                        message: checked_message_id(0),
                        payload: Some(CheckedValueTemplate::Literal(CheckedPayloadValue::new(
                            value_type("Job"),
                            "Job\n".to_string(),
                        ))),
                    },
                ],
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values("WorkerState", &["WorkerState"]),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Assign".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    Some(value_type("Job")),
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("invalid literal payload label should fail");

        assert!(
            err.to_string()
                .contains("payload value must be non-empty and contain no control characters")
        );
    }

    #[test]
    fn static_validation_rejects_received_payload_send_target_with_non_process_ref_type() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    Some(value_type("Job")),
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Send],
                actions: vec![CheckedAction::Send {
                    target: CheckedSendTarget::ReceivedPayload {
                        ty: value_type("Job"),
                        target: checked_process_id(1),
                    },
                    message: checked_message_id(0),
                    payload: None,
                }],
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values("WorkerState", &["WorkerState"]),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Done".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("non-process-ref received send target should fail");

        assert!(
            err.to_string()
                .contains("process reference payload type Job must be a process reference type")
        );
    }

    #[test]
    fn static_validation_rejects_process_ref_template_with_non_process_ref_type() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: vec![CheckedProcessRef::new(
                ident("worker"),
                checked_process_id(1),
            )],
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Spawn, Effect::Send],
                actions: vec![
                    CheckedAction::Spawn {
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    },
                    CheckedAction::Send {
                        target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                        message: checked_message_id(0),
                        payload: Some(CheckedValueTemplate::ProcessRef {
                            ty: value_type("Job"),
                            target: checked_process_id(1),
                            process_ref: checked_process_ref_id(0),
                        }),
                    },
                ],
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values("WorkerState", &["WorkerState"]),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Assign".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    Some(value_type("Job")),
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("non-process-ref process ref template should fail");

        assert!(
            err.to_string()
                .contains("process reference payload type Job must be a process reference type")
        );
    }

    #[test]
    fn static_validation_formats_process_ref_type_diagnostics_without_internal_labels() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: vec![CheckedProcessRef::new(
                ident("worker"),
                checked_process_id(1),
            )],
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Spawn, Effect::Send],
                actions: vec![
                    CheckedAction::Spawn {
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    },
                    CheckedAction::Send {
                        target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                        message: checked_message_id(0),
                        payload: Some(CheckedValueTemplate::ProcessRef {
                            ty: process_ref_type("Worker"),
                            target: checked_process_id(0),
                            process_ref: checked_process_ref_id(0),
                        }),
                    },
                ],
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values("WorkerState", &["WorkerState"]),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Reply".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    Some(process_ref_type("Worker")),
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("process-ref type target mismatch should fail");
        let message = err.to_string();

        assert!(message.contains(
            "process reference payload type ProcessRef<Worker> targets Worker (process id 1), expected Main (process id 0)"
        ));
        assert!(!message.contains("__strata_checked_process_ref_"));
    }

    #[test]
    fn static_validation_rejects_nested_process_ref_payload_template() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: vec![CheckedProcessRef::new(
                ident("worker"),
                checked_process_id(1),
            )],
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Spawn, Effect::Send],
                actions: vec![
                    CheckedAction::Spawn {
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    },
                    CheckedAction::Send {
                        target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                        message: checked_message_id(0),
                        payload: Some(CheckedValueTemplate::Record {
                            ty: value_type("Box"),
                            fields: vec![CheckedValueTemplateField::new(
                                ident("reply_to"),
                                CheckedValueTemplate::ProcessRef {
                                    ty: process_ref_type("Worker"),
                                    target: checked_process_id(1),
                                    process_ref: checked_process_ref_id(0),
                                },
                            )],
                        }),
                    },
                ],
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values("WorkerState", &["WorkerState"]),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Assign".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    Some(value_type("Box")),
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("nested process ref template should fail");

        assert!(
            err.to_string()
                .contains("process reference payload templates must be direct message payloads")
        );
    }

    #[test]
    fn static_validation_rejects_process_ref_next_state_template() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: vec![CheckedProcessRef::new(
                ident("worker"),
                checked_process_id(1),
            )],
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Template(CheckedValueTemplate::Record {
                    ty: value_type("MainState"),
                    fields: vec![CheckedValueTemplateField::new(
                        ident("reply_to"),
                        CheckedValueTemplate::ProcessRef {
                            ty: process_ref_type("Worker"),
                            target: checked_process_id(1),
                            process_ref: checked_process_ref_id(0),
                        },
                    )],
                }),
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values("WorkerState", &["WorkerState"]),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Done".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("process ref next-state template should fail");

        assert!(
            err.to_string()
                .contains("process reference templates are not valid next-state values")
        );
    }

    #[test]
    fn static_validation_rejects_payload_template_next_state_outside_state_table() {
        let main = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: vec![CheckedProcessRef::new(
                ident("worker"),
                checked_process_id(1),
            )],
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Current,
                effects: vec![Effect::Spawn, Effect::Send],
                actions: vec![
                    CheckedAction::Spawn {
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    },
                    CheckedAction::Send {
                        target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                        message: checked_message_id(0),
                        payload: Some(CheckedValueTemplate::Literal(CheckedPayloadValue::new(
                            value_type("Job"),
                            "Job{phase:Ready}".to_string(),
                        ))),
                    },
                ],
            })],
        });
        let worker = CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Worker"),
            state_type: value_type("WorkerState"),
            state_values: checked_state_values(
                "WorkerState",
                &["WorkerState{active:Job{phase:Done}}"],
            ),
            message_type: value_type("WorkerMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Assign".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    Some(value_type("Job")),
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: vec![CheckedTransition::new(CheckedTransitionParts {
                message: checked_message_id(0),
                step_result: CheckedStepResult::Stop,
                next_state: CheckedNextState::Template(CheckedValueTemplate::Record {
                    ty: value_type("WorkerState"),
                    fields: vec![CheckedValueTemplateField::new(
                        ident("active"),
                        CheckedValueTemplate::ReceivedPayload {
                            ty: value_type("Job"),
                        },
                    )],
                }),
                effects: Vec::new(),
                actions: Vec::new(),
            })],
        });

        let err = validate_action_references(
            &[main, worker],
            &checked_process_id(0),
            &checked_message_id(0),
        )
        .expect_err("unadmitted payload-derived template state should fail");

        assert!(err.to_string().contains(
            "process Worker next_state template produced value WorkerState{active:Job{phase:Ready}} not admitted by state table"
        ));
    }

    fn checked_process_with_declared_refs(process_ref_count: usize) -> CheckedProcess {
        CheckedProcess::new(CheckedProcessParts {
            debug_name: ident("Main"),
            state_type: value_type("MainState"),
            state_values: checked_state_values("MainState", &["MainState"]),
            message_type: value_type("MainMsg"),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
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

    fn value_type(label: &str) -> CheckedTypeRef {
        CheckedTypeRef::test_value(label)
    }

    fn process_ref_type(target: &str) -> CheckedTypeRef {
        let target_process = match target {
            "Worker" => checked_process_id(1),
            other => panic!("test process ref target {other} is not mapped"),
        };
        CheckedTypeRef::test_process_ref(
            &format!("__strata_checked_process_ref_{}", target_process.as_u32()),
            target_process,
        )
    }

    fn checked_state_values(ty: &str, values: &[&str]) -> Vec<CheckedStateValue> {
        values
            .iter()
            .map(|value| CheckedStateValue::new(value_type(ty), (*value).to_string()))
            .collect()
    }

    fn checked_process_id(index: usize) -> CheckedProcessId {
        CheckedProcessId::from_index(index).expect("valid checked process id")
    }

    fn checked_process_ref_id(index: usize) -> CheckedProcessRefId {
        CheckedProcessRefId::from_index(index).expect("valid checked process reference id")
    }

    fn checked_state_id(index: usize) -> CheckedStateId {
        CheckedStateId::from_index(index).expect("valid checked state id")
    }

    fn checked_message_id(index: usize) -> CheckedMessageId {
        CheckedMessageId::from_index(index).expect("valid checked message id")
    }
}
