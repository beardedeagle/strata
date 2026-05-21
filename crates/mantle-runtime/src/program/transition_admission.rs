use super::templates::{
    LoadedTemplateAdmission, evaluate_loaded_state_value,
    loaded_template_depends_on_received_payload, validate_loaded_bool_condition,
};
use super::*;

#[derive(Clone, Copy)]
struct LoadedTransitionValueTypes<'a> {
    received_payload: Option<TypeId>,
    current_state_payload: Option<&'a RuntimePayload>,
}

impl LoadedTransitionValueTypes<'_> {
    fn current_state_payload_type(self) -> Option<TypeId> {
        self.current_state_payload.map(|payload| payload.ty)
    }
}

impl LoadedTransition {
    pub(in crate::program) fn from_artifact(transition: &ArtifactTransition) -> Result<Self> {
        Ok(Self {
            current_state: transition.current_state,
            message: transition.message,
            payload_guard: transition
                .payload_guard
                .as_ref()
                .map(RuntimePayload::from_artifact)
                .transpose()?,
            step_result: transition.step_result,
            next_state: LoadedNextState::from_artifact(&transition.next_state)?,
            effect_authority: LoadedEffectAuthority::from_artifact(&transition.effects),
            actions: transition
                .actions
                .iter()
                .map(LoadedAction::from_artifact)
                .collect::<Result<Vec<_>>>()?,
        })
    }

    pub(in crate::program) fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        self.validate_next_state(program, process, message)?;
        self.validate_payload_guard(program, process, message)?;

        let current_state_payload = transition_current_state_payload(process, self)?;
        let mut spawned_refs = vec![false; process.process_refs.len()];
        for action in &self.actions {
            action.validate_admission(
                program,
                process,
                message,
                current_state_payload,
                &mut spawned_refs,
            )?;
        }
        Ok(())
    }

    fn validate_payload_guard(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        let Some(payload_guard) = &self.payload_guard else {
            return Ok(());
        };
        if payload_guard.process_ref.is_some() || payload_guard.value.contains_process_ref() {
            return Err(Error::new(format!(
                "process {} message id {} payload guard cannot be a process reference payload",
                process.debug_name,
                message.as_u32()
            )));
        }
        let message_variant = process
            .message_variants
            .get(message.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} is not loaded",
                    process.debug_name,
                    message.as_u32()
                ))
            })?;
        let payload_type = message_variant
            .payload_type
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} has a payload guard but the message does not accept a payload",
                    process.debug_name,
                    message.as_u32()
                ))
            })?;
        if payload_guard.ty != payload_type {
            return Err(Error::new(format!(
                "process {} message id {} payload guard has type id {}, expected {}",
                process.debug_name,
                message.as_u32(),
                payload_guard.ty.as_u32(),
                payload_type.as_u32()
            )));
        }
        program.validate_value_matches_type(
            &format!(
                "process {} message id {} payload guard",
                process.debug_name,
                message.as_u32()
            ),
            payload_guard.ty,
            &payload_guard.value,
        )?;
        Ok(())
    }

    fn validate_next_state(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        let context = self.transition_context(message);
        let value_types = LoadedTransitionValueTypes {
            received_payload: process
                .message_variants
                .get(message.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} message id {} is not loaded",
                        process.debug_name,
                        message.as_u32()
                    ))
                })?
                .payload_type,
            current_state_payload: transition_current_state_payload(process, self)?,
        };
        self.validate_next_state_node(program, process, &context, &self.next_state, value_types, 0)
    }

    fn validate_next_state_node(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        context: &str,
        next_state: &LoadedNextState,
        value_types: LoadedTransitionValueTypes,
        depth: usize,
    ) -> Result<()> {
        match next_state {
            LoadedNextState::Current => Ok(()),
            LoadedNextState::Value(state) => {
                if state.index() >= process.state_values.len() {
                    return Err(Error::new(format!(
                        "process {} {} next_state id {} is not a loaded state value",
                        process.debug_name,
                        context,
                        state.as_u32()
                    )));
                }
                Ok(())
            }
            LoadedNextState::Template(template) => {
                LoadedTemplateAdmission {
                    expected_type: Some(process.state_type),
                    received_payload_type: value_types.received_payload,
                    current_state_payload_type: value_types.current_state_payload_type(),
                    allow_direct_process_ref: false,
                    loop_elements: &[],
                    program,
                    process,
                    spawned_refs: &[],
                }
                .validate(
                    &format!(
                        "process {} {} next_state_template",
                        process.debug_name, context
                    ),
                    template,
                )?;
                if loaded_template_depends_on_received_payload(template) {
                    return Ok(());
                }
                let value = evaluate_loaded_state_value(
                    program,
                    template,
                    None,
                    value_types.current_state_payload,
                )?;
                if process.state_values.iter().any(|state_value| {
                    state_value.ty == value.ty && state_value.value == value.value
                }) {
                    return Ok(());
                }
                Err(Error::new(format!(
                    "process {} {} next_state_template produced value {} not admitted by loaded state table",
                    process.debug_name, context, value.label
                )))
            }
            LoadedNextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                if depth >= MAX_NEXT_STATE_IF_ELSE_DEPTH {
                    return Err(Error::new(format!(
                        "process {} {} next_state runtime if nesting exceeds maximum depth of {MAX_NEXT_STATE_IF_ELSE_DEPTH} in this loaded runtime slice",
                        process.debug_name, context
                    )));
                }
                let branch_depth = depth + 1;
                validate_loaded_bool_condition(
                    program,
                    process,
                    &format!(
                        "process {} {} next_state_condition",
                        process.debug_name, context
                    ),
                    condition,
                    value_types.received_payload,
                    value_types.current_state_payload,
                )?;
                self.validate_next_state_node(
                    program,
                    process,
                    &format!("{context} then"),
                    then_state,
                    value_types,
                    branch_depth,
                )?;
                self.validate_next_state_node(
                    program,
                    process,
                    &format!("{context} else"),
                    else_state,
                    value_types,
                    branch_depth,
                )
            }
        }
    }

    fn transition_context(&self, message: MessageId) -> String {
        match self.current_state {
            Some(current_state) => format!(
                "message id {} current_state id {}",
                message.as_u32(),
                current_state.as_u32()
            ),
            None => format!("message id {}", message.as_u32()),
        }
    }
}

