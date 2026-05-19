use super::value_template::ValueTemplatePayloadValidation;
use super::*;

mod actions;
mod templates;

use templates::{
    transition_payload_guard_key, transition_payload_guard_label, validate_bool_condition_template,
    validate_template_loop_elements,
};

type TransitionPayloadGuardKey = Option<(u32, ArtifactValue)>;
type TransitionCoverageKey = (u32, Option<u32>, TransitionPayloadGuardKey);

#[derive(Clone, Copy)]
struct TransitionValueTypes<'a> {
    received_payload: Option<TypeId>,
    current_state_payload: Option<&'a ArtifactPayload>,
}

impl TransitionValueTypes<'_> {
    fn current_state_payload_type(self) -> Option<TypeId> {
        self.current_state_payload.map(|payload| payload.ty)
    }
}

#[derive(Clone, Copy)]
struct ActiveArtifactLoopElement {
    id: LoopElementId,
    ty: TypeId,
}

#[derive(Clone, Copy)]
pub(in crate::artifact) struct ActionReferenceScope<'a> {
    active_loop_elements: &'a [ActiveArtifactLoopElement],
    inside_loop: bool,
    inside_runtime_if_branch: bool,
}

impl<'a> ActionReferenceScope<'a> {
    const fn root() -> Self {
        Self {
            active_loop_elements: &[],
            inside_loop: false,
            inside_runtime_if_branch: false,
        }
    }

    const fn loop_body(active_loop_elements: &'a [ActiveArtifactLoopElement]) -> Self {
        Self {
            active_loop_elements,
            inside_loop: true,
            inside_runtime_if_branch: false,
        }
    }

    const fn if_branch(self) -> Self {
        Self {
            active_loop_elements: self.active_loop_elements,
            inside_loop: self.inside_loop,
            inside_runtime_if_branch: true,
        }
    }
}

