use super::super::source_functions::validate_source_function_value_expr;
use super::returns::{StepReturnInput, resolve_step_return, step_source_bindings};
use super::*;
use mantle_artifact::{MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH, MAX_VALUE_TEMPLATE_FIELDS};

mod actions;
mod send;

use actions::checked_actions_for_statements;

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
    let mut loop_elements = LoopElementAllocator::default();
    let outcome = check_step_block_outcome(
        context,
        state_space,
        outputs,
        types,
        &function_scope,
        &source_bindings,
        &template_bindings,
        &input,
        &mut loop_elements,
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

#[derive(Debug, Clone, Copy)]
enum RuntimeIfBranchScope {
    Outside,
    Statement,
    FinalPosition,
}

#[derive(Debug, Clone, Copy)]
struct ActionCheckScope {
    in_loop_body: bool,
    runtime_if_branch: RuntimeIfBranchScope,
    statement_if_depth: usize,
}

impl ActionCheckScope {
    const TOP_LEVEL: Self = Self {
        in_loop_body: false,
        runtime_if_branch: RuntimeIfBranchScope::Outside,
        statement_if_depth: 0,
    };

    const fn for_loop_body(self) -> Self {
        Self {
            in_loop_body: true,
            runtime_if_branch: self.runtime_if_branch,
            statement_if_depth: self.statement_if_depth,
        }
    }

    const fn for_statement_if_branch(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            runtime_if_branch: RuntimeIfBranchScope::Statement,
            statement_if_depth: self.statement_if_depth.saturating_add(1),
        }
    }

    const fn for_final_runtime_if_branch(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            runtime_if_branch: RuntimeIfBranchScope::FinalPosition,
            statement_if_depth: self.statement_if_depth,
        }
    }

    fn validate_statement_if_allowed(self, process: &Identifier) -> Result<()> {
        if matches!(self.runtime_if_branch, RuntimeIfBranchScope::FinalPosition) {
            return Err(Error::new(format!(
                "process {process} nested statement-level if branches are not supported in this source slice"
            )));
        }
        if self.in_loop_body && self.statement_if_depth > 0 {
            return Err(Error::new(format!(
                "process {process} nested statement-level if branches are not supported in loop bodies in this source slice"
            )));
        }
        if self.statement_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
            return Err(Error::new(format!(
                "process {process} statement-level if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH} in this source slice"
            )));
        }
        Ok(())
    }

    fn runtime_if_branch_label(self) -> &'static str {
        match self.runtime_if_branch {
            RuntimeIfBranchScope::Outside => "runtime if branch",
            RuntimeIfBranchScope::Statement => "statement-level if branch",
            RuntimeIfBranchScope::FinalPosition => "final-position runtime if branch",
        }
    }
}

#[derive(Default)]
struct LoopElementAllocator {
    next: usize,
}

impl LoopElementAllocator {
    fn next_id(&mut self) -> Result<CheckedLoopElementId> {
        if self.next >= MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "loop element count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        let id = CheckedLoopElementId::from_index(self.next)?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| Error::new("loop element id overflowed"))?;
        Ok(id)
    }
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
    loop_elements: &mut LoopElementAllocator,
    body: &FunctionBlock,
) -> Result<CheckedBlockOutcome> {
    let mut actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input.payload_bindings,
        loop_elements,
        ActionCheckScope::TOP_LEVEL,
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
        loop_elements,
        body,
    )?;
    actions.extend(outcome.actions);
    Ok(CheckedBlockOutcome { actions, ..outcome })
}

#[allow(clippy::too_many_arguments)]
fn check_runtime_if_branch_block_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    input: &StepTransitionInput<'_>,
    loop_elements: &mut LoopElementAllocator,
    body: &FunctionBlock,
) -> Result<CheckedBlockOutcome> {
    if matches!(body.returns, ReturnExpr::IfElse { .. }) {
        return Err(Error::new(format!(
            "process {} nested runtime if branches are not supported in this source slice",
            context.process.name
        )));
    }

    let mut actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input.payload_bindings,
        loop_elements,
        ActionCheckScope::TOP_LEVEL.for_final_runtime_if_branch(),
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
        loop_elements,
        body,
    )?;
    actions.extend(outcome.actions);
    Ok(CheckedBlockOutcome { actions, ..outcome })
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
    loop_elements: &mut LoopElementAllocator,
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
            loop_elements,
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
    loop_elements: &mut LoopElementAllocator,
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
    let then_outcome = check_runtime_if_branch_block_outcome(
        context,
        state_space,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input,
        loop_elements,
        then_branch,
    )?;
    let else_outcome = check_runtime_if_branch_block_outcome(
        context,
        state_space,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        input,
        loop_elements,
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
