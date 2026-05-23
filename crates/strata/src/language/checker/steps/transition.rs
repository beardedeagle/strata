use super::super::source_functions::validate_source_function_value_expr;
use super::returns::{StepReturnInput, resolve_step_return, step_source_bindings};
use super::*;
use mantle_artifact::{
    MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH, MAX_NEXT_STATE_IF_ELSE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS,
};

mod actions;
mod return_match_actions;
mod send;

use actions::{ActionCheckInput, checked_actions_for_statements};
use return_match_actions::validate_return_match_arm_action_statements;

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
    let transition_env = StepTransitionEnv {
        function_scope: &function_scope,
        source_bindings: &source_bindings,
        template_bindings: &template_bindings,
        input: &input,
    };
    let outcome = check_step_block_outcome(
        context,
        state_space,
        outputs,
        types,
        &mut loop_elements,
        transition_env,
        StepBlockInput {
            body: input.body,
            next_state_if_depth: 0,
        },
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

#[derive(Clone, Copy)]
struct StepTransitionEnv<'a, 'scope, 'source, 'template, 'input> {
    function_scope: &'a SourceFunctionScope<'scope>,
    source_bindings: &'a [SourceValueBinding<'source>],
    template_bindings: &'a [ValueTemplateBinding<'template>],
    input: &'a StepTransitionInput<'input>,
}

#[derive(Clone, Copy)]
struct StepBlockInput<'a> {
    body: &'a FunctionBlock,
    next_state_if_depth: usize,
}

#[derive(Clone, Copy)]
struct RuntimeIfReturnInput<'a> {
    condition: &'a ValueExpr,
    then_branch: &'a FunctionBlock,
    else_branch: &'a FunctionBlock,
    next_state_if_depth: usize,
}

#[derive(Clone, Copy)]
struct NextStateInput<'a> {
    state_arg: &'a ValueExpr,
    next_state_if_depth: usize,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeIfBranchScope {
    Outside,
    Statement,
    FinalPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepReturnMatchForBodyScope {
    Outside,
    DirectArm,
    RuntimeIfBranch,
}

#[derive(Debug, Clone, Copy)]
struct ActionCheckScope {
    in_loop_body: bool,
    in_step_return_match_arm: bool,
    runtime_if_branch: RuntimeIfBranchScope,
    statement_if_depth: usize,
    step_return_match_for_body: StepReturnMatchForBodyScope,
}

impl ActionCheckScope {
    const TOP_LEVEL: Self = Self {
        in_loop_body: false,
        in_step_return_match_arm: false,
        runtime_if_branch: RuntimeIfBranchScope::Outside,
        statement_if_depth: 0,
        step_return_match_for_body: StepReturnMatchForBodyScope::Outside,
    };

    const fn for_loop_body(self) -> Self {
        let step_return_match_for_body = if self.in_step_return_match_arm {
            match self.runtime_if_branch {
                RuntimeIfBranchScope::Outside => StepReturnMatchForBodyScope::DirectArm,
                RuntimeIfBranchScope::Statement | RuntimeIfBranchScope::FinalPosition => {
                    StepReturnMatchForBodyScope::RuntimeIfBranch
                }
            }
        } else {
            StepReturnMatchForBodyScope::Outside
        };
        Self {
            in_loop_body: true,
            in_step_return_match_arm: self.in_step_return_match_arm,
            runtime_if_branch: self.runtime_if_branch,
            statement_if_depth: self.statement_if_depth,
            step_return_match_for_body,
        }
    }

    const fn for_statement_if_branch(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            in_step_return_match_arm: self.in_step_return_match_arm,
            runtime_if_branch: RuntimeIfBranchScope::Statement,
            statement_if_depth: self.statement_if_depth.saturating_add(1),
            step_return_match_for_body: self.step_return_match_for_body,
        }
    }

    const fn for_final_runtime_if_branch(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            in_step_return_match_arm: self.in_step_return_match_arm,
            runtime_if_branch: RuntimeIfBranchScope::FinalPosition,
            statement_if_depth: self.statement_if_depth.saturating_add(1),
            step_return_match_for_body: self.step_return_match_for_body,
        }
    }

    const fn for_step_return_match_arm(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            in_step_return_match_arm: true,
            runtime_if_branch: self.runtime_if_branch,
            statement_if_depth: self.statement_if_depth,
            step_return_match_for_body: self.step_return_match_for_body,
        }
    }

    fn validate_statement_if_allowed(self, process: &Identifier) -> Result<()> {
        if self.statement_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
            return Err(Error::new(format!(
                "process {process} statement-level if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH} in this source slice"
            )));
        }
        Ok(())
    }

    const fn allows_step_return_match_loop_body_if(self) -> bool {
        match self.step_return_match_for_body {
            StepReturnMatchForBodyScope::Outside => false,
            StepReturnMatchForBodyScope::DirectArm => {
                matches!(self.runtime_if_branch, RuntimeIfBranchScope::Outside)
                    && self.statement_if_depth == 0
            }
            StepReturnMatchForBodyScope::RuntimeIfBranch => {
                matches!(self.runtime_if_branch, RuntimeIfBranchScope::Statement)
                    && self.statement_if_depth == 1
            }
        }
    }

    const fn allows_step_return_match_runtime_if_branch_for(self) -> bool {
        self.in_step_return_match_arm
            && !self.in_loop_body
            && matches!(self.runtime_if_branch, RuntimeIfBranchScope::Statement)
            && self.statement_if_depth == 1
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

fn check_step_block_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    loop_elements: &mut LoopElementAllocator,
    env: StepTransitionEnv<'_, '_, '_, '_, '_>,
    block: StepBlockInput<'_>,
) -> Result<CheckedBlockOutcome> {
    let mut actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        loop_elements,
        ActionCheckInput {
            function_scope: env.function_scope,
            source_bindings: env.source_bindings,
            template_bindings: env.template_bindings,
            payload_bindings: env.input.payload_bindings,
            scope: ActionCheckScope::TOP_LEVEL,
        },
        &block.body.statements,
    )?;
    let outcome = checked_return_outcome(
        context,
        state_space,
        outputs,
        types,
        loop_elements,
        env,
        block,
    )?;
    actions.extend(outcome.actions);
    Ok(CheckedBlockOutcome { actions, ..outcome })
}

