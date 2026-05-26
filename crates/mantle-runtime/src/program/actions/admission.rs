use super::{LoadedAction, LoadedLoopElement, LoadedSendTarget};
use crate::program::templates::{
    LoadedBoolConditionAdmission, LoadedTemplateAdmission, validate_loaded_bool_condition,
    validate_loaded_bool_condition_with_loop_elements,
};
use crate::program::{LoadedProcess, LoadedProgram, LoadedValueTemplate, RuntimePayload};
use mantle_artifact::{
    ArtifactValueShape, EffectOutcomeId, Error, MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH,
    MAX_VALUE_TEMPLATE_FIELDS, MessageId, ProcessId, Result, TypeId,
};

mod effect_outcomes;
use effect_outcomes::{validate_loaded_send_outcome_type, validate_loaded_spawn_outcome_type};

#[derive(Clone, Copy)]
struct LoopBodyAdmissionScope<'a> {
    current_state_payload: Option<&'a RuntimePayload>,
    loop_elements: &'a [LoadedLoopElement],
    effect_outcomes: &'a [(EffectOutcomeId, TypeId)],
    runtime_if_depth: usize,
}

impl<'a> LoopBodyAdmissionScope<'a> {
    const fn new(
        current_state_payload: Option<&'a RuntimePayload>,
        loop_elements: &'a [LoadedLoopElement],
        effect_outcomes: &'a [(EffectOutcomeId, TypeId)],
    ) -> Self {
        Self {
            current_state_payload,
            loop_elements,
            effect_outcomes,
            runtime_if_depth: 0,
        }
    }

    const fn if_branch(self) -> Self {
        Self {
            current_state_payload: self.current_state_payload,
            loop_elements: self.loop_elements,
            effect_outcomes: self.effect_outcomes,
            runtime_if_depth: self.runtime_if_depth.saturating_add(1),
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::program) struct ActionAdmissionContext<'a> {
    pub(in crate::program) program: &'a LoadedProgram,
    pub(in crate::program) process: &'a LoadedProcess,
    pub(in crate::program) process_id: ProcessId,
    pub(in crate::program) message: MessageId,
    pub(in crate::program) current_state_payload: Option<&'a RuntimePayload>,
    pub(in crate::program) effect_outcomes: &'a [(EffectOutcomeId, TypeId)],
}

#[derive(Clone, Copy)]
struct RuntimeIfAdmission<'a> {
    condition: &'a LoadedValueTemplate,
    then_actions: &'a [LoadedAction],
    else_actions: &'a [LoadedAction],
    depth: usize,
}

