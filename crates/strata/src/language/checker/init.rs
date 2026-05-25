use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitReturnMatchPolicy {
    AllowTopLevel,
    RejectNested,
}

pub(super) fn check_init(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<CheckedStateId> {
    let init = &process.init;
    if !init.params.is_empty() {
        return Err(Error::new("init must declare no parameters"));
    }
    if !semantic_index.same_type(&init.return_type, &process.state_type) {
        return Err(Error::new(format!(
            "init returns {}, expected {}",
            init.return_type, process.state_type
        )));
    }
    if !init.may.is_empty() {
        return Err(Error::new("init may-behaviors must be empty"));
    }
    if init.determinism != Determinism::Det {
        return Err(Error::new("init must be deterministic"));
    }

    validate_effects("init", &init.effects, BTreeSet::new())?;

    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        process_refs: None,
        semantic_index,
    };
    let Some(body) = &init.body else {
        return Err(Error::new("init must have a body for buildable source"));
    };
    match body {
        FunctionBody::Block(body) => {
            let value = resolve_init_return_block_value(
                process,
                &function_scope,
                body,
                &[],
                "init body",
                InitReturnMatchPolicy::AllowTopLevel,
            )?;
            state_space.resolve_state_value(semantic_index, types, &value)
        }
        FunctionBody::Match(match_body) => {
            check_init_match(process, &function_scope, state_space, types, match_body)
        }
    }
}

fn check_init_match(
    process: &Process,
    scope: &SourceFunctionScope<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    match_body: &super::super::ast::Match,
) -> Result<CheckedStateId> {
    let state =
        resolve_init_match_value(process, scope, match_body, "init match", "init match arm")?;
    state_space.resolve_state_value(scope.semantic_index, types, &state)
}

fn resolve_init_match_value(
    process: &Process,
    scope: &SourceFunctionScope<'_>,
    match_body: &super::super::ast::Match,
    label: &'static str,
    arm_context: &'static str,
) -> Result<ValueExpr> {
    let scrutinee_type = scope
        .semantic_index
        .fieldless_enum_variant_type(scope.module, &match_body.scrutinee)?;
    let enum_decl = scope
        .semantic_index
        .enum_decl(scope.module, &scrutinee_type)?;
    let selected_variant = scope.semantic_index.enum_variant_index(
        scope.module,
        &scrutinee_type,
        &match_body.scrutinee,
    )?;
    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module: scope.module,
        semantic_index: scope.semantic_index,
        enum_decl,
        enum_type: &scrutinee_type,
        subject: &subject,
        label,
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;

    let mut selected_state = None;
    let mut wildcard_state = None;
    for arm in arms {
        let payload_bindings = match &arm.pattern {
            TypedMatchPattern::Variant { bindings, .. } => bindings.as_slice(),
            TypedMatchPattern::Wildcard => &[],
        };
        let state = resolve_init_return_block_value(
            process,
            scope,
            arm.body,
            payload_bindings,
            arm_context,
            InitReturnMatchPolicy::RejectNested,
        )?;
        match arm.pattern {
            TypedMatchPattern::Variant { variant, .. } if variant == selected_variant => {
                selected_state = Some(state);
            }
            TypedMatchPattern::Wildcard => {
                wildcard_state = Some(state);
            }
            _ => {}
        }
    }

    let state = selected_state.or(wildcard_state).ok_or_else(|| {
        Error::new(format!(
            "process {} {label} has no arm for scrutinee {}",
            process.name, match_body.scrutinee,
        ))
    })?;
    Ok(state)
}

fn resolve_init_return_block_value(
    process: &Process,
    scope: &SourceFunctionScope<'_>,
    body: &FunctionBlock,
    payload_bindings: &[PatternPayloadParam],
    context: &str,
    return_match_policy: InitReturnMatchPolicy,
) -> Result<ValueExpr> {
    if !body.statements.is_empty() {
        return Err(Error::new(format!("{context} must not perform statements")));
    }
    let value = match &body.returns {
        ReturnExpr::Value(value) => value.clone(),
        ReturnExpr::Call { name, arg } => ValueExpr::Call {
            name: name.clone(),
            arg: Box::new(arg.clone()),
        },
        ReturnExpr::Match(match_body)
            if return_match_policy == InitReturnMatchPolicy::AllowTopLevel =>
        {
            return resolve_init_match_value(
                process,
                scope,
                match_body,
                "init return match",
                "init return match arm",
            );
        }
        ReturnExpr::Match(_) => {
            return Err(Error::new(format!(
                "process {} {context} nested return match is not supported in init",
                process.name
            )));
        }
        ReturnExpr::IfElse { .. } => {
            return Err(Error::new(format!(
                "process {} {context} runtime if is not supported in init",
                process.name
            )));
        }
    };
    let binding_storage;
    let bindings: &[SourceValueBinding<'_>] = if payload_bindings.is_empty() {
        &[]
    } else {
        binding_storage = payload_bindings
            .iter()
            .map(|binding| SourceValueBinding {
                name: &binding.name,
                ty: &binding.ty,
            })
            .collect::<Vec<_>>();
        binding_storage.as_slice()
    };
    let value = resolve_source_value_expr(scope, &process.state_type, &value, bindings, 0)?;
    for binding in payload_bindings {
        if source_value_uses_binding(&value, &binding.name) {
            return Err(Error::new(format!(
                "process {} {context} cannot use payload binding {} in returned state",
                process.name, binding.name
            )));
        }
    }
    check_source_value_type(scope, &process.state_type, &value, bindings)?;
    Ok(value)
}
