use std::collections::BTreeSet;

use super::super::checked::{
    CheckedAction, CheckedMessageId, CheckedPayloadValue, CheckedProcess, CheckedProcessId,
    CheckedProcessRefId, CheckedStateId, CheckedTransition, CheckedTypeId, CheckedValueTemplate,
};
use super::super::diagnostic::{Error, Result};
use mantle_artifact::ArtifactValue;

mod process_refs;
mod runtime_order;
mod templates;

use process_refs::{
    message_payload_type, process_by_id, process_label, process_ref_target, validate_send_target,
};
use runtime_order::validate_static_runtime_order;
use templates::{
    current_state_payload_type, validate_next_state, validate_value_template_binding_types,
    validate_value_template_payload_labels, validate_value_template_process_refs,
};

type TransitionPayloadGuardKey = Option<(CheckedTypeId, ArtifactValue)>;
type TransitionCoverageKey = (
    CheckedMessageId,
    Option<CheckedStateId>,
    TransitionPayloadGuardKey,
);

pub(super) fn validate_action_references(
    processes: &[CheckedProcess],
    entry_process: &CheckedProcessId,
    entry_message: &CheckedMessageId,
) -> Result<()> {
    for (process_index, process) in processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(process_index)?;
        validate_checked_state_table(process)?;
        for transition in process.transitions() {
            validate_transition(processes, process, process_id, *entry_process, transition)?;
        }
        validate_transition_coverage(process)?;
    }
    validate_static_runtime_order(processes, *entry_process, *entry_message)?;
    Ok(())
}

