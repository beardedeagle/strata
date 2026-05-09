use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactProcess, Error, MessageId, Result, StateId};

use super::{LoadedProcess, LoadedTransition};

#[derive(Debug, Clone)]
pub(super) struct TransitionLookup {
    by_key: BTreeMap<(u32, Option<u32>), usize>,
    state_specific_messages: BTreeSet<u32>,
}

impl TransitionLookup {
    pub(super) fn from_transitions(transitions: &[LoadedTransition]) -> Self {
        let mut by_key = BTreeMap::new();
        let mut state_specific_messages = BTreeSet::new();
        for (index, transition) in transitions.iter().enumerate() {
            let message = transition.message.as_u32();
            let current_state = transition.current_state.map(StateId::as_u32);
            if current_state.is_some() {
                state_specific_messages.insert(message);
            }
            by_key.insert((message, current_state), index);
        }
        Self {
            by_key,
            state_specific_messages,
        }
    }

    pub(super) fn for_dispatch(&self, message: MessageId, current_state: StateId) -> Option<usize> {
        if self.is_state_specific_message(message) {
            self.exact(message, Some(current_state))
        } else {
            self.exact(message, None)
        }
    }

    fn exact(&self, message: MessageId, current_state: Option<StateId>) -> Option<usize> {
        self.by_key
            .get(&(message.as_u32(), current_state.map(StateId::as_u32)))
            .copied()
    }

    pub(super) fn is_state_specific_message(&self, message: MessageId) -> bool {
        self.state_specific_messages.contains(&message.as_u32())
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
            Ok(LoadedTransition::from_artifact(transition))
        })
        .collect()
}

pub(super) fn validate_loaded_transition_coverage(process: &LoadedProcess) -> Result<()> {
    let mut transition_keys = BTreeSet::new();
    for transition in &process.transitions {
        if !transition_keys.insert((
            transition.message.as_u32(),
            transition.current_state.map(StateId::as_u32),
        )) {
            return Err(Error::new(format!(
                "process {} declares duplicate transition for message id {} current_state {:?}",
                process.debug_name,
                transition.message.as_u32(),
                transition.current_state.map(StateId::as_u32)
            )));
        }
    }

    for message_index in 0..process.message_variants.len() {
        let message = message_index as u32;
        let has_unguarded = transition_keys.contains(&(message, None));
        let has_guarded = (0..process.state_values.len())
            .any(|state_index| transition_keys.contains(&(message, Some(state_index as u32))));
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
            if !transition_keys.contains(&(message, Some(state_index as u32))) {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {} current_state id {}",
                    process.debug_name, message, state_index
                )));
            }
        }
    }
    Ok(())
}
