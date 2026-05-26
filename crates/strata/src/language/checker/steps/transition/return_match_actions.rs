use crate::language::ast::{ListValue, MapValue, MapValueEntry, RecordValue, RecordValueField};

use super::send::checked_send_payload_template;
use super::*;

mod for_each;
mod static_arm;

use for_each::validate_step_return_match_arm_for_each_statement;
use static_arm::static_step_return_match_arm_substitutions;

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticArmSubstitution<'a> {
    name: &'a Identifier,
    value: ValueExpr,
}

#[derive(Clone, Copy)]
struct ArmTemplateValidation<'a, 'template, 'arm> {
    template_bindings: &'a [ValueTemplateBinding<'template>],
    arm_substitutions: &'a [StaticArmSubstitution<'arm>],
}

#[derive(Clone, Copy)]
struct ArmStatementValidation<'a, 'template, 'arm> {
    template: ArmTemplateValidation<'a, 'template, 'arm>,
    runtime_if_depth: usize,
    in_loop_body: bool,
}

struct ArmStatementValidationState<'a, 'input> {
    input: &'a StepTransitionInput<'input>,
    loop_elements: &'a mut ArmLoopElementAllocator,
}

#[derive(Default)]
struct ArmLoopElementAllocator {
    next: usize,
}

impl ArmLoopElementAllocator {
    fn with_next(next: usize) -> Self {
        Self { next }
    }

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

#[derive(Clone, Copy)]
struct ArmRuntimeIf<'a> {
    condition: &'a ValueExpr,
    then_body: &'a [Statement],
    else_body: &'a [Statement],
}

#[derive(Clone, Copy)]
struct ArmSend<'a> {
    target: &'a Identifier,
    message: &'a Identifier,
    payload: Option<&'a ValueExpr>,
}

#[derive(Clone, Copy)]
struct ArmForEach<'a> {
    item: &'a ForEachItem,
    collection: &'a ValueExpr,
    body: &'a [Statement],
}

pub(super) struct ReturnMatchArmActionInput<'a, 'source, 'template, 'input> {
    pub(super) source_bindings: &'a [SourceValueBinding<'source>],
    pub(super) template_bindings: &'a [ValueTemplateBinding<'template>],
    pub(super) input: &'a StepTransitionInput<'input>,
    pub(super) action_scope: ActionCheckScope,
    pub(super) loop_element_base: usize,
    pub(super) body: &'a FunctionBlock,
}

pub(super) fn validate_return_match_arm_action_statements(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    request: ReturnMatchArmActionInput<'_, '_, '_, '_>,
) -> Result<()> {
    let ReturnExpr::Match(match_body) = &request.body.returns else {
        return Ok(());
    };
    let scrutinee_binding = request
        .source_bindings
        .iter()
        .find(|binding| *binding.name == match_body.scrutinee)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} step return match scrutinee {} must be a concrete enum source value binding",
                context.process.name, match_body.scrutinee
            ))
        })?;
    let enum_decl = context
        .semantic_index
        .enum_decl(context.module, scrutinee_binding.ty)
        .map_err(|_| {
            Error::new(format!(
                "process {} step return match scrutinee {} must be a concrete enum source value binding",
                context.process.name, match_body.scrutinee
            ))
        })?;
    let subject = format!("process {}", context.process.name);
    let pattern_context = PatternCheckContext {
        module: context.module,
        semantic_index: context.semantic_index,
        enum_decl,
        enum_type: scrutinee_binding.ty,
        subject: &subject,
        label: "step return match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_payload_sensitive_typed_match_arms(&pattern_context, &match_body.arms)?;
    if arms.len() != match_body.arms.len() {
        return Err(Error::new(format!(
            "process {} step return match typed arm count does not match source arm count",
            context.process.name
        )));
    }
    for (arm, source_arm) in arms.iter().zip(&match_body.arms) {
        let arm_substitutions = static_step_return_match_arm_substitutions(context, &arm.pattern)?;
        let template_validation = ArmTemplateValidation {
            template_bindings: request.template_bindings,
            arm_substitutions: &arm_substitutions,
        };
        let mut arm_bindings = Vec::new();
        let validation_bindings = step_return_match_arm_source_bindings(
            context,
            request.source_bindings,
            &arm.pattern,
            &mut arm_bindings,
        )?;
        let mut loop_elements = ArmLoopElementAllocator::with_next(request.loop_element_base);
        let mut statement_state = ArmStatementValidationState {
            input: request.input,
            loop_elements: &mut loop_elements,
        };
        for statement in &source_arm.body.statements {
            validate_step_return_match_arm_action_statement(
                context,
                types,
                function_scope,
                validation_bindings,
                &mut statement_state,
                ArmStatementValidation {
                    template: template_validation,
                    runtime_if_depth: request.action_scope.statement_if_depth,
                    in_loop_body: request.action_scope.in_loop_body,
                },
                statement,
            )?;
        }
        validate_step_return_match_arm_terminal_template(
            context,
            types,
            function_scope,
            validation_bindings,
            request.template_bindings,
            &arm_substitutions,
            &source_arm.body.returns,
        )?;
    }
    Ok(())
}

