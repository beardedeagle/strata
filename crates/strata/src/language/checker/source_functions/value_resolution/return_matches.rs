use super::*;
use crate::language::checker::source_functions::collection_patterns::resolve_collection_pattern_value_bindings;

struct ReturnMatchResolutionContext<'ctx, 'module, 'local, 'outer> {
    scope: &'ctx SourceFunctionScope<'module>,
    function: &'ctx Function,
    match_body: &'ctx Match,
    substitutions: &'ctx [SourceSubstitution],
    local_bindings: &'ctx [SourceValueBinding<'local>],
    bindings: &'ctx [SourceValueBinding<'outer>],
    depth: usize,
}

pub(super) fn resolve_source_function_return_match_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    match_body: &Match,
    substitutions: &[SourceSubstitution],
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
    if scope
        .semantic_index
        .collection_type(scrutinee.ty)?
        .is_some()
    {
        return resolve_source_function_return_collection_match_value(
            &context,
            scrutinee.ty,
            &selected,
        );
    }

    Err(Error::new(format!(
        "function {} return match scrutinee {} must be a declared record, enum, list, or map source value",
        function.name, match_body.scrutinee
    )))
}

fn resolve_source_function_return_enum_match_value(
    context: &ReturnMatchResolutionContext<'_, '_, '_, '_>,
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
            Pattern::Constructor { name, payload } => {
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
                let (pattern_substitutions, pattern_bindings) =
                    resolve_constructor_payload_pattern_bindings(
                        context.scope,
                        context.function,
                        name,
                        &context
                            .scope
                            .semantic_index
                            .enum_decl(context.scope.module, enum_type)?
                            .variants[variant],
                        payload.as_ref(),
                        selected_payload,
                    )?;
                arm_substitutions.extend(pattern_substitutions);
                arm_bindings.extend(pattern_bindings.iter().map(|binding| SourceValueBinding {
                    name: &binding.name,
                    ty: &binding.ty,
                }));
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
            Pattern::List(_) => {
                return Err(Error::new(format!(
                    "function {} return match pattern List[...] destructures a list, but this match expects enum constructors",
                    context.function.name
                )));
            }
            Pattern::Map(_) => {
                return Err(Error::new(format!(
                    "function {} return match pattern Map[...] destructures a map, but this match expects enum constructors",
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
    context: &ReturnMatchResolutionContext<'_, '_, '_, '_>,
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
    for binding in &pattern_bindings {
        if context
            .local_bindings
            .iter()
            .any(|existing| existing.name == &binding.name)
        {
            return Err(Error::new(format!(
                "function {} return match record pattern binding {} conflicts with an existing source value binding",
                context.function.name, binding.name
            )));
        }
    }
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
        Pattern::List(_) => Error::new(format!(
            "function {} return match list pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Map(_) => Error::new(format!(
            "function {} return match map pattern cannot match record {}",
            function.name, record_decl.name
        )),
    }
}

fn resolve_source_function_return_collection_match_value(
    context: &ReturnMatchResolutionContext<'_, '_, '_, '_>,
    collection_type: &TypeRef,
    selected: &ValueExpr,
) -> Result<ValueExpr> {
    let mut wildcard = None;
    for arm in &context.match_body.arms {
        match &arm.pattern {
            Pattern::Wildcard => {
                wildcard = Some(&arm.body);
            }
            Pattern::List(_) | Pattern::Map(_) => {
                let Some(resolution) = resolve_collection_pattern_value_bindings(
                    context.scope.module,
                    context.scope.semantic_index,
                    context.function.name.as_str(),
                    "return match",
                    collection_type,
                    &arm.pattern,
                    selected,
                )?
                else {
                    continue;
                };
                for binding in &resolution.bindings {
                    if context
                        .local_bindings
                        .iter()
                        .any(|existing| existing.name == &binding.name)
                    {
                        return Err(Error::new(format!(
                            "function {} return match collection pattern binding {} conflicts with an existing source value binding",
                            context.function.name, binding.name
                        )));
                    }
                }
                let mut arm_substitutions = context.substitutions.to_vec();
                arm_substitutions.extend(resolution.substitutions);
                let mut arm_bindings = context.local_bindings.to_vec();
                arm_bindings.extend(
                    resolution
                        .bindings
                        .iter()
                        .map(|binding| SourceValueBinding {
                            name: &binding.name,
                            ty: &binding.ty,
                        }),
                );
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
            Pattern::Constructor { name, .. } => {
                return Err(Error::new(format!(
                    "function {} return match pattern {} expects an enum constructor, but scrutinee is {}",
                    context.function.name, name, collection_type
                )));
            }
            Pattern::Record { name, .. } => {
                return Err(Error::new(format!(
                    "function {} return match pattern {} destructures a record, but scrutinee is {}",
                    context.function.name, name, collection_type
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
        "function {} return match has no collection pattern for concrete {}",
        context.function.name, selected
    )))
}
