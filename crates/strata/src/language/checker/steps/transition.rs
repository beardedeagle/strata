use super::super::source_functions::validate_source_function_value_expr;
use super::returns::{StepReturnInput, resolve_step_return, step_source_bindings};
use super::*;

pub(super) fn check_step_transition(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    input: StepTransitionInput<'_>,
) -> Result<CheckedTransition> {
    let payload_template_bindings =
        input
            .payload_bindings
            .iter()
            .map(|binding| ValueTemplateBinding {
                name: &binding.name,
                ty: &binding.ty,
                checked_ty: &binding.checked_ty,
                root_checked_ty: &binding.checked_payload_ty,
                source: ValueTemplateSource::ReceivedPayload,
                path: &binding.path,
            });
    let state_template_bindings =
        input
            .state_payload_bindings
            .iter()
            .map(|binding| ValueTemplateBinding {
                name: &binding.name,
                ty: &binding.ty,
                checked_ty: &binding.checked_ty,
                root_checked_ty: &binding.checked_payload_ty,
                source: ValueTemplateSource::CurrentStatePayload,
                path: &binding.path,
            });
    let template_bindings = payload_template_bindings
        .chain(state_template_bindings)
        .collect::<Vec<_>>();
    let function_scope = SourceFunctionScope {
        module: context.module,
        process_name: Some(&context.process.name),
        process_functions: &context.process.functions,
        semantic_index: context.semantic_index,
    };
    let source_bindings =
        step_source_bindings(input.payload_bindings, input.state_payload_bindings);
    let outcome = check_step_block_outcome(
        context,
        state_space,
        outputs,
        types,
        &function_scope,
        &source_bindings,
        &template_bindings,
        &input,
        input.body,
    )?;
    let transition = CheckedTransition::new(CheckedTransitionParts {
        current_state: input.current_state,
        message: input.message,
        step_result: outcome.step_result,
        next_state: outcome.next_state,
        effects: input.declared_effects.to_vec(),
        actions: outcome.actions,
    });
    Ok(match input.payload_guard {
        Some(payload_guard) => transition.with_payload_guard(payload_guard.clone()),
        None => transition,
    })
}

struct CheckedBlockOutcome {
    step_result: CheckedStepResult,
    next_state: CheckedNextState,
    actions: Vec<CheckedAction>,
}

#[allow(clippy::too_many_arguments)]
fn check_step_block_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    input: &StepTransitionInput<'_>,
    body: &FunctionBlock,
) -> Result<CheckedBlockOutcome> {
    let mut actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        source_bindings,
        template_bindings,
        input.payload_bindings,
        &body.statements,
    )?;
    let outcome = checked_return_outcome(
        context,
        state_space,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input,
        body,
    )?;
    actions.extend(outcome.actions);
    Ok(CheckedBlockOutcome { actions, ..outcome })
}

#[allow(clippy::too_many_arguments)]
fn checked_actions_for_statements(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    statements: &[Statement],
) -> Result<Vec<CheckedAction>> {
    let mut actions = Vec::with_capacity(statements.len());
    for statement in statements {
        match statement {
            Statement::Emit(text) => {
                actions.push(CheckedAction::Emit {
                    output: outputs.intern(text.as_str())?,
                });
            }
            Statement::LetProcessRef { name, target, .. } => {
                let binding = context.process_ref_index.get(name).ok_or_else(|| {
                    Error::new(format!(
                        "process {} process reference {} was not resolved",
                        context.process.name, name
                    ))
                })?;
                actions.push(CheckedAction::Spawn {
                    target: context.semantic_index.process_id(target)?,
                    process_ref: binding.id,
                });
            }
            Statement::Send {
                target,
                message,
                payload,
            } => {
                let send_target = resolve_checked_send_target(context, payload_bindings, target)?;
                let message_id = resolve_send_message_case(
                    context,
                    types,
                    send_target.target_process,
                    message,
                    payload.as_ref(),
                    source_bindings,
                    template_bindings,
                )?;
                actions.push(CheckedAction::Send {
                    target: send_target.target,
                    message: message_id.message,
                    payload: message_id.payload.map(Box::new),
                });
            }
        }
    }
    Ok(actions)
}