fn validate_checked_state_table(process: &CheckedProcess) -> Result<()> {
    if process.state_values().is_empty() {
        return Err(Error::new(format!(
            "process {} state_value_count must be greater than zero",
            process.debug_name()
        )));
    }
    if process.init_state().index() >= process.state_values().len() {
        return Err(Error::new(format!(
            "process {} init_state id {} is not a valid state value",
            process.debug_name(),
            process.init_state().as_u32()
        )));
    }

    let mut states = BTreeSet::new();
    for state in process.state_values() {
        if state.ty() != process.state_type() {
            return Err(Error::new(format!(
                "process {} state value {} has type {}, expected {}",
                process.debug_name(),
                state.label(),
                state.ty(),
                process.state_type()
            )));
        }
        state
            .value()
            .validate("state value")
            .map_err(|err| Error::new(err.to_string()))?;
        if state.value().contains_process_ref() {
            return Err(Error::new(format!(
                "process {} state value {} carries a process reference value",
                process.debug_name(),
                state.label()
            )));
        }
        if let Some(payload) = state.payload() {
            if payload.process_ref_payload().is_some() {
                return Err(Error::new(format!(
                    "process {} state value {} carries a process reference payload",
                    process.debug_name(),
                    state.label()
                )));
            }
            let value = payload.value().ok_or_else(|| {
                Error::new(format!(
                    "process {} state value {} carries a process reference payload",
                    process.debug_name(),
                    state.label()
                ))
            })?;
            value
                .validate("state value payload")
                .map_err(|err| Error::new(err.to_string()))?;
            if value.contains_process_ref() {
                return Err(Error::new(format!(
                    "process {} state value {} carries a process reference payload",
                    process.debug_name(),
                    state.label()
                )));
            }
        }
        if !states.insert((state.ty().id(), state.value().clone())) {
            return Err(Error::new(format!(
                "process {} declares duplicate state value {}",
                process.debug_name(),
                state.label()
            )));
        }
    }
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
    validate_transition_current_state(process, transition)?;
    validate_next_state(
        process,
        transition.message(),
        transition.current_state(),
        transition.next_state(),
    )?;
    validate_transition_payload_guard(process, transition)?;
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
                    transition,
                    target_process,
                    *message,
                    payload.as_deref(),
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

fn validate_transition_payload_guard(
    process: &CheckedProcess,
    transition: &CheckedTransition,
) -> Result<()> {
    let Some(payload_guard) = transition.payload_guard() else {
        return Ok(());
    };
    if payload_guard.process_ref_payload().is_some() || payload_guard.value().is_none() {
        return Err(Error::new(format!(
            "process {} transition message id {} payload guard cannot be a process reference payload",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    let Some(expected_type) = message_payload_type(process, transition.message())? else {
        return Err(Error::new(format!(
            "process {} transition message id {} has a payload guard, but the message accepts no payload",
            process.debug_name(),
            transition.message().as_u32()
        )));
    };
    if payload_guard.ty() != expected_type {
        return Err(Error::new(format!(
            "process {} transition message id {} payload guard has type {}, expected {}",
            process.debug_name(),
            transition.message().as_u32(),
            payload_guard.ty(),
            expected_type
        )));
    }
    let value = payload_guard.value().ok_or_else(|| {
        Error::new(format!(
            "process {} transition message id {} payload guard cannot be a process reference payload",
            process.debug_name(),
            transition.message().as_u32()
        ))
    })?;
    value
        .validate("transition payload guard")
        .map_err(|err| Error::new(err.to_string()))?;
    if value.contains_process_ref() {
        return Err(Error::new(format!(
            "process {} transition message id {} payload guard contains a process reference value",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    Ok(())
}

fn validate_send_payload_shape(
    process: &CheckedProcess,
    transition: &CheckedTransition,
    target_process: &CheckedProcess,
    target_message: CheckedMessageId,
    payload: Option<&CheckedValueTemplate>,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
    processes: &[CheckedProcess],
) -> Result<()> {
    let current_payload_type = message_payload_type(process, transition.message())?;
    let current_state_payload_type =
        current_state_payload_type(process, transition.current_state())?;
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
            validate_value_template_binding_types(
                payload,
                current_payload_type,
                current_state_payload_type,
            )?;
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

fn validate_transition_current_state(
    process: &CheckedProcess,
    transition: &CheckedTransition,
) -> Result<()> {
    if let Some(current_state) = transition.current_state() {
        if current_state.index() >= process.state_values().len() {
            return Err(Error::new(format!(
                "process {} message id {} current_state id {} is not a valid state value",
                process.debug_name(),
                transition.message().as_u32(),
                current_state.as_u32()
            )));
        }
    }
    Ok(())
}

fn validate_transition_coverage(process: &CheckedProcess) -> Result<()> {
    let mut declared: BTreeSet<TransitionCoverageKey> = BTreeSet::new();
    for transition in process.transitions() {
        let key = (
            transition.message(),
            transition.current_state(),
            transition_payload_guard_key(transition.payload_guard())?,
        );
        if !declared.insert(key) {
            return Err(Error::new(format!(
                "process {} declares duplicate transition for message id {} current_state {:?} payload_guard {}",
                process.debug_name(),
                transition.message().as_u32(),
                transition.current_state().map(CheckedStateId::as_u32),
                transition_payload_guard_label(transition.payload_guard())
            )));
        }
    }

    for (message, current_state, payload_guard) in &declared {
        let base_has_payload_guard =
            declared
                .iter()
                .any(|(other_message, other_state, other_payload_guard)| {
                    other_message == message
                        && other_state == current_state
                        && other_payload_guard.is_some()
                });
        if base_has_payload_guard && payload_guard.is_none() {
            return Err(Error::new(format!(
                "process {} mixes payload-guarded and unguarded transitions for message id {} current_state {:?}",
                process.debug_name(),
                message.as_u32(),
                current_state.map(CheckedStateId::as_u32)
            )));
        }
    }

    for message_index in 0..process.message_cases().len() {
        let message = CheckedMessageId::from_index(message_index)?;
        let mut has_unguarded = false;
        let mut guarded_states = BTreeSet::new();

        for (_, current_state, _) in declared
            .iter()
            .filter(|(transition_message, _, _)| *transition_message == message)
        {
            match current_state {
                Some(current_state) => {
                    guarded_states.insert(*current_state);
                }
                None => {
                    has_unguarded = true;
                }
            }
        }

        if has_unguarded {
            if !guarded_states.is_empty() {
                return Err(Error::new(format!(
                    "process {} mixes unguarded and state-specific transitions for message id {}",
                    process.debug_name(),
                    message.as_u32()
                )));
            }
            continue;
        }

        if guarded_states.is_empty() {
            return Err(Error::new(format!(
                "process {} has no transition for message id {}",
                process.debug_name(),
                message.as_u32()
            )));
        }

        for state_index in 0..process.state_values().len() {
            let state = CheckedStateId::from_index(state_index)?;
            if !guarded_states.contains(&state) {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {} current_state id {}",
                    process.debug_name(),
                    message.as_u32(),
                    state.as_u32()
                )));
            }
        }
    }

    Ok(())
}

fn transition_payload_guard_key(
    payload_guard: Option<&CheckedPayloadValue>,
) -> Result<TransitionPayloadGuardKey> {
    payload_guard
        .map(|payload_guard| {
            payload_guard
                .value()
                .cloned()
                .map(|value| (payload_guard.ty().id(), value))
                .ok_or_else(|| {
                    Error::new("transition payload guard cannot be a process reference payload")
                })
        })
        .transpose()
}

fn transition_payload_guard_label(payload_guard: Option<&CheckedPayloadValue>) -> String {
    payload_guard
        .map(|payload_guard| payload_guard.label().to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

#[cfg(test)]
mod tests;
