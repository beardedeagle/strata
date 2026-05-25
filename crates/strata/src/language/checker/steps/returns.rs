use super::*;
pub(in crate::language::checker) use matches::selected_step_return_match_action_statements;
use matches::{resolve_step_return_match, static_step_return_match_arm_state_args};

mod matches;

pub(super) struct StepReturnInput<'a> {
    pub(super) variant: CheckedMessageVariantId,
    pub(super) payload_guard: Option<&'a CheckedPayloadValue>,
    pub(super) payload_bindings: &'a [StepPayloadBinding],
    pub(super) state_payload_bindings: &'a [StepStatePayloadBinding],
    pub(super) body: &'a FunctionBlock,
}

pub(super) struct ResolvedStepReturn {
    pub(super) step_result: CheckedStepResult,
    pub(super) state_arg: ValueExpr,
    pub(super) action_statements: Vec<Statement>,
}

pub(super) struct StepReturnPreadmitContext<'ctx, 'state> {
    pub(super) module: &'ctx Module,
    pub(super) process: &'ctx Process,
    pub(super) process_id: CheckedProcessId,
    pub(super) semantic_index: &'ctx SemanticIndex,
    pub(super) message_cases: &'ctx MessageCaseTable,
    pub(super) state_space: &'ctx mut StateSpace<'state>,
    pub(super) types: &'ctx mut CheckedTypeInterner<'state>,
}

#[derive(Clone, Copy)]
pub(in crate::language::checker) struct StepReturnMatchPreadmitBindings<'source, 'binding> {
    pub(in crate::language::checker) source: &'source [SourceValueBinding<'binding>],
    pub(in crate::language::checker) static_match: &'source [SourceValueBinding<'binding>],
}

pub(super) fn step_source_bindings<'a>(
    payload_bindings: &'a [StepPayloadBinding],
    state_payload_bindings: &'a [StepStatePayloadBinding],
) -> Vec<SourceValueBinding<'a>> {
    let mut bindings = Vec::with_capacity(
        payload_bindings
            .len()
            .saturating_add(state_payload_bindings.len()),
    );
    for binding in payload_bindings {
        bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    for binding in state_payload_bindings {
        bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    bindings
}

pub(super) fn resolve_step_return(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    input: &StepReturnInput<'_>,
) -> Result<ResolvedStepReturn> {
    match &input.body.returns {
        ReturnExpr::Call { name, arg } => step_result_call(name, arg, "step body"),
        ReturnExpr::Match(match_body) => resolve_step_return_match(
            module,
            process,
            semantic_index,
            function_scope,
            source_bindings,
            input,
            match_body,
        ),
        ReturnExpr::IfElse { .. } => Err(step_return_shape_error("step body")),
        ReturnExpr::Value(_) => Err(step_return_shape_error("step body")),
    }
}

pub(super) fn preadmit_step_return_state_value(
    context: &mut StepReturnPreadmitContext<'_, '_>,
    input: &StepReturnInput<'_>,
) -> Result<()> {
    if let ReturnExpr::IfElse {
        then_branch,
        else_branch,
        ..
    } = &input.body.returns
    {
        let then_input = StepReturnInput {
            variant: input.variant,
            payload_guard: input.payload_guard,
            payload_bindings: input.payload_bindings,
            state_payload_bindings: input.state_payload_bindings,
            body: then_branch,
        };
        preadmit_step_return_state_value(context, &then_input)?;
        let else_input = StepReturnInput {
            variant: input.variant,
            payload_guard: input.payload_guard,
            payload_bindings: input.payload_bindings,
            state_payload_bindings: input.state_payload_bindings,
            body: else_branch,
        };
        return preadmit_step_return_state_value(context, &else_input);
    }

    let source_bindings =
        step_source_bindings(input.payload_bindings, input.state_payload_bindings);
    let module = context.module;
    let process = context.process;
    let semantic_index = context.semantic_index;
    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        process_refs: None,
        semantic_index,
    };
    let resolved = resolve_step_return(
        module,
        process,
        semantic_index,
        &function_scope,
        &source_bindings,
        input,
    )?;
    let state_arg = resolve_source_value_expr(
        &function_scope,
        &process.state_type,
        &resolved.state_arg,
        &source_bindings,
        0,
    )?;
    preadmit_step_state_arg(context, input, &state_arg)
}