impl LoadedNextState {
    pub(crate) fn from_artifact(next_state: &NextState) -> Result<Self> {
        Self::from_artifact_at_depth(next_state, 0)
    }

    fn from_artifact_at_depth(next_state: &NextState, depth: usize) -> Result<Self> {
        match next_state {
            NextState::Current => Ok(Self::Current),
            NextState::Value(state) => Ok(Self::Value(*state)),
            NextState::Template(template) => Ok(Self::Template(
                LoadedValueTemplate::from_artifact(template)?,
            )),
            NextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                if depth >= MAX_NEXT_STATE_IF_ELSE_DEPTH {
                    return Err(Error::new(format!(
                        "loaded next_state runtime if nesting exceeds maximum depth of {MAX_NEXT_STATE_IF_ELSE_DEPTH}"
                    )));
                }
                let branch_depth = depth + 1;
                Ok(Self::IfElse {
                    condition: LoadedValueTemplate::from_artifact(condition)?,
                    then_state: Box::new(Self::from_artifact_at_depth(then_state, branch_depth)?),
                    else_state: Box::new(Self::from_artifact_at_depth(else_state, branch_depth)?),
                })
            }
        }
    }
}

fn transition_current_state_payload<'a>(
    process: &'a LoadedProcess,
    transition: &LoadedTransition,
) -> Result<Option<&'a RuntimePayload>> {
    let Some(current_state) = transition.current_state else {
        return Ok(None);
    };
    let state_value = process
        .state_values
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} message id {} current_state id {} is not a loaded state value",
                process.debug_name,
                transition.message.as_u32(),
                current_state.as_u32()
            ))
        })?;
    Ok(state_value.payload.as_ref())
}
