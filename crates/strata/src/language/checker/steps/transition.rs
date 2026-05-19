use super::super::source_functions::validate_source_function_value_expr;
use super::returns::{StepReturnInput, resolve_step_return, step_source_bindings};
use super::*;
use mantle_artifact::MAX_VALUE_TEMPLATE_FIELDS;

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
}

impl ActionCheckScope {
    const TOP_LEVEL: Self = Self {
        in_loop_body: false,
        runtime_if_branch: RuntimeIfBranchScope::Outside,
    };

    const fn for_loop_body(self) -> Self {
        Self {
            in_loop_body: true,
            runtime_if_branch: self.runtime_if_branch,
        }
    }

    const fn for_statement_if_branch(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            runtime_if_branch: RuntimeIfBranchScope::Statement,
        }
    }

    const fn for_final_runtime_if_branch(self) -> Self {
        Self {
            in_loop_body: self.in_loop_body,
            runtime_if_branch: RuntimeIfBranchScope::FinalPosition,
        }
    }

    fn is_in_runtime_if_branch(self) -> bool {
        !matches!(self.runtime_if_branch, RuntimeIfBranchScope::Outside)
    }

    fn allows_for_each(self) -> bool {
        matches!(
            self.runtime_if_branch,
            RuntimeIfBranchScope::Outside | RuntimeIfBranchScope::Statement
        )
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
fn checked_actions_for_statements(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    loop_elements: &mut LoopElementAllocator,
    scope: ActionCheckScope,
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
                if scope.is_in_runtime_if_branch() {
                    return Err(Error::new(format!(
                        "process {} {} cannot bind process reference {} in this source slice",
                        context.process.name,
                        scope.runtime_if_branch_label(),
                        name
                    )));
                }
                if scope.in_loop_body {
                    return Err(Error::new(format!(
                        "process {} for loop body cannot bind process reference {} in this source slice",
                        context.process.name, name
                    )));
                }
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
            Statement::IfElse {
                condition,
                then_body,
                else_body,
            } => {
                if scope.is_in_runtime_if_branch() {
                    return Err(Error::new(format!(
                        "process {} nested statement-level if branches are not supported in this source slice",
                        context.process.name
                    )));
                }
                actions.push(checked_if_else_statement_action(
                    context,
                    outputs,
                    types,
                    function_scope,
                    source_bindings,
                    template_bindings,
                    payload_bindings,
                    loop_elements,
                    scope,
                    condition,
                    then_body,
                    else_body,
                )?);
            }
            Statement::ForEach {
                item,
                collection,
                body,
            } => {
                if scope.in_loop_body {
                    return Err(Error::new(format!(
                        "process {} nested for loops are not supported in this source slice",
                        context.process.name
                    )));
                }
                if !scope.allows_for_each() {
                    return Err(Error::new(format!(
                        "process {} final-position runtime if branch cannot contain for loop actions in this source slice",
                        context.process.name
                    )));
                }
                actions.push(checked_for_each_action(
                    context,
                    outputs,
                    types,
                    function_scope,
                    source_bindings,
                    template_bindings,
                    payload_bindings,
                    loop_elements,
                    item,
                    collection,
                    body,
                )?);
            }
        }
    }
    Ok(actions)
}

#[allow(clippy::too_many_arguments)]
fn checked_if_else_statement_action(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    loop_elements: &mut LoopElementAllocator,
    scope: ActionCheckScope,
    condition: &ValueExpr,
    then_body: &[Statement],
    else_body: &[Statement],
) -> Result<CheckedAction> {
    let condition = checked_runtime_bool_condition(
        context,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        condition,
    )?;
    let branch_scope = scope.for_statement_if_branch();
    let then_actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        payload_bindings,
        loop_elements,
        branch_scope,
        then_body,
    )?;
    let else_actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        payload_bindings,
        loop_elements,
        branch_scope,
        else_body,
    )?;
    if then_actions.is_empty() && else_actions.is_empty() {
        return Err(Error::new(format!(
            "process {} statement-level if branches cannot both be empty",
            context.process.name
        )));
    }
    Ok(CheckedAction::IfElse {
        condition,
        then_actions,
        else_actions,
    })
}