fn step_return_match_arm_source_bindings<'a>(
    context: &StepCheckContext<'_>,
    source_bindings: &'a [SourceValueBinding<'a>],
    pattern: &'a TypedMatchPattern,
    arm_bindings: &'a mut Vec<SourceValueBinding<'a>>,
) -> Result<&'a [SourceValueBinding<'a>]> {
    let TypedMatchPattern::Variant { bindings, .. } = pattern else {
        return Ok(source_bindings);
    };
    for binding in bindings {
        if source_bindings
            .iter()
            .any(|existing| existing.name == &binding.name)
        {
            return Err(Error::new(format!(
                "process {} step return match payload binding {} conflicts with an existing source value binding",
                context.process.name, binding.name
            )));
        }
        if context.process_ref_index.contains_key(&binding.name) {
            return Err(Error::new(format!(
                "process {} step return match payload binding {} conflicts with a process reference binding",
                context.process.name, binding.name
            )));
        }
    }
    if bindings.is_empty() {
        return Ok(source_bindings);
    }
    arm_bindings.reserve_exact(source_bindings.len().saturating_add(bindings.len()));
    arm_bindings.extend_from_slice(source_bindings);
    arm_bindings.extend(bindings.iter().map(|binding| SourceValueBinding {
        name: &binding.name,
        ty: &binding.ty,
    }));
    Ok(arm_bindings.as_slice())
}

fn validate_step_return_match_arm_action_statement(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    state: &mut ArmStatementValidationState<'_, '_>,
    validation: ArmStatementValidation<'_, '_, '_>,
    statement: &Statement,
) -> Result<()> {
    match statement {
        Statement::Emit(_) => validate_step_return_match_arm_effect(state.input, Effect::Emit),
        Statement::Send {
            target,
            message,
            payload,
        } => {
            validate_step_return_match_arm_effect(state.input, Effect::Send)?;
            validate_step_return_match_arm_send(
                context,
                types,
                function_scope,
                source_bindings,
                state.input,
                validation.template,
                ArmSend {
                    target,
                    message,
                    payload: payload.as_ref(),
                },
            )
        }
        Statement::LetProcessRef { name, .. } => Err(Error::new(format!(
            "process {} step return match arm cannot bind process reference {}",
            context.process.name, name
        ))),
        Statement::LetSpawnOutcome { name, .. } | Statement::LetSendOutcome { name, .. } => {
            Err(Error::new(format!(
                "process {} step return match arm cannot bind effect outcome {}",
                context.process.name, name
            )))
        }
        Statement::LetValue { name, .. } => Err(Error::new(format!(
            "process {} step return match arm cannot bind source-local value {}",
            context.process.name, name
        ))),
        Statement::IfElse {
            condition,
            then_body,
            else_body,
        } => {
            if validation.runtime_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
                return Err(Error::new(format!(
                    "process {} statement-level if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH}",
                    context.process.name,
                )));
            }
            validate_step_return_match_arm_runtime_if_statement(
                context,
                types,
                function_scope,
                source_bindings,
                state,
                validation,
                ArmRuntimeIf {
                    condition,
                    then_body,
                    else_body,
                },
            )
        }
        Statement::ForEach {
            item,
            collection,
            body,
        } => {
            if validation.in_loop_body {
                return Err(Error::new(format!(
                    "process {} nested for loops are not supported",
                    context.process.name
                )));
            }
            validate_step_return_match_arm_for_each_statement(
                context,
                types,
                function_scope,
                source_bindings,
                state,
                validation,
                ArmForEach {
                    item,
                    collection,
                    body,
                },
            )
        }
    }
}