impl LoadedAction {
    pub(in crate::program) fn validate_admission(
        &self,
        context: ActionAdmissionContext<'_>,
        spawned_refs: &mut [bool],
    ) -> Result<()> {
        let program = context.program;
        let process = context.process;
        let process_id = context.process_id;
        let message = context.message;
        let current_state_payload = context.current_state_payload;
        let effect_outcomes = context.effect_outcomes;
        let current_state_payload_type = current_state_payload.map(|payload| payload.ty);
        match self {
            Self::Emit { output } => {
                program.output(*output)?;
                Ok(())
            }
            Self::Spawn {
                target,
                process_ref,
            } => {
                program.process(*target)?;
                let declared_target = process.process_ref_target(*process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} transition {} spawn process reference id {} targets process id {}, expected {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                let Some(is_spawned) = spawned_refs.get_mut(process_ref.index()) else {
                    return Err(Error::new(format!(
                        "process {} transition {} spawn references unloaded process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    )));
                };
                if *is_spawned {
                    return Err(Error::new(format!(
                        "process {} transition {} duplicates process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    )));
                }
                *is_spawned = true;
                Ok(())
            }
            Self::SpawnOutcome {
                outcome_ty, target, ..
            } => {
                program.process(*target)?;
                if *target == program.entry_process {
                    return Err(Error::new(format!(
                        "process {} transition {} spawn outcome targets entry process id {}",
                        process.debug_name,
                        message.as_u32(),
                        target.as_u32()
                    )));
                }
                if *target == process_id {
                    return Err(Error::new(format!(
                        "process {} transition {} spawn outcome targets itself, which is not supported",
                        process.debug_name,
                        message.as_u32()
                    )));
                }
                validate_loaded_spawn_outcome_type(program, *outcome_ty, *target)
            }
            Self::Send {
                target,
                message: sent_message,
                payload,
            }
            | Self::SendOutcome {
                outcome_ty: _,
                target,
                message: sent_message,
                payload,
                ..
            } => {
                let target_process_id =
                    target.validate_admission(program, process, message, spawned_refs)?;
                let target_process = program.process(target_process_id)?;
                let target_message =
                    target_process
                        .message_variants
                        .get(sent_message.index())
                        .ok_or_else(|| {
                            Error::new(format!(
                                "process {} transition {} sends message id {} not loaded by process id {}",
                                process.debug_name,
                                message.as_u32(),
                                sent_message.as_u32(),
                                target_process_id.as_u32()
                            ))
                        })?;
                if let Self::SendOutcome { outcome_ty, .. } = self {
                    validate_loaded_send_outcome_type(
                        program,
                        *outcome_ty,
                        target_process.message_type,
                    )?;
                }
                match (target_message.payload_type, payload) {
                    (None, None) => Ok(()),
                    (None, Some(_)) => Err(Error::new(format!(
                        "process {} transition {} sends payload to process id {} message id {}, which does not accept one",
                        process.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32(),
                        sent_message.as_u32()
                    ))),
                    (Some(_), None) => Err(Error::new(format!(
                        "process {} transition {} sends process id {} message id {} without required payload",
                        process.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32(),
                        sent_message.as_u32()
                    ))),
                    (Some(payload_type), Some(payload)) => LoadedTemplateAdmission {
                        expected_type: Some(payload_type),
                        received_payload_type: process.message_variants[message.index()]
                            .payload_type,
                        current_state_payload_type,
                        allow_direct_process_ref: true,
                        allow_process_ref_effect_outcome: false,
                        loop_elements: &[],
                        effect_outcomes,
                        program,
                        process,
                        spawned_refs,
                    }
                    .validate(
                        &format!(
                            "process {} transition {} send payload",
                            process.debug_name,
                            message.as_u32()
                        ),
                        payload,
                    ),
                }
            }
            Self::IfElse {
                condition,
                then_actions,
                else_actions,
            } => Self::validate_if_else_admission(
                ActionAdmissionContext {
                    program,
                    process,
                    process_id,
                    message,
                    current_state_payload,
                    effect_outcomes,
                },
                spawned_refs,
                RuntimeIfAdmission {
                    condition,
                    then_actions,
                    else_actions,
                    depth: 0,
                },
            ),
            Self::ForEach {
                element,
                collection,
                max_items,
                body,
            } => {
                program.validate_value_type("for loop element type", element.ty)?;
                if element.id.index() >= MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "for loop element id {} must be no greater than {}",
                        element.id.as_u32(),
                        MAX_VALUE_TEMPLATE_FIELDS - 1
                    )));
                }
                if *max_items > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "for loop max_items must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                validate_for_each_collection_type(
                    program,
                    &format!(
                        "process {} transition {} for collection",
                        process.debug_name,
                        message.as_u32()
                    ),
                    collection,
                    element.ty,
                )?;
                LoadedTemplateAdmission {
                    expected_type: None,
                    received_payload_type: process.message_variants[message.index()].payload_type,
                    current_state_payload_type,
                    allow_direct_process_ref: false,
                    allow_process_ref_effect_outcome: false,
                    loop_elements: &[],
                    effect_outcomes,
                    program,
                    process,
                    spawned_refs,
                }
                .validate(
                    &format!(
                        "process {} transition {} for collection",
                        process.debug_name,
                        message.as_u32()
                    ),
                    collection,
                )?;
                let active = [element.clone()];
                for action in body {
                    action.validate_loop_body_admission(
                        program,
                        process,
                        process_id,
                        message,
                        spawned_refs,
                        LoopBodyAdmissionScope::new(
                            current_state_payload,
                            &active,
                            effect_outcomes,
                        ),
                    )?;
                }
                Ok(())
            }
        }
    }

    fn validate_runtime_if_branch_admission(
        &self,
        context: ActionAdmissionContext<'_>,
        spawned_refs: &mut [bool],
        runtime_if_depth: usize,
    ) -> Result<()> {
        match self {
            Self::Spawn { .. } | Self::SpawnOutcome { .. } => Err(Error::new(format!(
                "process {} transition {} runtime if branch cannot bind process references or spawn outcomes in this artifact slice",
                context.process.debug_name,
                context.message.as_u32()
            ))),
            Self::SendOutcome { .. } => Err(Error::new(format!(
                "process {} transition {} runtime if branch cannot bind send outcomes in this artifact slice",
                context.process.debug_name,
                context.message.as_u32()
            ))),
            Self::ForEach { .. } => self.validate_admission(context, spawned_refs),
            Self::IfElse {
                condition,
                then_actions,
                else_actions,
            } => Self::validate_if_else_admission(
                context,
                spawned_refs,
                RuntimeIfAdmission {
                    condition,
                    then_actions,
                    else_actions,
                    depth: runtime_if_depth,
                },
            ),
            Self::Emit { .. } | Self::Send { .. } => self.validate_admission(context, spawned_refs),
        }
    }

    fn validate_if_else_admission(
        context: ActionAdmissionContext<'_>,
        spawned_refs: &mut [bool],
        runtime_if: RuntimeIfAdmission<'_>,
    ) -> Result<()> {
        if runtime_if.depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
            return Err(Error::new(format!(
                "process {} transition {} runtime if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH} in this artifact slice",
                context.process.debug_name,
                context.message.as_u32()
            )));
        }
        validate_loaded_bool_condition(
            context.program,
            context.process,
            &format!(
                "process {} transition {} if condition",
                context.process.debug_name,
                context.message.as_u32()
            ),
            runtime_if.condition,
            context.process.message_variants[context.message.index()].payload_type,
            context.current_state_payload,
            context.effect_outcomes,
        )?;
        if runtime_if.then_actions.is_empty() && runtime_if.else_actions.is_empty() {
            return Err(Error::new(format!(
                "process {} transition {} runtime if action branches cannot both be empty",
                context.process.debug_name,
                context.message.as_u32()
            )));
        }
        let branch_runtime_if_depth = runtime_if
            .depth
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime if action nesting depth overflowed"))?;
        for action in runtime_if.then_actions {
            action.validate_runtime_if_branch_admission(
                context,
                spawned_refs,
                branch_runtime_if_depth,
            )?;
        }
        for action in runtime_if.else_actions {
            action.validate_runtime_if_branch_admission(
                context,
                spawned_refs,
                branch_runtime_if_depth,
            )?;
        }
        Ok(())
    }

    fn validate_loop_body_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        process_id: ProcessId,
        message: MessageId,
        spawned_refs: &mut [bool],
        scope: LoopBodyAdmissionScope<'_>,
    ) -> Result<()> {
        let current_state_payload_type = scope.current_state_payload.map(|payload| payload.ty);
        match self {
            Self::Spawn { .. } | Self::SpawnOutcome { .. } => Err(Error::new(format!(
                "process {} transition {} for loop body cannot bind process references or spawn outcomes",
                process.debug_name,
                message.as_u32()
            ))),
            Self::SendOutcome { .. } => Err(Error::new(format!(
                "process {} transition {} for loop body cannot bind send outcomes",
                process.debug_name,
                message.as_u32()
            ))),
            Self::ForEach { .. } => Err(Error::new(format!(
                "process {} transition {} nested for loops are not supported in this artifact slice",
                process.debug_name,
                message.as_u32()
            ))),
            Self::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                if scope.runtime_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
                    return Err(Error::new(format!(
                        "process {} transition {} runtime if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH} in this artifact slice",
                        process.debug_name,
                        message.as_u32()
                    )));
                }
                validate_loaded_bool_condition_with_loop_elements(
                    LoadedBoolConditionAdmission {
                        program,
                        process,
                        field: &format!(
                            "process {} transition {} if condition",
                            process.debug_name,
                            message.as_u32()
                        ),
                        received_payload_type: process.message_variants[message.index()]
                            .payload_type,
                        current_state_payload: scope.current_state_payload,
                        loop_elements: scope.loop_elements,
                        effect_outcomes: scope.effect_outcomes,
                    },
                    condition,
                )?;
                if then_actions.is_empty() && else_actions.is_empty() {
                    return Err(Error::new(format!(
                        "process {} transition {} runtime if action branches cannot both be empty",
                        process.debug_name,
                        message.as_u32()
                    )));
                }
                let branch_scope = scope.if_branch();
                for action in then_actions {
                    action.validate_loop_body_admission(
                        program,
                        process,
                        process_id,
                        message,
                        spawned_refs,
                        branch_scope,
                    )?;
                }
                for action in else_actions {
                    action.validate_loop_body_admission(
                        program,
                        process,
                        process_id,
                        message,
                        spawned_refs,
                        branch_scope,
                    )?;
                }
                Ok(())
            }
            Self::Emit { .. } => self.validate_admission(
                ActionAdmissionContext {
                    program,
                    process,
                    process_id,
                    message,
                    current_state_payload: scope.current_state_payload,
                    effect_outcomes: scope.effect_outcomes,
                },
                spawned_refs,
            ),
            Self::Send {
                target,
                message: sent_message,
                payload,
            } => {
                let target_process_id =
                    target.validate_admission(program, process, message, spawned_refs)?;
                let target_process = program.process(target_process_id)?;
                let target_message =
                    target_process
                        .message_variants
                        .get(sent_message.index())
                        .ok_or_else(|| {
                            Error::new(format!(
                                "process {} transition {} sends message id {} not loaded by process id {}",
                                process.debug_name,
                                message.as_u32(),
                                sent_message.as_u32(),
                                target_process_id.as_u32()
                            ))
                        })?;
                match (target_message.payload_type, payload) {
                    (None, None) => Ok(()),
                    (Some(payload_type), Some(payload)) => LoadedTemplateAdmission {
                        expected_type: Some(payload_type),
                        received_payload_type: process.message_variants[message.index()]
                            .payload_type,
                        current_state_payload_type,
                        allow_direct_process_ref: false,
                        allow_process_ref_effect_outcome: false,
                        loop_elements: scope.loop_elements,
                        effect_outcomes: scope.effect_outcomes,
                        program,
                        process,
                        spawned_refs,
                    }
                    .validate(
                        &format!(
                            "process {} transition {} send payload",
                            process.debug_name,
                            message.as_u32()
                        ),
                        payload,
                    ),
                    (None, Some(_)) => Err(Error::new(format!(
                        "process {} transition {} sends payload to process id {} message id {}, which does not accept one",
                        process.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32(),
                        sent_message.as_u32()
                    ))),
                    (Some(_), None) => Err(Error::new(format!(
                        "process {} transition {} sends process id {} message id {} without required payload",
                        process.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32(),
                        sent_message.as_u32()
                    ))),
                }
            }
        }
    }
}