fn check_runtime_if_branch_block_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    loop_elements: &mut LoopElementAllocator,
    env: StepTransitionEnv<'_, '_, '_, '_, '_>,
    block: StepBlockInput<'_>,
) -> Result<CheckedBlockOutcome> {
    let mut actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        loop_elements,
        ActionCheckInput {
            function_scope: env.function_scope,
            source_bindings: env.source_bindings,
            template_bindings: env.template_bindings,
            payload_bindings: env.input.payload_bindings,
            scope: ActionCheckScope::TOP_LEVEL.for_final_runtime_if_branch(),
        },
        &block.body.statements,
    )?;
    let outcome = checked_return_outcome(
        context,
        state_space,
        outputs,
        types,
        loop_elements,
        env,
        block,
    )?;
    actions.extend(outcome.actions);
    Ok(CheckedBlockOutcome { actions, ..outcome })
}

fn checked_return_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    loop_elements: &mut LoopElementAllocator,
    env: StepTransitionEnv<'_, '_, '_, '_, '_>,
    block: StepBlockInput<'_>,
) -> Result<CheckedBlockOutcome> {
    if let ReturnExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = &block.body.returns
    {
        return checked_if_else_return_outcome(
            context,
            state_space,
            outputs,
            types,
            loop_elements,
            env,
            RuntimeIfReturnInput {
                condition,
                then_branch,
                else_branch,
                next_state_if_depth: block.next_state_if_depth,
            },
        );
    }

    let step_return = resolve_step_return(
        context.module,
        context.process,
        context.semantic_index,
        env.function_scope,
        env.source_bindings,
        &StepReturnInput {
            variant: env.input.variant,
            payload_guard: env.input.payload_guard,
            payload_bindings: env.input.payload_bindings,
            state_payload_bindings: env.input.state_payload_bindings,
            body: block.body,
        },
    )?;
    validate_return_match_arm_action_statements(
        context,
        types,
        env.function_scope,
        env.source_bindings,
        env.template_bindings,
        env.input,
        block.body,
    )?;
    let next_state = checked_next_state_for_arg(
        context,
        state_space,
        types,
        env,
        NextStateInput {
            state_arg: &step_return.state_arg,
            next_state_if_depth: block.next_state_if_depth,
        },
    )?;
    let actions = if step_return.action_statements.is_empty() {
        Vec::new()
    } else {
        checked_actions_for_statements(
            context,
            outputs,
            types,
            loop_elements,
            ActionCheckInput {
                function_scope: env.function_scope,
                source_bindings: env.source_bindings,
                template_bindings: env.template_bindings,
                payload_bindings: env.input.payload_bindings,
                scope: ActionCheckScope::TOP_LEVEL.for_step_return_match_arm(),
            },
            &step_return.action_statements,
        )?
    };
    Ok(CheckedBlockOutcome {
        step_result: step_return.step_result,
        next_state,
        actions,
    })
}