pub(in crate::language::checker) fn preadmit_static_step_return_match_state_values(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    bindings: StepReturnMatchPreadmitBindings<'_, '_>,
    match_body: &Match,
) -> Result<()> {
    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        process_refs: None,
        semantic_index,
    };
    for state_arg in static_step_return_match_arm_state_args(
        module,
        process,
        semantic_index,
        bindings.source,
        bindings.static_match,
        match_body,
    )? {
        let state_arg =
            resolve_source_value_expr(&function_scope, &process.state_type, &state_arg, &[], 0)?;
        state_space.resolve_state_value(semantic_index, types, &state_arg)?;
    }
    Ok(())
}

fn step_result_call(
    name: &Identifier,
    arg: &ValueExpr,
    context: &str,
) -> Result<ResolvedStepReturn> {
    let step_result = match name.as_str() {
        "Stop" => CheckedStepResult::Stop,
        "Continue" => CheckedStepResult::Continue,
        "Panic" => CheckedStepResult::Panic,
        _ => return Err(step_return_shape_error(context)),
    };
    Ok(ResolvedStepReturn {
        step_result,
        state_arg: arg.clone(),
        action_statements: Vec::new(),
    })
}

fn step_return_shape_error(context: &str) -> Error {
    Error::new(format!(
        "{context} must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)"
    ))
}

fn preadmit_step_state_arg(
    context: &mut StepReturnPreadmitContext<'_, '_>,
    input: &StepReturnInput<'_>,
    state_arg: &ValueExpr,
) -> Result<()> {
    if matches!(state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        return Ok(());
    }

    let uses_payload = input
        .payload_bindings
        .iter()
        .any(|binding| source_value_uses_binding(state_arg, &binding.name));
    let uses_state = input
        .state_payload_bindings
        .iter()
        .any(|binding| source_value_uses_binding(state_arg, &binding.name));
    if !uses_payload && !uses_state {
        context.state_space.resolve_state_value(
            context.semantic_index,
            context.types,
            state_arg,
        )?;
        return Ok(());
    }
    if !uses_payload {
        let bindings = state_value_bindings(input);
        context.state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            context.types,
            state_arg,
            &bindings,
        )?;
        return Ok(());
    }

    preadmit_step_state_arg_with_payload_bindings(context, input, state_arg)
}

fn preadmit_step_state_arg_with_payload_bindings(
    context: &mut StepReturnPreadmitContext<'_, '_>,
    input: &StepReturnInput<'_>,
    state_arg: &ValueExpr,
) -> Result<()> {
    if let Some(payload) = input.payload_guard {
        let bindings = concrete_value_bindings_for_payload(
            context.module,
            context.semantic_index,
            context.process,
            input,
            payload,
        )?;
        context.state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            context.types,
            state_arg,
            &bindings,
        )?;
        return Ok(());
    }

    for payload in context
        .message_cases
        .payload_values(context.process_id, input.variant)?
    {
        let bindings = concrete_value_bindings_for_payload(
            context.module,
            context.semantic_index,
            context.process,
            input,
            payload,
        )?;
        context.state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            context.types,
            state_arg,
            &bindings,
        )?;
    }
    Ok(())
}

fn concrete_value_bindings_for_payload<'a>(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    input: &'a StepReturnInput<'_>,
    payload: &CheckedPayloadValue,
) -> Result<Vec<ValueBinding<'a>>> {
    let mut bindings = Vec::with_capacity(
        input
            .payload_bindings
            .len()
            .saturating_add(input.state_payload_bindings.len()),
    );
    for binding in input.payload_bindings {
        let (label, value) = checked_payload_binding(
            module,
            semantic_index,
            payload,
            &PatternPayloadParam {
                name: binding.name.clone(),
                ty: binding.ty.clone(),
                path: binding.path.clone(),
            },
        )?
        .ok_or_else(|| {
            Error::new(format!(
                "process {} message payload {} does not match step return binding {}",
                process.name,
                payload.label(),
                binding.name
            ))
        })?;
        bindings.push(ValueBinding {
            name: &binding.name,
            ty: &binding.ty,
            label,
            value,
        });
    }
    append_state_value_bindings(input, &mut bindings);
    Ok(bindings)
}

fn state_value_bindings<'a>(input: &'a StepReturnInput<'_>) -> Vec<ValueBinding<'a>> {
    let mut bindings = Vec::with_capacity(input.state_payload_bindings.len());
    append_state_value_bindings(input, &mut bindings);
    bindings
}

fn append_state_value_bindings<'a>(
    input: &'a StepReturnInput<'_>,
    bindings: &mut Vec<ValueBinding<'a>>,
) {
    bindings.extend(
        input
            .state_payload_bindings
            .iter()
            .map(|binding| ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: binding.label.clone(),
                value: Some(binding.value.clone()),
            }),
    );
}
