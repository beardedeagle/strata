use super::*;

struct ReturnMatchResolutionContext<'ctx, 'module, 'value, 'local, 'outer> {
    scope: &'ctx SourceFunctionScope<'module>,
    function: &'ctx Function,
    match_body: &'ctx Match,
    substitutions: &'ctx [SourceSubstitution<'value>],
    local_bindings: &'ctx [SourceValueBinding<'local>],
    bindings: &'ctx [SourceValueBinding<'outer>],
    depth: usize,
}

pub(super) fn resolve_source_function_return_match_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    match_body: &Match,
    substitutions: &[SourceSubstitution<'_>],
    local_bindings: &[SourceValueBinding<'_>],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let scrutinee = local_bindings
        .iter()
        .find(|binding| *binding.name == match_body.scrutinee)
        .ok_or_else(|| {
            Error::new(format!(
                "function {} return match scrutinee {} must be a source value binding",
                function.name, match_body.scrutinee
            ))
        })?;
    let selected = substitute_source_value_bindings(
        ValueExpr::Identifier(match_body.scrutinee.clone()),
        substitutions,
    );
    let context = ReturnMatchResolutionContext {
        scope,
        function,
        match_body,
        substitutions,
        local_bindings,
        bindings,
        depth,
    };
    if let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, scrutinee.ty) {
        return resolve_source_function_return_enum_match_value(
            &context,
            scrutinee.ty,
            enum_decl,
            &selected,
        );
    }
    if let Ok(record_decl) = scope.semantic_index.record_decl(scope.module, scrutinee.ty) {
        return resolve_source_function_return_record_match_value(&context, record_decl, &selected);
    }

    Err(Error::new(format!(
        "function {} return match scrutinee {} must be a declared record or enum source value",
        function.name, match_body.scrutinee
    )))
}

fn resolve_source_function_return_enum_match_value(
    context: &ReturnMatchResolutionContext<'_, '_, '_, '_, '_>,
    enum_type: &TypeRef,
    enum_decl: &Enum,
    selected: &ValueExpr,
) -> Result<ValueExpr> {
    let (variant_name, selected_payload) =
        concrete_source_enum_value(context.function.name.as_str(), "return match", selected)?;
    let selected_variant = context.scope.semantic_index.enum_variant_index(
        context.scope.module,
        enum_type,
        variant_name,
    )?;
    let mut wildcard = None;
    for arm in &context.match_body.arms {
        match &arm.pattern {
            Pattern::Constructor { name, binding } => {
                let variant = context.scope.semantic_index.enum_variant_index(
                    context.scope.module,
                    enum_type,
                    name,
                )?;
                if variant != selected_variant {
                    continue;
                }
                let mut arm_substitutions = context.substitutions.to_vec();
                let mut arm_bindings = context.local_bindings.to_vec();
                if let Some(binding) = binding {
                    let Some(payload) = selected_payload else {
                        return Err(Error::new(format!(
                            "function {} return match pattern {} requires a payload value",
                            context.function.name, name
                        )));
                    };
                    arm_substitutions.push((&binding.name, payload));
                    arm_bindings.push(SourceValueBinding {
                        name: &binding.name,
                        ty: &binding.ty,
                    });
                }
                return resolve_source_function_block_return_value(
                    context.scope,
                    context.function,
                    &arm.body,
                    &arm_substitutions,
                    &arm_bindings,
                    context.bindings,
                    context.depth + 1,
                );
            }
            Pattern::Wildcard => {
                wildcard = Some(&arm.body);
            }
            Pattern::Record { name, .. } => {
                return Err(Error::new(format!(
                    "function {} return match pattern {name} destructures a record, but this match expects enum constructors",
                    context.function.name
                )));
            }
        }
    }
    if let Some(body) = wildcard {
        return resolve_source_function_block_return_value(
            context.scope,
            context.function,
            body,
            context.substitutions,
            context.local_bindings,
            context.bindings,
            context.depth + 1,
        );
    }
    Err(Error::new(format!(
        "function {} return match has no arm for variant {} of enum {}",
        context.function.name, variant_name, enum_decl.name
    )))
}

fn resolve_source_function_return_record_match_value(
    context: &ReturnMatchResolutionContext<'_, '_, '_, '_, '_>,
    record_decl: &Record,
    selected: &ValueExpr,
) -> Result<ValueExpr> {
    let record_value =
        concrete_source_record_value(context.function.name.as_str(), "return match", selected)?;
    if record_value.name != record_decl.name {
        return Err(Error::new(format!(
            "function {} return match expected record {}, found {}",
            context.function.name, record_decl.name, record_value.name
        )));
    }

    let [arm] = context.match_body.arms.as_slice() else {
        return Err(Error::new(format!(
            "function {} return match record pattern {} must declare exactly one arm",
            context.function.name, record_decl.name
        )));
    };
    let Pattern::Record { name, fields } = &arm.pattern else {
        return Err(record_return_match_pattern_error(
            context.function,
            &arm.pattern,
            record_decl,
        ));
    };
    if name != &record_decl.name {
        return Err(Error::new(format!(
            "function {} return match record pattern {} cannot match record {}",
            context.function.name, name, record_decl.name
        )));
    }

    let subject = format!("function {} return match", context.function.name);
    let (record_substitutions, pattern_bindings) = resolve_record_pattern_value_bindings(
        context.scope.semantic_index,
        &subject,
        record_decl,
        fields,
        record_value,
    )?;
    let mut arm_substitutions = context.substitutions.to_vec();
    arm_substitutions.extend(record_substitutions);
    let mut arm_bindings = context.local_bindings.to_vec();
    arm_bindings.extend(pattern_bindings.iter().map(|binding| SourceValueBinding {
        name: &binding.name,
        ty: &binding.ty,
    }));

    resolve_source_function_block_return_value(
        context.scope,
        context.function,
        &arm.body,
        &arm_substitutions,
        &arm_bindings,
        context.bindings,
        context.depth + 1,
    )
}

fn record_return_match_pattern_error(
    function: &Function,
    pattern: &Pattern,
    record_decl: &Record,
) -> Error {
    match pattern {
        Pattern::Constructor { name, .. } => Error::new(format!(
            "function {} return match pattern {} expects an enum constructor, but scrutinee is record {}",
            function.name, name, record_decl.name
        )),
        Pattern::Record { .. } => Error::new(format!(
            "function {} return match record pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Wildcard => Error::new(format!(
            "function {} return match over record {} cannot use a wildcard pattern",
            function.name, record_decl.name
        )),
    }
}