impl ArtifactProcess {
    pub(super) fn validate_identity(&self, artifact: &MantleArtifact) -> Result<()> {
        validate_ident_field("process debug_name", &self.debug_name)?;
        artifact.validate_value_type("state_type", self.state_type)?;
        artifact.validate_value_type("message_type", self.message_type)?;
        validate_count("mailbox_bound", self.mailbox_bound, 1, MAX_MAILBOX_BOUND)?;
        validate_count(
            "state_value_count",
            self.state_values.len(),
            1,
            MAX_STATE_VALUES_PER_PROCESS,
        )?;
        validate_count(
            "message_count",
            self.message_variants.len(),
            1,
            MAX_MESSAGE_VARIANTS_PER_PROCESS,
        )?;
        validate_count(
            "process_ref_count",
            self.process_refs.len(),
            0,
            MAX_PROCESS_REFS_PER_PROCESS,
        )?;
        validate_count(
            "transition_count",
            self.transitions.len(),
            1,
            MAX_TRANSITIONS_PER_PROCESS,
        )?;
        validate_unique_state_value_list(&self.state_values)?;
        for state_value in &self.state_values {
            artifact.validate_value_type("state value type", state_value.ty)?;
            if state_value.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} state value {} (label {}) has type id {}, expected {}",
                    self.debug_name,
                    state_value.value.label(),
                    state_value.label,
                    state_value.ty.as_u32(),
                    self.state_type.as_u32()
                )));
            }
            artifact.validate_value_matches_type(
                "state value",
                state_value.ty,
                &state_value.value,
            )?;
            if let Some(payload) = &state_value.payload {
                if payload.process_ref.is_some() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state_value.label
                    )));
                }
                artifact.validate_value_matches_type(
                    "state value payload",
                    payload.ty,
                    &payload.value,
                )?;
            }
        }
        validate_unique_message_variant_list(&self.message_variants)?;
        for message in &self.message_variants {
            if let Some(payload_type) = message.payload_type {
                artifact.type_entry(payload_type).map_err(|err| {
                    Error::new(format!(
                        "process {} message {} payload_type_id {} is invalid: {err}",
                        self.debug_name,
                        message.label,
                        payload_type.as_u32()
                    ))
                })?;
            }
        }
        validate_unique_process_ref_list(&self.process_refs)?;
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a valid state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        let mut transition_keys: BTreeSet<TransitionCoverageKey> = BTreeSet::new();
        let mut action_count = 0usize;
        for transition in &self.transitions {
            self.validate_transition_payload_guard(artifact, transition)?;
            let transition_key = (
                transition.message.as_u32(),
                transition.current_state.map(StateId::as_u32),
                transition_payload_guard_key(&transition.payload_guard),
            );
            if !transition_keys.insert(transition_key) {
                return Err(Error::new(format!(
                    "process {} declares duplicate transition for message id {} current_state {:?} payload_guard {}",
                    self.debug_name,
                    transition.message.as_u32(),
                    transition.current_state.map(StateId::as_u32),
                    transition_payload_guard_label(&transition.payload_guard)
                )));
            }
            if transition.message.index() >= self.message_variants.len() {
                return Err(Error::new(format!(
                    "process {} transition message id {} is not accepted",
                    self.debug_name,
                    transition.message.as_u32()
                )));
            }
            let current_state_payload = self.transition_current_state_payload(transition)?;
            let transition_context = transition.transition_context();
            let value_types = TransitionValueTypes {
                received_payload: self
                    .message_variants
                    .get(transition.message.index())
                    .and_then(|message| message.payload_type),
                current_state_payload,
            };
            self.validate_next_state(
                artifact,
                transition,
                &transition_context,
                &transition.next_state,
                value_types,
                0,
            )?;
            action_count = action_count
                .checked_add(super::action_count(&transition.actions)?)
                .ok_or_else(|| Error::new("process action_count overflowed"))?;
        }
        validate_count("action_count", action_count, 0, MAX_ACTIONS_PER_PROCESS)?;
        self.validate_transition_coverage(&transition_keys)?;
        Ok(())
    }

    fn validate_transition_coverage(
        &self,
        transition_keys: &BTreeSet<TransitionCoverageKey>,
    ) -> Result<()> {
        for (message, current_state, _) in transition_keys {
            let has_unguarded_payload = transition_keys.contains(&(*message, *current_state, None));
            let has_guarded_payload = transition_keys.iter().any(
                |(transition_message, transition_state, payload_guard)| {
                    transition_message == message
                        && transition_state == current_state
                        && payload_guard.is_some()
                },
            );
            if has_unguarded_payload && has_guarded_payload {
                return Err(Error::new(format!(
                    "process {} mixes payload-guarded and unguarded transitions for message id {} current_state {:?}",
                    self.debug_name, message, current_state
                )));
            }
        }

        for message_index in 0..self.message_variants.len() {
            let message = message_index as u32;
            let has_unguarded =
                transition_keys
                    .iter()
                    .any(|(transition_message, current_state, _)| {
                        *transition_message == message && current_state.is_none()
                    });
            let has_guarded = (0..self.state_values.len()).any(|state_index| {
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
                        self.debug_name, message
                    )));
                }
                continue;
            }
            if !has_guarded {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {}",
                    self.debug_name, message
                )));
            }
            for state_index in 0..self.state_values.len() {
                if !transition_keys
                    .iter()
                    .any(|(transition_message, current_state, _)| {
                        *transition_message == message && *current_state == Some(state_index as u32)
                    })
                {
                    return Err(Error::new(format!(
                        "process {} has no transition for message id {} current_state id {}",
                        self.debug_name, message, state_index
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_transition_payload_guard(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
    ) -> Result<()> {
        let Some(payload_guard) = &transition.payload_guard else {
            return Ok(());
        };
        if payload_guard.process_ref.is_some() {
            return Err(Error::new(format!(
                "process {} transition message id {} payload guard cannot be a process reference payload",
                self.debug_name,
                transition.message.as_u32()
            )));
        }
        let message = self
            .message_variants
            .get(transition.message.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} transition message id {} is not accepted",
                    self.debug_name,
                    transition.message.as_u32()
                ))
            })?;
        let Some(expected_type) = message.payload_type else {
            return Err(Error::new(format!(
                "process {} transition message id {} has a payload guard but the message does not accept a payload",
                self.debug_name,
                transition.message.as_u32()
            )));
        };
        if payload_guard.ty != expected_type {
            return Err(Error::new(format!(
                "process {} transition message id {} payload guard has type id {}, expected {}",
                self.debug_name,
                transition.message.as_u32(),
                payload_guard.ty.as_u32(),
                expected_type.as_u32()
            )));
        }
        artifact.validate_value_matches_type(
            "transition payload guard",
            payload_guard.ty,
            &payload_guard.value,
        )
    }

    fn validate_static_next_state_template_value(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        template: &ArtifactValueTemplate,
    ) -> Result<()> {
        if template.depends_on_received_payload() {
            return Ok(());
        }
        let current_state_payload = transition
            .current_state
            .and_then(|state| self.state_values.get(state.index()))
            .and_then(|state| state.payload.as_ref());
        let value = artifact.evaluate_state_value_with_current_state(
            template,
            None,
            current_state_payload,
        )?;
        if self
            .state_values
            .iter()
            .any(|state_value| state_value.has_same_identity(&value))
        {
            return Ok(());
        }
        Err(Error::new(format!(
            "process {} {} next_state_template produced value {} not admitted by state table",
            self.debug_name,
            transition.transition_context(),
            value.label
        )))
    }

    fn validate_next_state(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        transition_context: &str,
        next_state: &NextState,
        value_types: TransitionValueTypes,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "process {} {} next_state exceeds maximum control-flow depth of {MAX_VALUE_TEMPLATE_DEPTH}",
                self.debug_name, transition_context
            )));
        }
        match next_state {
            NextState::Current => Ok(()),
            NextState::Value(state) => {
                if state.index() >= self.state_values.len() {
                    return Err(Error::new(format!(
                        "process {} {} next_state id {} is not a valid state value",
                        self.debug_name,
                        transition_context,
                        state.as_u32()
                    )));
                }
                Ok(())
            }
            NextState::Template(template) => {
                validate_template_loop_elements(
                    artifact,
                    template,
                    &[],
                    &format!(
                        "process {} {} next_state_template",
                        self.debug_name, transition_context
                    ),
                )?;
                template.validate_for_received_payload(
                    artifact,
                    &format!(
                        "process {} {} next_state_template",
                        self.debug_name, transition_context
                    ),
                    ValueTemplatePayloadValidation::new(
                        Some(self.state_type),
                        value_types.received_payload,
                        value_types.current_state_payload_type(),
                        false,
                    ),
                    0,
                )?;
                self.validate_static_next_state_template_value(artifact, transition, template)
            }
            NextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                validate_bool_condition_template(
                    artifact,
                    &format!(
                        "process {} {} next_state_condition",
                        self.debug_name, transition_context
                    ),
                    condition,
                    value_types.received_payload,
                    value_types.current_state_payload,
                )?;
                validate_template_loop_elements(
                    artifact,
                    condition,
                    &[],
                    &format!(
                        "process {} {} next_state_condition",
                        self.debug_name, transition_context
                    ),
                )?;
                self.validate_next_state(
                    artifact,
                    transition,
                    &format!("{transition_context} then"),
                    then_state,
                    value_types,
                    depth + 1,
                )?;
                self.validate_next_state(
                    artifact,
                    transition,
                    &format!("{transition_context} else"),
                    else_state,
                    value_types,
                    depth + 1,
                )
            }
        }
    }

    fn transition_current_state_payload(
        &self,
        transition: &ArtifactTransition,
    ) -> Result<Option<&ArtifactPayload>> {
        let Some(current_state) = transition.current_state else {
            return Ok(None);
        };
        let state_value = self
            .state_values
            .get(current_state.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} current_state id {} is not a valid state value",
                    self.debug_name,
                    transition.message.as_u32(),
                    current_state.as_u32()
                ))
            })?;
        Ok(state_value.payload.as_ref())
    }

    pub(super) fn validate_references(
        &self,
        artifact: &MantleArtifact,
        process_id: ProcessId,
    ) -> Result<()> {
        for process_ref in &self.process_refs {
            if process_ref.target.index() >= artifact.processes.len() {
                return Err(Error::new(format!(
                    "process {} process reference {} targets undefined process id {}",
                    self.debug_name,
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == artifact.entry_process {
                return Err(Error::new(format!(
                    "process {} process reference {} targets entry process id {}",
                    self.debug_name,
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == process_id {
                return Err(Error::new(format!(
                    "process {} process reference {} targets itself, which is not supported",
                    self.debug_name, process_ref.debug_name
                )));
            }
        }
        for transition in &self.transitions {
            let declared_effects = transition.validate_effects(&self.debug_name)?;
            let mut spawned_refs = BTreeSet::new();
            let mut used_effects = BTreeSet::new();
            for action in &transition.actions {
                action.collect_effects(&mut used_effects);
            }
            for action_effect in &used_effects {
                if !declared_effects.contains(action_effect) {
                    return Err(Error::new(format!(
                        "process {} transition {} uses effect {action_effect} but does not declare it",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
            }
            for action in &transition.actions {
                self.validate_action_reference(
                    artifact,
                    transition,
                    &mut spawned_refs,
                    action,
                    ActionReferenceScope::root(),
                )?;
            }
            for declared_effect in &declared_effects {
                if !used_effects.contains(declared_effect) {
                    return Err(Error::new(format!(
                        "process {} transition {} declares effect {declared_effect} but no action uses it",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
            }
        }
        Ok(())
    }
}