fn validate_step_return_match_arm_runtime_if_statement(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    state: &mut ArmStatementValidationState<'_, '_>,
    validation: ArmStatementValidation<'_, '_, '_>,
    runtime_if: ArmRuntimeIf<'_>,
) -> Result<()> {
    validate_step_return_match_arm_runtime_if_condition(
        context,
        types,
        function_scope,
        source_bindings,
        validation.template,
        runtime_if.condition,
    )?;
    if runtime_if.then_body.is_empty() && runtime_if.else_body.is_empty() {
        return Err(Error::new(format!(
            "process {} statement-level if branches cannot both be empty",
            context.process.name
        )));
    }
    validate_step_return_match_arm_runtime_if_branch(
        context,
        types,
        function_scope,
        source_bindings,
        state,
        validation,
        runtime_if.then_body,
    )?;
    validate_step_return_match_arm_runtime_if_branch(
        context,
        types,
        function_scope,
        source_bindings,
        state,
        validation,
        runtime_if.else_body,
    )
}

fn validate_step_return_match_arm_runtime_if_condition(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_validation: ArmTemplateValidation<'_, '_, '_>,
    condition: &ValueExpr,
) -> Result<()> {
    let bool_type = context.semantic_index.bool_type(context.module)?;
    validate_source_function_value_expr(function_scope, &bool_type, condition, source_bindings)
        .map_err(|err| Error::new(format!("if condition must have type {bool_type}: {err}")))?;
    let resolved =
        resolve_source_value_expr(function_scope, &bool_type, condition, source_bindings, 0)?;
    let condition = substitute_static_arm_bindings(resolved, template_validation.arm_substitutions);
    checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        &bool_type,
        &condition,
        template_validation.template_bindings,
    )?;
    Ok(())
}

fn validate_step_return_match_arm_runtime_if_branch(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    state: &mut ArmStatementValidationState<'_, '_>,
    validation: ArmStatementValidation<'_, '_, '_>,
    statements: &[Statement],
) -> Result<()> {
    for statement in statements {
        validate_step_return_match_arm_action_statement(
            context,
            types,
            function_scope,
            source_bindings,
            state,
            ArmStatementValidation {
                template: validation.template,
                runtime_if_depth: validation.runtime_if_depth.saturating_add(1),
                in_loop_body: validation.in_loop_body,
            },
            statement,
        )?;
    }
    Ok(())
}

fn validate_step_return_match_arm_effect(
    input: &StepTransitionInput<'_>,
    effect: Effect,
) -> Result<()> {
    if input.declared_effects.contains(&effect) {
        return Ok(());
    }
    Err(Error::new(format!(
        "step uses effect {effect} but does not declare it"
    )))
}

fn validate_step_return_match_arm_send(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    input: &StepTransitionInput<'_>,
    template_validation: ArmTemplateValidation<'_, '_, '_>,
    send: ArmSend<'_>,
) -> Result<()> {
    let target_process =
        validate_step_return_match_arm_send_target(context, input.payload_bindings, send.target)?;
    let variant = context.semantic_index.message_id_for_process(
        context.module,
        context.process.name.as_str(),
        target_process,
        send.message,
    )?;
    let variant_decl =
        context
            .semantic_index
            .message_variant(context.module, target_process, variant)?;
    match (&variant_decl.payload_type, send.payload) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(Error::new(format!(
            "process {} sends payload to message {}, which does not accept one",
            context.process.name, variant_decl.name
        ))),
        (Some(_), None) => Err(Error::new(format!(
            "process {} sends message {} without required payload",
            context.process.name, variant_decl.name
        ))),
        (Some(payload_type), Some(payload)) => {
            let resolved_payload = resolve_source_value_expr(
                function_scope,
                payload_type,
                payload,
                source_bindings,
                0,
            )?;
            if let Some(expected_target) = context
                .semantic_index
                .process_ref_target_type(payload_type)?
            {
                return validate_step_return_match_arm_process_ref_payload(
                    context,
                    input.payload_bindings,
                    payload_type,
                    expected_target,
                    &resolved_payload,
                );
            }
            validate_source_function_value_expr(
                function_scope,
                payload_type,
                &resolved_payload,
                source_bindings,
            )?;
            let template_payload = substitute_static_arm_bindings(
                resolved_payload,
                template_validation.arm_substitutions,
            );
            if source_value_uses_template_binding(
                &template_payload,
                template_validation.template_bindings,
            ) {
                checked_send_payload_template(
                    context,
                    types,
                    payload_type,
                    &template_payload,
                    template_validation.template_bindings,
                )?;
            }
            Ok(())
        }
    }
}