fn checked_if_else_return_outcome(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    loop_elements: &mut LoopElementAllocator,
    env: StepTransitionEnv<'_, '_, '_, '_, '_>,
    runtime_if: RuntimeIfReturnInput<'_>,
) -> Result<CheckedBlockOutcome> {
    let branch_next_state_if_depth = checked_next_state_if_child_depth(
        context.process.name.as_str(),
        runtime_if.next_state_if_depth,
    )?;
    let condition = checked_runtime_bool_condition(
        context,
        types,
        env.function_scope,
        env.source_bindings,
        env.template_bindings,
        runtime_if.condition,
    )?;
    let then_outcome = check_runtime_if_branch_block_outcome(
        context,
        state_space,
        outputs,
        types,
        loop_elements,
        env,
        StepBlockInput {
            body: runtime_if.then_branch,
            next_state_if_depth: branch_next_state_if_depth,
        },
    )?;
    let else_outcome = check_runtime_if_branch_block_outcome(
        context,
        state_space,
        outputs,
        types,
        loop_elements,
        env,
        StepBlockInput {
            body: runtime_if.else_branch,
            next_state_if_depth: branch_next_state_if_depth,
        },
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

fn checked_next_state_for_arg(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    env: StepTransitionEnv<'_, '_, '_, '_, '_>,
    next_state: NextStateInput<'_>,
) -> Result<CheckedNextState> {
    if let ValueExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = next_state.state_arg
    {
        let condition = checked_runtime_bool_condition(
            context,
            types,
            env.function_scope,
            env.source_bindings,
            env.template_bindings,
            condition,
        )?;
        let child_next_state_if_depth = checked_next_state_if_child_depth(
            context.process.name.as_str(),
            next_state.next_state_if_depth,
        )?;
        let then_state = checked_next_state_for_arg(
            context,
            state_space,
            types,
            env,
            NextStateInput {
                state_arg: then_branch,
                next_state_if_depth: child_next_state_if_depth,
            },
        )?;
        let else_state = checked_next_state_for_arg(
            context,
            state_space,
            types,
            env,
            NextStateInput {
                state_arg: else_branch,
                next_state_if_depth: child_next_state_if_depth,
            },
        )?;
        return Ok(CheckedNextState::IfElse {
            condition,
            then_state: Box::new(then_state),
            else_state: Box::new(else_state),
        });
    }

    let state_arg = resolve_source_value_expr(
        env.function_scope,
        &context.process.state_type,
        next_state.state_arg,
        env.source_bindings,
        0,
    )?;
    if matches!(&state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        return Ok(CheckedNextState::Current);
    }
    if env
        .template_bindings
        .iter()
        .any(|binding| source_value_uses_binding(&state_arg, binding.name))
    {
        let template = checked_value_template_with_binding(
            context.module,
            context.semantic_index,
            types,
            &context.process.state_type,
            &state_arg,
            env.template_bindings,
        )?;
        populate_template_state_values(context, state_space, types, env, &state_arg)?;
        return Ok(CheckedNextState::Template(template));
    }
    Ok(CheckedNextState::Value(state_space.resolve_state_value(
        context.semantic_index,
        types,
        &state_arg,
    )?))
}

fn checked_next_state_if_child_depth(process: &str, depth: usize) -> Result<usize> {
    if depth >= MAX_NEXT_STATE_IF_ELSE_DEPTH {
        return Err(Error::new(format!(
            "process {process} next_state runtime if nesting exceeds maximum depth of {MAX_NEXT_STATE_IF_ELSE_DEPTH} in this source slice"
        )));
    }
    Ok(depth + 1)
}

fn populate_template_state_values(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    env: StepTransitionEnv<'_, '_, '_, '_, '_>,
    state_arg: &ValueExpr,
) -> Result<()> {
    if !env.input.payload_bindings.is_empty() {
        let payloads = match env.input.payload_guard {
            Some(payload) => vec![payload],
            None => context
                .message_cases
                .payload_values(context.process_id, env.input.variant)?
                .iter()
                .collect::<Vec<_>>(),
        };
        for payload in payloads {
            let payload_values = env
                .input
                .payload_bindings
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
            for (binding, (label, value)) in env.input.payload_bindings.iter().zip(&payload_values)
            {
                bindings.push(ValueBinding {
                    name: &binding.name,
                    ty: &binding.ty,
                    label: label.clone(),
                    value: value.clone(),
                });
            }
            for state_binding in env.input.state_payload_bindings {
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
    if !env.input.state_payload_bindings.is_empty() {
        let bindings = env
            .input
            .state_payload_bindings
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