#[allow(clippy::too_many_arguments)]
fn checked_for_each_action(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    loop_elements: &mut LoopElementAllocator,
    item: &Identifier,
    collection: &ValueExpr,
    body: &[Statement],
) -> Result<CheckedAction> {
    validate_loop_element_binding(context, source_bindings, item)?;
    let ValueExpr::Identifier(collection_name) = collection else {
        return Err(Error::new(format!(
            "process {} for loop collection must be a runtime list binding",
            context.process.name
        )));
    };
    let Some(collection_binding) = source_bindings
        .iter()
        .find(|binding| binding.name == collection_name)
    else {
        return Err(Error::new(format!(
            "process {} for loop collection {} is not a source value binding",
            context.process.name, collection_name
        )));
    };
    let Some(CollectionType::List {
        element: element_type,
        capacity,
    }) = context
        .semantic_index
        .collection_type(collection_binding.ty)?
    else {
        return Err(Error::new(format!(
            "process {} for loop collection {} must have type List<T,N>",
            context.process.name, collection_name
        )));
    };
    if context
        .semantic_index
        .process_ref_target_type(element_type)?
        .is_some()
    {
        return Err(Error::new(format!(
            "process {} for loop element binding {} cannot have process reference type",
            context.process.name, item
        )));
    }
    if !template_bindings
        .iter()
        .any(|binding| binding.name == collection_name)
    {
        return Err(Error::new(format!(
            "process {} for loop collection {} must be runtime-bound",
            context.process.name, collection_name
        )));
    }
    validate_source_function_value_expr(
        function_scope,
        collection_binding.ty,
        collection,
        source_bindings,
    )?;
    let collection = resolve_source_value_expr(
        function_scope,
        collection_binding.ty,
        collection,
        source_bindings,
        0,
    )?;
    let collection_template = checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        collection_binding.ty,
        &collection,
        template_bindings,
    )?;

    let element_id = loop_elements.next_id()?;
    let element_ty = types.intern(element_type)?;
    let loop_element = CheckedLoopElement::new(element_id, element_ty.clone());
    let element_path = PayloadBindingPath::whole();
    let element_template_binding = ValueTemplateBinding {
        name: item,
        ty: element_type,
        checked_ty: &element_ty,
        root_checked_ty: &element_ty,
        source: ValueTemplateSource::LoopElement(element_id),
        path: &element_path,
    };
    let mut body_source_bindings = source_bindings.to_vec();
    body_source_bindings.push(SourceValueBinding {
        name: item,
        ty: element_type,
    });
    let mut body_template_bindings = template_bindings.to_vec();
    body_template_bindings.push(element_template_binding);
    let body = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        &body_source_bindings,
        &body_template_bindings,
        payload_bindings,
        loop_elements,
        ActionCheckScope::TOP_LEVEL.for_loop_body(),
        body,
    )?;

    Ok(CheckedAction::ForEach {
        element: loop_element,
        collection: collection_template,
        max_items: capacity,
        body,
    })
}

fn validate_loop_element_binding(
    context: &StepCheckContext<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    item: &Identifier,
) -> Result<()> {
    if item.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a reserved state parameter name",
            context.process.name, item
        )));
    }
    if source_bindings.iter().any(|binding| binding.name == item) {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with an existing source value binding",
            context.process.name, item
        )));
    }
    if context.process_ref_index.contains_key(item) {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a process reference binding",
            context.process.name, item
        )));
    }
    if context.semantic_index.process_id(item).is_ok() {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a process declaration",
            context.process.name, item
        )));
    }
    if context
        .semantic_index
        .identifier_conflicts_with_declared_value(item)
    {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a declared type or value constructor",
            context.process.name, item
        )));
    }
    Ok(())
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
                    ValueTemplateSource::LoopElement(element) => {
                        CheckedValueTemplate::LoopElement {
                            ty: binding.checked_ty.clone(),
                            element,
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
