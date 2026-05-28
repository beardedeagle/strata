use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactProcess, Error, MessageId, Result, StateId};

use super::{LoadedProcess, LoadedTransition, RuntimePayload};

type TransitionBaseKey = (u32, Option<u32>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TransitionCoverageKey {
    message: u32,
    current_state: Option<u32>,
    payload_guarded: bool,
}

#[derive(Debug, Clone)]
pub(super) struct TransitionLookup {
    unguarded_by_key: BTreeMap<TransitionBaseKey, usize>,
    guarded_indexes: Vec<usize>,
    state_specific_messages: BTreeSet<u32>,
    payload_specific_transitions: BTreeSet<TransitionBaseKey>,
}

impl TransitionLookup {
    pub(super) fn from_transitions(transitions: &[LoadedTransition]) -> Self {
        let mut unguarded_by_key: BTreeMap<TransitionBaseKey, usize> = BTreeMap::new();
        let mut guarded_indexes = Vec::new();
        let mut state_specific_messages = BTreeSet::new();
        let mut payload_specific_transitions: BTreeSet<TransitionBaseKey> = BTreeSet::new();
        for (index, transition) in transitions.iter().enumerate() {
            let message = transition.message.as_u32();
            let current_state = transition.current_state.map(StateId::as_u32);
            if current_state.is_some() {
                state_specific_messages.insert(message);
            }
            if transition.payload_guard.is_some() {
                payload_specific_transitions.insert((message, current_state));
                guarded_indexes.push(index);
            } else {
                unguarded_by_key.insert((message, current_state), index);
            }
        }
        Self {
            unguarded_by_key,
            guarded_indexes,
            state_specific_messages,
            payload_specific_transitions,
        }
    }

    pub(super) fn for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
        transitions: &[LoadedTransition],
    ) -> Option<usize> {
        let current_state = self
            .is_state_specific_message(message)
            .then_some(current_state);
        if self
            .payload_specific_transitions
            .contains(&(message.as_u32(), current_state.map(StateId::as_u32)))
        {
            let payload = payload?;
            self.guarded_indexes.iter().copied().find(|index| {
                let Some(transition) = transitions.get(*index) else {
                    return false;
                };
                transition.message == message
                    && transition.current_state == current_state
                    && transition
                        .payload_guard
                        .as_ref()
                        .is_some_and(|guard| guard.ty == payload.ty && guard.value == payload.value)
            })
        } else {
            self.unguarded_by_key
                .get(&(message.as_u32(), current_state.map(StateId::as_u32)))
                .copied()
        }
    }

    pub(super) fn is_state_specific_message(&self, message: MessageId) -> bool {
        self.state_specific_messages.contains(&message.as_u32())
    }

    pub(super) fn is_payload_specific_base(
        &self,
        message: MessageId,
        current_state: Option<StateId>,
    ) -> bool {
        self.payload_specific_transitions
            .contains(&(message.as_u32(), current_state.map(StateId::as_u32)))
    }
}

pub(super) fn load_transitions(process: &ArtifactProcess) -> Result<Vec<LoadedTransition>> {
    process
        .transitions
        .iter()
        .map(|transition| {
            if transition.message.index() >= process.message_variants.len() {
                return Err(Error::new(format!(
                    "process {} transition message id {} is not loaded",
                    process.debug_name,
                    transition.message.as_u32()
                )));
            }
            LoadedTransition::from_artifact(transition)
        })
        .collect()
}

pub(super) fn validate_loaded_transition_coverage(process: &LoadedProcess) -> Result<()> {
    for (index, transition) in process.transitions.iter().enumerate() {
        if process.transitions[..index].iter().any(|previous| {
            previous.message == transition.message
                && previous.current_state == transition.current_state
                && previous.payload_guard == transition.payload_guard
        }) {
            let payload_guard = transition
                .payload_guard
                .as_ref()
                .map(RuntimePayload::label)
                .unwrap_or_else(|| "<none>".to_string());
            return Err(Error::new(format!(
                "process {} declares duplicate transition for message id {} current_state {:?} payload_guard {}",
                process.debug_name,
                transition.message.as_u32(),
                transition.current_state.map(StateId::as_u32),
                payload_guard
            )));
        }
    }

    let transition_keys = process
        .transitions
        .iter()
        .map(|transition| TransitionCoverageKey {
            message: transition.message.as_u32(),
            current_state: transition.current_state.map(StateId::as_u32),
            payload_guarded: transition.payload_guard.is_some(),
        })
        .collect::<BTreeSet<_>>();

    for key in &transition_keys {
        let has_unguarded_payload = transition_keys.contains(&TransitionCoverageKey {
            message: key.message,
            current_state: key.current_state,
            payload_guarded: false,
        });
        let has_guarded_payload = transition_keys.contains(&TransitionCoverageKey {
            message: key.message,
            current_state: key.current_state,
            payload_guarded: true,
        });
        if has_unguarded_payload && has_guarded_payload {
            return Err(Error::new(format!(
                "process {} mixes payload-guarded and unguarded transitions for message id {} current_state {:?}",
                process.debug_name, key.message, key.current_state
            )));
        }
    }

    for message_index in 0..process.message_variants.len() {
        let message = message_index as u32;
        let has_unguarded = transition_keys
            .iter()
            .any(|key| key.message == message && key.current_state.is_none());
        let has_guarded = (0..process.state_values.len()).any(|state_index| {
            transition_keys
                .iter()
                .any(|key| key.message == message && key.current_state == Some(state_index as u32))
        });
        if has_unguarded {
            if has_guarded {
                return Err(Error::new(format!(
                    "process {} mixes unguarded and state-specific transitions for message id {}",
                    process.debug_name, message
                )));
            }
            continue;
        }
        if !has_guarded {
            return Err(Error::new(format!(
                "process {} has no transition for message id {}",
                process.debug_name, message
            )));
        }
        for state_index in 0..process.state_values.len() {
            if !transition_keys
                .iter()
                .any(|key| key.message == message && key.current_state == Some(state_index as u32))
            {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {} current_state id {}",
                    process.debug_name, message, state_index
                )));
            }
        }
    }
    Ok(())
}