#[allow(clippy::too_many_arguments)]
fn checked_return_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    input: &StepTransitionInput<'_>,
    body: &FunctionBlock,
) -> Result<CheckedBlockOutcome> {
    if let ReturnExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = &body.returns
    {
        return checked_if_else_return_outcome(
            context,
            state_space,
            outputs,
            types,
            function_scope,
            source_bindings,
            template_bindings,
            input,
            condition,
            then_branch,
            else_branch,
        );
    }

    let step_return = resolve_step_return(
        context.module,
        context.process,
        context.semantic_index,
        function_scope,
        source_bindings,
        &StepReturnInput {
            variant: input.variant,
            payload_guard: input.payload_guard,
            payload_bindings: input.payload_bindings,
            state_payload_bindings: input.state_payload_bindings,
            body,
        },
    )?;
    let next_state = checked_next_state_for_arg(
        context,
        state_space,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input,
        &step_return.state_arg,
    )?;
    Ok(CheckedBlockOutcome {
        step_result: step_return.step_result,
        next_state,
        actions: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
fn checked_if_else_return_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    input: &StepTransitionInput<'_>,
    condition: &ValueExpr,
    then_branch: &FunctionBlock,
    else_branch: &FunctionBlock,
) -> Result<CheckedBlockOutcome> {
    let condition = checked_runtime_bool_condition(
        context,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        condition,
    )?;
    let then_outcome = check_step_block_outcome(
        context,
        state_space,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input,
        then_branch,
    )?;
    let else_outcome = check_step_block_outcome(
        context,
        state_space,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input,
        else_branch,
    )?;
    if then_outcome.step_result != else_outcome.step_result {
        return Err(Error::new(format!(
            "process {} runtime if branches must return the same step result",
            context.process.name
        )));
    }
    let actions = if then_outcome.actions.is_empty() && else_outcome.actions.is_empty() {
        Vec::new()
    } else {
        vec![CheckedAction::IfElse {
            condition: condition.clone(),
            then_actions: then_outcome.actions,
            else_actions: else_outcome.actions,
        }]
    };
    Ok(CheckedBlockOutcome {
        step_result: then_outcome.step_result,
        next_state: CheckedNextState::IfElse {
            condition,
            then_state: Box::new(then_outcome.next_state),
            else_state: Box::new(else_outcome.next_state),
        },
        actions,
    })
}

fn checked_runtime_bool_condition(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    condition: &ValueExpr,
) -> Result<CheckedValueTemplate> {
    let bool_type = context.semantic_index.bool_type(context.module)?;
    validate_source_function_value_expr(function_scope, &bool_type, condition, source_bindings)
        .map_err(|err| Error::new(format!("if condition must have type {bool_type}: {err}")))?;
    let resolved =
        resolve_source_value_expr(function_scope, &bool_type, condition, source_bindings, 0)?;
    checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        &bool_type,
        &resolved,
        template_bindings,
    )
}

#[allow(clippy::too_many_arguments)]
fn checked_next_state_for_arg(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    input: &StepTransitionInput<'_>,
    state_arg: &ValueExpr,
) -> Result<CheckedNextState> {
    if let ValueExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = state_arg
    {
        let condition = checked_runtime_bool_condition(
            context,
            types,
            function_scope,
            source_bindings,
            template_bindings,
            condition,
        )?;
        let then_state = checked_next_state_for_arg(
            context,
            state_space,
            types,
            function_scope,
            source_bindings,
            template_bindings,
            input,
            then_branch,
        )?;
        let else_state = checked_next_state_for_arg(
            context,
            state_space,
            types,
            function_scope,
            source_bindings,
            template_bindings,
            input,
            else_branch,
        )?;
        return Ok(CheckedNextState::IfElse {
            condition,
            then_state: Box::new(then_state),
            else_state: Box::new(else_state),
        });
    }

    let state_arg = resolve_source_value_expr(
        function_scope,
        &context.process.state_type,
        state_arg,
        source_bindings,
        0,
    )?;
    if matches!(&state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        return Ok(CheckedNextState::Current);
    }
    if template_bindings
        .iter()
        .any(|binding| source_value_uses_binding(&state_arg, binding.name))
    {
        let template = checked_value_template_with_binding(
            context.module,
            context.semantic_index,
            types,
            &context.process.state_type,
            &state_arg,
            template_bindings,
        )?;
        populate_template_state_values(
            context,
            state_space,
            types,
            input.variant,
            input.payload_guard,
            &state_arg,
            input.payload_bindings,
            input.state_payload_bindings,
        )?;
        return Ok(CheckedNextState::Template(template));
    }
    Ok(CheckedNextState::Value(state_space.resolve_state_value(
        context.semantic_index,
        types,
        &state_arg,
    )?))
}

struct ResolvedCheckedSendTarget {
    target: CheckedSendTarget,
    target_process: CheckedProcessId,
}

fn resolve_checked_send_target(
    context: &StepCheckContext<'_>,
    payload_bindings: &[StepPayloadBinding],
    target: &Identifier,
) -> Result<ResolvedCheckedSendTarget> {
    if let Some(binding) = context.process_ref_index.get(target) {
        return Ok(ResolvedCheckedSendTarget {
            target: CheckedSendTarget::ProcessRef(binding.id),
            target_process: binding.target,
        });
    }
    if let Some(binding) = payload_bindings
        .iter()
        .find(|binding| binding.name == *target)
    {
        let target_process = context
            .semantic_index
            .process_ref_target_type(&binding.ty)?
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} send target {} is not a process reference payload",
                    context.process.name, target
                ))
            })?;
        return Ok(ResolvedCheckedSendTarget {
            target: CheckedSendTarget::ReceivedPayload {
                ty: binding.checked_ty.clone(),
                target: target_process,
            },
            target_process,
        });
    }
    Err(Error::new(format!(
        "process {} sends to undeclared process reference {}",
        context.process.name, target
    )))
}

