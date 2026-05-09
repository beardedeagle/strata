use super::values::{check_source_value_type, resolve_source_value_expr};
use super::*;

pub(super) fn resolve_binding_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    function: &Function,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let FunctionParam::Binding(param) = &function.params[0] else {
        return Err(Error::new(format!(
            "function {} must declare a binding parameter",
            function.name
        )));
    };
    let resolved_arg = resolve_source_value_expr(scope, &param.ty, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &param.ty, &resolved_arg, bindings)?;
    let returned = resolve_source_function_body_value(
        scope,
        function,
        &[(&param.name, &resolved_arg)],
        bindings,
        depth + 1,
    )?;
    resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1)
}

pub(super) fn resolve_pattern_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    functions: &[&Function],
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let enum_type =
        infer_pattern_function_enum_type(scope.module, scope.semantic_index, "source", functions)?;
    let resolved_arg = resolve_source_value_expr(scope, &enum_type, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &enum_type, &resolved_arg, bindings)?;
    let (variant_name, selected_payload) = concrete_source_enum_value(
        functions[0].name.as_str(),
        "pattern dispatch",
        &resolved_arg,
    )?;
    let enum_decl = scope.semantic_index.enum_decl(scope.module, &enum_type)?;
    let selected_variant =
        scope
            .semantic_index
            .enum_variant_index(scope.module, &enum_type, variant_name)?;

    let mut wildcard = None;
    for function in functions {
        let FunctionParam::Pattern(pattern) = &function.params[0] else {
            return Err(Error::new(format!(
                "function {} cannot mix binding and pattern clauses",
                function.name
            )));
        };
        match pattern {
            Pattern::Constructor {
                name,
                binding: payload_binding,
            } => {
                let variant =
                    scope
                        .semantic_index
                        .enum_variant_index(scope.module, &enum_type, name)?;
                if variant == selected_variant {
                    let returned =
                        source_function_block_return_value(source_function_block(function)?)?;
                    let mut substitutions = Vec::new();
                    if let Some(payload_binding) = payload_binding {
                        let Some(payload) = selected_payload else {
                            return Err(Error::new(format!(
                                "function {} signature pattern {} requires a payload value",
                                function.name, name
                            )));
                        };
                        substitutions.push((&payload_binding.name, payload));
                    }
                    let returned = substitute_source_value_bindings(returned, &substitutions);
                    return resolve_source_value_expr(
                        scope,
                        expected_type,
                        &returned,
                        bindings,
                        depth + 1,
                    );
                }
            }
            Pattern::Wildcard => {
                wildcard = Some(function);
            }
        }
    }

    if let Some(function) = wildcard {
        let returned = source_function_block_return_value(source_function_block(function)?)?;
        return resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1);
    }

    Err(Error::new(format!(
        "function {} has no pattern for variant {} of enum {}",
        functions[0].name, variant_name, enum_decl.name
    )))
}

fn resolve_source_function_body_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    substitutions: &[(&Identifier, &ValueExpr)],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let body_scope = source_function_body_scope(scope, function);
    let scope = &body_scope;
    match source_function_body(function)? {
        FunctionBody::Block(body) => {
            let value = source_function_block_return_value(body)?;
            Ok(substitute_source_value_bindings(value, substitutions))
        }
        FunctionBody::Match(match_body) => {
            let Some((param_name, arg)) = substitutions.first().copied() else {
                return Err(Error::new(format!(
                    "function {} match body requires a parameter argument",
                    function.name
                )));
            };
            if match_body.scrutinee != *param_name {
                return Err(Error::new(format!(
                    "function {} match scrutinee {} must be parameter {}",
                    function.name, match_body.scrutinee, param_name
                )));
            }
            let (variant_name, selected_payload) =
                concrete_source_enum_value(function.name.as_str(), "match dispatch", arg)?;
            let FunctionParam::Binding(param) = &function.params[0] else {
                return Err(Error::new(format!(
                    "function {} match body requires a binding parameter",
                    function.name
                )));
            };
            let enum_decl = scope.semantic_index.enum_decl(scope.module, &param.ty)?;
            let selected_variant =
                scope
                    .semantic_index
                    .enum_variant_index(scope.module, &param.ty, variant_name)?;
            let subject = format!("function {}", function.name);
            let pattern_context = PatternCheckContext {
                module: scope.module,
                semantic_index: scope.semantic_index,
                enum_decl,
                enum_type: &param.ty,
                subject: &subject,
                label: "match",
                payload_context: PatternPayloadContext::SourceValue,
                binding_context: PatternBindingContext::Source { owner: &subject },
            };
            let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;
            let mut wildcard = None;
            for arm in arms {
                match arm.pattern {
                    TypedMatchPattern::Variant { variant, binding }
                        if variant == selected_variant =>
                    {
                        let value = source_function_block_return_value(arm.body)?;
                        if let Some(binding) = binding {
                            let Some(payload) = selected_payload else {
                                return Err(Error::new(format!(
                                    "function {} match pattern {} requires a payload value",
                                    function.name, enum_decl.variants[variant].name
                                )));
                            };
                            let mut arm_substitutions = substitutions.to_vec();
                            arm_substitutions.push((&binding.name, payload));
                            return Ok(substitute_source_value_bindings(value, &arm_substitutions));
                        }
                        return Ok(substitute_source_value_bindings(value, substitutions));
                    }
                    TypedMatchPattern::Wildcard => {
                        wildcard = Some(arm.body);
                    }
                    _ => {}
                }
            }
            if let Some(body) = wildcard {
                let value = source_function_block_return_value(body)?;
                return Ok(substitute_source_value_bindings(value, substitutions));
            }
            Err(Error::new(format!(
                "function {} match has no arm for variant {} of enum {}",
                function.name, variant_name, enum_decl.name
            )))
        }
    }
    .and_then(|value| {
        resolve_source_value_expr(scope, &function.return_type, &value, bindings, depth + 1)
    })
}

fn source_function_block_return_value(body: &FunctionBlock) -> Result<ValueExpr> {
    if !body.statements.is_empty() {
        return Err(Error::new(
            "source function body must not perform statements",
        ));
    }
    Ok(match &body.returns {
        ReturnExpr::Value(value) => value.clone(),
        ReturnExpr::Call { name, arg } => ValueExpr::Call {
            name: name.clone(),
            arg: Box::new(arg.clone()),
        },
    })
}

fn concrete_source_enum_value<'a>(
    function_name: &str,
    usage: &str,
    value: &'a ValueExpr,
) -> Result<(&'a Identifier, Option<&'a ValueExpr>)> {
    match value {
        ValueExpr::Identifier(name) => Ok((name, None)),
        ValueExpr::EnumVariant { name, payload } => Ok((name, Some(payload.as_ref()))),
        ValueExpr::Call { .. } | ValueExpr::Record(_) => Err(Error::new(format!(
            "function {function_name} {usage} requires a concrete enum constructor argument"
        ))),
    }
}

fn substitute_source_value_bindings(
    value: ValueExpr,
    bindings: &[(&Identifier, &ValueExpr)],
) -> ValueExpr {
    match value {
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find_map(|(binding_name, replacement)| {
                (name == **binding_name).then(|| (*replacement).clone())
            })
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_source_value_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_source_value_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_source_value_bindings(field.value, bindings),
                })
                .collect(),
        }),
    }
}