fn validate_step_return_match_arm_terminal_template(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    arm_substitutions: &[StaticArmSubstitution<'_>],
    returns: &ReturnExpr,
) -> Result<()> {
    let ReturnExpr::Call { arg, .. } = returns else {
        return Ok(());
    };
    let state_arg = resolve_source_value_expr(
        function_scope,
        &context.process.state_type,
        arg,
        source_bindings,
        0,
    )?;
    if matches!(&state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        return Ok(());
    }
    let state_arg = substitute_static_arm_bindings(state_arg, arm_substitutions);
    if source_value_uses_template_binding(&state_arg, template_bindings) {
        checked_value_template_with_binding(
            context.module,
            context.semantic_index,
            types,
            &context.process.state_type,
            &state_arg,
            template_bindings,
        )?;
    }
    Ok(())
}

fn validate_step_return_match_arm_send_target(
    context: &StepCheckContext<'_>,
    payload_bindings: &[StepPayloadBinding],
    target: &Identifier,
) -> Result<CheckedProcessId> {
    if let Some(binding) = context.process_ref_index.get(target) {
        return Ok(binding.target);
    }
    if let Some(binding) = payload_bindings
        .iter()
        .find(|binding| binding.name == *target)
    {
        return context
            .semantic_index
            .process_ref_target_type(&binding.ty)?
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} send target {} is not a process reference payload",
                    context.process.name, target
                ))
            });
    }
    Err(Error::new(format!(
        "process {} sends to undeclared process reference {}",
        context.process.name, target
    )))
}

fn validate_step_return_match_arm_process_ref_payload(
    context: &StepCheckContext<'_>,
    payload_bindings: &[StepPayloadBinding],
    expected_type: &TypeRef,
    expected_target: CheckedProcessId,
    payload: &ValueExpr,
) -> Result<()> {
    let ValueExpr::Identifier(name) = payload else {
        return Err(Error::new(format!(
            "process {} sends process reference payload of type {} using a non-reference value",
            context.process.name, expected_type
        )));
    };
    if let Some(binding) = payload_bindings
        .iter()
        .find(|binding| binding.name == *name)
    {
        if context.semantic_index.same_type(&binding.ty, expected_type) {
            return Ok(());
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
    if process_ref.target != expected_target {
        return Err(Error::new(format!(
            "process {} payload {} targets process id {}, expected {}",
            context.process.name,
            name,
            process_ref.target.as_u32(),
            expected_target.as_u32()
        )));
    }
    Ok(())
}

fn source_value_uses_template_binding(
    value: &ValueExpr,
    template_bindings: &[ValueTemplateBinding<'_>],
) -> bool {
    template_bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
}

fn substitute_static_arm_bindings(
    value: ValueExpr,
    bindings: &[StaticArmSubstitution<'_>],
) -> ValueExpr {
    if bindings.is_empty() {
        return value;
    }
    match value {
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find_map(|binding| (binding.name == &name).then(|| binding.value.clone()))
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::ScalarLiteral(_) => value,
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_static_arm_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_static_arm_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_static_arm_bindings(field.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::List(list) => ValueExpr::List(ListValue {
            element_type: list.element_type,
            capacity: list.capacity,
            items: list
                .items
                .into_iter()
                .map(|item| substitute_static_arm_bindings(item, bindings))
                .collect(),
        }),
        ValueExpr::Map(map) => ValueExpr::Map(MapValue {
            key_type: map.key_type,
            value_type: map.value_type,
            capacity: map.capacity,
            entries: map
                .entries
                .into_iter()
                .map(|entry| MapValueEntry {
                    key: substitute_static_arm_bindings(entry.key, bindings),
                    value: substitute_static_arm_bindings(entry.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => ValueExpr::IfElse {
            condition: Box::new(substitute_static_arm_bindings(*condition, bindings)),
            then_branch: Box::new(substitute_static_arm_bindings(*then_branch, bindings)),
            else_branch: Box::new(substitute_static_arm_bindings(*else_branch, bindings)),
        },
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => ValueExpr::Equality {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::ScalarArithmetic {
            operator,
            left,
            right,
        } => ValueExpr::ScalarArithmetic {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::ScalarOrdering {
            operator,
            left,
            right,
        } => ValueExpr::ScalarOrdering {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::BooleanNot { operand } => ValueExpr::BooleanNot {
            operand: Box::new(substitute_static_arm_bindings(*operand, bindings)),
        },
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => ValueExpr::BooleanBinary {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::Grouped { value } => ValueExpr::Grouped {
            value: Box::new(substitute_static_arm_bindings(*value, bindings)),
        },
    }
}