struct CheckedSendMessage {
    message: CheckedMessageId,
    payload: Option<CheckedValueTemplate>,
}

fn resolve_send_message_case(
    context: &mut StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    target_process: CheckedProcessId,
    message: &Identifier,
    payload: Option<&ValueExpr>,
    source_bindings: &[SourceValueBinding<'_>],
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<CheckedSendMessage> {
    let variant = context.semantic_index.message_id_for_process(
        context.module,
        context.process.name.as_str(),
        target_process,
        message,
    )?;
    let variant_decl =
        context
            .semantic_index
            .message_variant(context.module, target_process, variant)?;
    let payload = match (&variant_decl.payload_type, payload) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(Error::new(format!(
                "process {} sends payload to message {}, which does not accept one",
                context.process.name, variant_decl.name
            )));
        }
        (Some(_), None) => {
            return Err(Error::new(format!(
                "process {} sends message {} without required payload",
                context.process.name, variant_decl.name
            )));
        }
        (Some(payload_type), Some(payload)) => {
            let resolved_payload = {
                let function_scope = SourceFunctionScope {
                    module: context.module,
                    process_name: Some(&context.process.name),
                    process_functions: &context.process.functions,
                    semantic_index: context.semantic_index,
                };
                resolve_source_value_expr(
                    &function_scope,
                    payload_type,
                    payload,
                    source_bindings,
                    0,
                )?
            };
            Some(checked_send_payload_template(
                context,
                types,
                payload_type,
                &resolved_payload,
                bindings,
            )?)
        }
    };
    Ok(CheckedSendMessage {
        message: context.message_cases.message_id(target_process, variant)?,
        payload,
    })
}

