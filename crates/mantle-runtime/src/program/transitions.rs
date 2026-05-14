use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactProcess, ArtifactValue, Error, MessageId, Result, StateId, TypeId};

use super::{LoadedProcess, LoadedTransition, RuntimePayload};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PayloadDispatchKey {
    ty: TypeId,
    value: ArtifactValue,
}

type TransitionDispatchKey = (u32, Option<u32>, Option<PayloadDispatchKey>);
type TransitionBaseKey = (u32, Option<u32>);

impl PayloadDispatchKey {
    fn from_payload(payload: &RuntimePayload) -> Self {
        Self {
            ty: payload.ty,
            value: payload.value.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TransitionLookup {
    by_key: BTreeMap<TransitionDispatchKey, usize>,
    state_specific_messages: BTreeSet<u32>,
    payload_specific_transitions: BTreeSet<TransitionBaseKey>,
}

impl TransitionLookup {
    pub(super) fn from_transitions(transitions: &[LoadedTransition]) -> Self {
        let mut by_key: BTreeMap<TransitionDispatchKey, usize> = BTreeMap::new();
        let mut state_specific_messages = BTreeSet::new();
        let mut payload_specific_transitions: BTreeSet<TransitionBaseKey> = BTreeSet::new();
        for (index, transition) in transitions.iter().enumerate() {
            let message = transition.message.as_u32();
            let current_state = transition.current_state.map(StateId::as_u32);
            if current_state.is_some() {
                state_specific_messages.insert(message);
            }
            let payload_guard = transition
                .payload_guard
                .as_ref()
                .map(PayloadDispatchKey::from_payload);
            if payload_guard.is_some() {
                payload_specific_transitions.insert((message, current_state));
            }
            by_key.insert((message, current_state, payload_guard), index);
        }
        Self {
            by_key,
            state_specific_messages,
            payload_specific_transitions,
        }
    }

    pub(super) fn for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
    ) -> Option<usize> {
        let current_state = self
            .is_state_specific_message(message)
            .then_some(current_state);
        if self
            .payload_specific_transitions
            .contains(&(message.as_u32(), current_state.map(StateId::as_u32)))
        {
            let payload = payload.map(PayloadDispatchKey::from_payload)?;
            self.exact(message, current_state, Some(payload))
        } else {
            self.exact(message, current_state, None)
        }
    }

    fn exact(
        &self,
        message: MessageId,
        current_state: Option<StateId>,
        payload: Option<PayloadDispatchKey>,
    ) -> Option<usize> {
        self.by_key
            .get(&(
                message.as_u32(),
                current_state.map(StateId::as_u32),
                payload,
            ))
            .copied()
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
    let mut transition_keys: BTreeSet<TransitionDispatchKey> = BTreeSet::new();
    for transition in &process.transitions {
        let payload_guard = transition
            .payload_guard
            .as_ref()
            .map(PayloadDispatchKey::from_payload);
        if !transition_keys.insert((
            transition.message.as_u32(),
            transition.current_state.map(StateId::as_u32),
            payload_guard,
        )) {
            return Err(Error::new(format!(
                "process {} declares duplicate transition for message id {} current_state {:?} payload_guard {}",
                process.debug_name,
                transition.message.as_u32(),
                transition.current_state.map(StateId::as_u32),
                transition
                    .payload_guard
                    .as_ref()
                    .map(RuntimePayload::label)
                    .unwrap_or("<none>")
            )));
        }
    }

    for (message, current_state, _) in &transition_keys {
        let has_unguarded_payload = transition_keys.contains(&(*message, *current_state, None));
        let has_guarded_payload =
            transition_keys
                .iter()
                .any(|(transition_message, transition_state, payload_guard)| {
                    transition_message == message
                        && transition_state == current_state
                        && payload_guard.is_some()
                });
        if has_unguarded_payload && has_guarded_payload {
            return Err(Error::new(format!(
                "process {} mixes payload-guarded and unguarded transitions for message id {} current_state {:?}",
                process.debug_name, message, current_state
            )));
        }
    }

    for message_index in 0..process.message_variants.len() {
        let message = message_index as u32;
        let has_unguarded = transition_keys
            .iter()
            .any(|(transition_message, current_state, _)| {
                *transition_message == message && current_state.is_none()
            });
        let has_guarded = (0..process.state_values.len()).any(|state_index| {
            transition_keys
                .iter()
                .any(|(transition_message, current_state, _)| {
                    *transition_message == message && *current_state == Some(state_index as u32)
                })
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
                .any(|(transition_message, current_state, _)| {
                    *transition_message == message && *current_state == Some(state_index as u32)
                })
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