fn validate_for_each_collection_type(
    program: &LoadedProgram,
    field: &str,
    collection: &LoadedValueTemplate,
    element_type: TypeId,
) -> Result<()> {
    let collection_type = collection.result_type();
    let type_entry = program.type_entry(collection_type)?;
    let ArtifactValueShape::List { element, .. } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field} type id {} must be a list type",
            collection_type.as_u32()
        )));
    };
    if *element != element_type {
        return Err(Error::new(format!(
            "{field} element type id {}, expected {}",
            element.as_u32(),
            element_type.as_u32()
        )));
    }
    Ok(())
}

impl LoadedSendTarget {
    fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
        spawned_refs: &[bool],
    ) -> Result<ProcessId> {
        match self {
            Self::ProcessRef(process_ref) => {
                let target_process = process.process_ref_target(*process_ref)?;
                let is_spawned = spawned_refs.get(process_ref.index()).copied().ok_or_else(|| {
                    Error::new(format!(
                        "process {} transition {} sends through unloaded process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    ))
                })?;
                if !is_spawned {
                    return Err(Error::new(format!(
                        "process {} transition {} sends through unbound process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    )));
                }
                Ok(target_process)
            }
            Self::ReceivedPayload { ty, target_process } => {
                program.validate_process_ref_type_id_target(
                    "send target payload type",
                    *ty,
                    *target_process,
                )?;
                let received_payload_type = process.message_variants[message.index()]
                    .payload_type
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} transition {} send target requires a payload-bearing message",
                            process.debug_name,
                            message.as_u32()
                        ))
                    })?;
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "process {} transition {} send target has received payload type id {}, expected {}",
                        process.debug_name,
                        message.as_u32(),
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                Ok(*target_process)
            }
        }
    }
}