fn checked_send_payload_template(
    context: &mut StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    payload: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<CheckedValueTemplate> {
    if let Some(target_process) = context
        .semantic_index
        .process_ref_target_type(expected_type)?
    {
        let ValueExpr::Identifier(name) = payload else {
            return Err(Error::new(format!(
                "process {} sends process reference payload of type {} using a non-reference value",
                context.process.name, expected_type
            )));
        };
        if let Some(binding) = bindings.iter().find(|binding| name == binding.name) {
            if binding.ty == expected_type {
                return Ok(match binding.source {
                    ValueTemplateSource::ReceivedPayload => CheckedValueTemplate::ReceivedPayload {
                        ty: binding.checked_ty.clone(),
                    },
                    ValueTemplateSource::CurrentStatePayload => {
                        CheckedValueTemplate::CurrentStatePayload {
                            ty: binding.checked_ty.clone(),
                        }
                    }
                });
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
        let process_ref = context.process_ref_index.get(name).ok_or_else(|| {
            Error::new(format!(
                "process {} payload {} is not a bound process reference",
                context.process.name, name
            ))
        })?;
        if process_ref.target != target_process {
            return Err(Error::new(format!(
                "process {} payload {} targets process id {}, expected {}",
                context.process.name,
                name,
                process_ref.target.as_u32(),
                target_process.as_u32()
            )));
        }
        return Ok(CheckedValueTemplate::ProcessRef {
            ty: types.intern(expected_type)?,
            target: target_process,
            process_ref: process_ref.id,
        });
    }

    checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        expected_type,
        payload,
        bindings,
    )
}

#[allow(clippy::too_many_arguments)]
fn populate_template_state_values(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    variant: CheckedMessageVariantId,
    payload_guard: Option<&CheckedPayloadValue>,
    state_arg: &ValueExpr,
    payload_bindings: &[StepPayloadBinding],
    state_payload_bindings: &[StepStatePayloadBinding],
) -> Result<()> {
    if !payload_bindings.is_empty() {
        let payloads = match payload_guard {
            Some(payload) => vec![payload],
            None => context
                .message_cases
                .payload_values(context.process_id, variant)?
                .iter()
                .collect::<Vec<_>>(),
        };
        for payload in payloads {
            let payload_values = payload_bindings
                .iter()
                .map(|binding| {
                    checked_payload_binding(
                        context.module,
                        context.semantic_index,
                        payload,
                        &PatternPayloadParam {
                            name: binding.name.clone(),
                            ty: binding.ty.clone(),
                            path: binding.path.clone(),
                        },
                    )?
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} message payload {} does not match step pattern binding {}",
                            context.process.name,
                            payload.label(),
                            binding.name
                        ))
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let mut bindings = Vec::new();
            for (binding, (label, value)) in payload_bindings.iter().zip(&payload_values) {
                bindings.push(ValueBinding {
                    name: &binding.name,
                    ty: &binding.ty,
                    label: label.clone(),
                    value: value.clone(),
                });
            }
            for state_binding in state_payload_bindings {
                bindings.push(ValueBinding {
                    name: &state_binding.name,
                    ty: &state_binding.ty,
                    label: state_binding.label.clone(),
                    value: Some(state_binding.value.clone()),
                });
            }
            state_space.resolve_state_value_with_bindings(
                context.semantic_index,
                types,
                state_arg,
                &bindings,
            )?;
        }
        return Ok(());
    }
    if !state_payload_bindings.is_empty() {
        let bindings = state_payload_bindings
            .iter()
            .map(|binding| ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: binding.label.clone(),
                value: Some(binding.value.clone()),
            })
            .collect::<Vec<_>>();
        state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            types,
            state_arg,
            &bindings,
        )?;
    }
    Ok(())
}
