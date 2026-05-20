use std::collections::BTreeSet;

use mantle_artifact::ArtifactValue;

use crate::language::checked::{
    CheckedMessageId, CheckedPayloadValue, CheckedProcess, CheckedStateId, CheckedTypeId,
};
use crate::language::diagnostic::{Error, Result};

type TransitionPayloadGuardKey = Option<(CheckedTypeId, ArtifactValue)>;
type TransitionCoverageKey = (
    CheckedMessageId,
    Option<CheckedStateId>,
    TransitionPayloadGuardKey,
);

pub(super) fn validate_transition_coverage(process: &CheckedProcess) -> Result<()> {
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
