use crate::language::MAX_VALUE_NESTING;
use crate::language::ast::{ListValue, MapValue, MapValueEntry, RecordValue, RecordValueField};

use super::send::checked_send_payload_template;
use super::*;

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
    in_runtime_if_branch: bool,
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

pub(super) fn validate_return_match_arm_action_statements(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    input: &StepTransitionInput<'_>,
    body: &FunctionBlock,
) -> Result<()> {
    let ReturnExpr::Match(match_body) = &body.returns else {
        return Ok(());
    };
    let scrutinee_binding = source_bindings
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
            template_bindings,
            arm_substitutions: &arm_substitutions,
        };
        let mut arm_bindings = Vec::new();
        let validation_bindings = step_return_match_arm_source_bindings(
            context,
            source_bindings,
            &arm.pattern,
            &mut arm_bindings,
        )?;
        let mut runtime_if_count = 0usize;
        for statement in &source_arm.body.statements {
            if matches!(statement, Statement::IfElse { .. }) {
                runtime_if_count = runtime_if_count.saturating_add(1);
                if runtime_if_count > 1 {
                    return Err(Error::new(format!(
                        "process {} step return match arm cannot perform more than one runtime if in this source slice",
                        context.process.name
                    )));
                }
            }
            validate_step_return_match_arm_action_statement(
                context,
                types,
                function_scope,
                validation_bindings,
                input,
                ArmStatementValidation {
                    template: template_validation,
                    in_runtime_if_branch: false,
                },
                statement,
            )?;
        }
        validate_step_return_match_arm_terminal_template(
            context,
            types,
            function_scope,
            validation_bindings,
            template_bindings,
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
    input: &StepTransitionInput<'_>,
    validation: ArmStatementValidation<'_, '_, '_>,
    statement: &Statement,
) -> Result<()> {
    match statement {
        Statement::Emit(_) => validate_step_return_match_arm_effect(input, Effect::Emit),
        Statement::Send {
            target,
            message,
            payload,
        } => {
            validate_step_return_match_arm_effect(input, Effect::Send)?;
            validate_step_return_match_arm_send(
                context,
                types,
                function_scope,
                source_bindings,
                input,
                validation.template,
                ArmSend {
                    target,
                    message,
                    payload: payload.as_ref(),
                },
            )
        }
        Statement::LetProcessRef { name, .. } => Err(Error::new(format!(
            "process {} step return match arm cannot bind process reference {} in this source slice",
            context.process.name, name
        ))),
        Statement::IfElse {
            condition,
            then_body,
            else_body,
        } => {
            if validation.in_runtime_if_branch {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot perform nested runtime if in this source slice",
                    context.process.name
                )));
            }
            validate_step_return_match_arm_runtime_if_statement(
                context,
                types,
                function_scope,
                source_bindings,
                input,
                validation.template,
                ArmRuntimeIf {
                    condition,
                    then_body,
                    else_body,
                },
            )
        }
        Statement::ForEach { .. } => Err(Error::new(format!(
            "process {} step return match arm cannot perform for loops in this source slice",
            context.process.name
        ))),
    }
}

fn validate_step_return_match_arm_runtime_if_statement(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    input: &StepTransitionInput<'_>,
    template_validation: ArmTemplateValidation<'_, '_, '_>,
    runtime_if: ArmRuntimeIf<'_>,
) -> Result<()> {
    validate_step_return_match_arm_runtime_if_condition(
        context,
        types,
        function_scope,
        source_bindings,
        template_validation,
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
        input,
        template_validation,
        runtime_if.then_body,
    )?;
    validate_step_return_match_arm_runtime_if_branch(
        context,
        types,
        function_scope,
        source_bindings,
        input,
        template_validation,
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
    input: &StepTransitionInput<'_>,
    template_validation: ArmTemplateValidation<'_, '_, '_>,
    statements: &[Statement],
) -> Result<()> {
    for statement in statements {
        validate_step_return_match_arm_action_statement(
            context,
            types,
            function_scope,
            source_bindings,
            input,
            ArmStatementValidation {
                template: template_validation,
                in_runtime_if_branch: true,
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

fn static_step_return_match_arm_substitutions<'a>(
    context: &StepCheckContext<'_>,
    pattern: &'a TypedMatchPattern,
) -> Result<Vec<StaticArmSubstitution<'a>>> {
    let TypedMatchPattern::Variant { bindings, .. } = pattern else {
        return Ok(Vec::new());
    };
    if bindings.is_empty() {
        return Ok(Vec::new());
    }
    bindings
        .iter()
        .map(|binding| {
            Ok(StaticArmSubstitution {
                name: &binding.name,
                value: static_source_value_for_type(
                    context.module,
                    context.semantic_index,
                    &binding.ty,
                    0,
                )?,
            })
        })
        .collect()
}

fn static_source_value_for_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    ty: &TypeRef,
    depth: usize,
) -> Result<ValueExpr> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if semantic_index.process_ref_target_type(ty)?.is_some() {
        return Err(Error::new(
            "process references must be direct message payloads",
        ));
    }
    if let Ok(record) = semantic_index.record_decl(module, ty) {
        if record.fields.is_empty() {
            return Ok(ValueExpr::Identifier(record.name.clone()));
        }
        let mut fields = Vec::with_capacity(record.fields.len());
        for field in &record.fields {
            fields.push(RecordValueField {
                name: field.name.clone(),
                value: static_source_value_for_type(module, semantic_index, &field.ty, depth + 1)?,
            });
        }
        return Ok(ValueExpr::Record(RecordValue {
            name: record.name.clone(),
            fields,
        }));
    }
    if let Some(collection) = semantic_index.collection_type(ty)? {
        return Ok(match collection {
            CollectionType::List { element, capacity } => ValueExpr::List(ListValue {
                element_type: Some(element.clone()),
                capacity: Some(capacity),
                items: Vec::new(),
            }),
            CollectionType::Map {
                key,
                value,
                capacity,
            } => ValueExpr::Map(MapValue {
                key_type: Some(key.clone()),
                value_type: Some(value.clone()),
                capacity: Some(capacity),
                entries: Vec::<MapValueEntry>::new(),
            }),
        });
    }
    let enum_decl = semantic_index.enum_decl(module, ty)?;
    let variant = enum_decl
        .variants
        .iter()
        .find(|variant| variant.payload_type.is_none())
        .or_else(|| enum_decl.variants.first())
        .ok_or_else(|| {
            Error::new(format!(
                "enum {} must declare at least one variant",
                enum_decl.name
            ))
        })?;
    match &variant.payload_type {
        Some(payload_type) => Ok(ValueExpr::EnumVariant {
            name: variant.name.clone(),
            payload: Box::new(static_source_value_for_type(
                module,
                semantic_index,
                payload_type,
                depth + 1,
            )?),
        }),
        None => Ok(ValueExpr::Identifier(variant.name.clone())),
    }
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
