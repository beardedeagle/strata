use super::record_patterns::{check_record_pattern_bindings, record_pattern_type};
use super::values::{check_source_value_type, resolve_source_value_expr};
use super::*;

mod body_matches;
mod return_matches;

use body_matches::resolve_source_function_body_match_value;
use return_matches::resolve_source_function_return_match_value;

type SourceSubstitution<'a> = (&'a Identifier, &'a ValueExpr);
type RecordPatternValueResolution<'a> = (Vec<SourceSubstitution<'a>>, Vec<PatternPayloadParam>);

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
    let local_bindings = [SourceValueBinding {
        name: &param.name,
        ty: &param.ty,
    }];
    let returned = resolve_source_function_body_value(
        scope,
        function,
        &[(&param.name, &resolved_arg)],
        &local_bindings,
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
                    let mut substitutions = Vec::new();
                    let mut local_bindings = Vec::new();
                    if let Some(payload_binding) = payload_binding {
                        let Some(payload) = selected_payload else {
                            return Err(Error::new(format!(
                                "function {} signature pattern {} requires a payload value",
                                function.name, name
                            )));
                        };
                        substitutions.push((&payload_binding.name, payload));
                        local_bindings.push(SourceValueBinding {
                            name: &payload_binding.name,
                            ty: &payload_binding.ty,
                        });
                    }
                    let returned = resolve_source_function_block_return_value(
                        scope,
                        function,
                        source_function_block(function)?,
                        &substitutions,
                        &local_bindings,
                        bindings,
                        depth + 1,
                    )?;
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
            Pattern::Record { .. } => {
                return Err(Error::new(format!(
                    "function {} cannot mix enum and record pattern clauses",
                    function.name
                )));
            }
        }
    }

    if let Some(function) = wildcard {
        let returned = resolve_source_function_block_return_value(
            scope,
            function,
            source_function_block(function)?,
            &[],
            &[],
            bindings,
            depth + 1,
        )?;
        return resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1);
    }

    Err(Error::new(format!(
        "function {} has no pattern for variant {} of enum {}",
        functions[0].name, variant_name, enum_decl.name
    )))
}

pub(super) fn resolve_record_pattern_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    function: &Function,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let record_type = record_pattern_type(function)?;
    let record_decl = scope
        .semantic_index
        .record_decl(scope.module, &record_type)?;
    let resolved_arg = resolve_source_value_expr(scope, &record_type, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &record_type, &resolved_arg, bindings)?;

    let FunctionParam::Pattern(Pattern::Record { fields, .. }) = &function.params[0] else {
        return Err(Error::new(format!(
            "function {} must declare a record pattern parameter",
            function.name
        )));
    };
    let subject = format!("function {}", function.name);
    let ValueExpr::Record(record_value) = &resolved_arg else {
        return Err(Error::new(format!(
            "function {} record pattern {} requires a concrete record value argument",
            function.name, record_decl.name
        )));
    };

    let (substitutions, pattern_bindings) = resolve_record_pattern_value_bindings(
        scope.semantic_index,
        &subject,
        record_decl,
        fields,
        record_value,
    )?;
    let local_bindings = pattern_bindings
        .iter()
        .map(|binding| SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        })
        .collect::<Vec<_>>();

    let returned = resolve_source_function_block_return_value(
        scope,
        function,
        source_function_block(function)?,
        &substitutions,
        &local_bindings,
        bindings,
        depth + 1,
    )?;
    resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1)
}

fn resolve_record_pattern_value_bindings<'a>(
    semantic_index: &SemanticIndex,
    subject: &str,
    record_decl: &Record,
    fields: &'a [RecordPatternField],
    record_value: &'a RecordValue,
) -> Result<RecordPatternValueResolution<'a>> {
    let pattern_bindings =
        check_record_pattern_bindings(semantic_index, subject, record_decl, fields)?;
    let mut substitutions = Vec::with_capacity(fields.len());
    for field in fields {
        let Some(value_field) = record_value
            .fields
            .iter()
            .find(|candidate| candidate.name == field.field)
        else {
            return Err(Error::new(format!(
                "{subject} record pattern {} could not resolve field {}",
                record_decl.name, field.field
            )));
        };
        substitutions.push((&field.binding, &value_field.value));
    }
    Ok((substitutions, pattern_bindings))
}

fn resolve_source_function_body_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    substitutions: &[(&Identifier, &ValueExpr)],
    local_bindings: &[SourceValueBinding<'_>],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let body_scope = source_function_body_scope(scope, function);
    let scope = &body_scope;
    match source_function_body(function)? {
        FunctionBody::Block(body) => resolve_source_function_block_return_value(
            scope,
            function,
            body,
            substitutions,
            local_bindings,
            bindings,
            depth + 1,
        ),
        FunctionBody::Match(match_body) => resolve_source_function_body_match_value(
            scope,
            function,
            match_body,
            substitutions,
            local_bindings,
            bindings,
            depth + 1,
        ),
    }
    .and_then(|value| {
        resolve_source_value_expr(scope, &function.return_type, &value, bindings, depth + 1)
    })
}

fn resolve_source_function_block_return_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    body: &FunctionBlock,
    substitutions: &[(&Identifier, &ValueExpr)],
    local_bindings: &[SourceValueBinding<'_>],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    if !body.statements.is_empty() {
        return Err(Error::new(
            "source function body must not perform statements",
        ));
    }
    resolve_source_function_return_value(
        scope,
        function,
        &body.returns,
        substitutions,
        local_bindings,
        bindings,
        depth + 1,
    )
}

fn resolve_source_function_return_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    returns: &ReturnExpr,
    substitutions: &[(&Identifier, &ValueExpr)],
    local_bindings: &[SourceValueBinding<'_>],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    match returns {
        ReturnExpr::Value(value) => Ok(substitute_source_value_bindings(
            value.clone(),
            substitutions,
        )),
        ReturnExpr::Call { name, arg } => Ok(substitute_source_value_bindings(
            ValueExpr::Call {
                name: name.clone(),
                arg: Box::new(arg.clone()),
            },
            substitutions,
        )),
        ReturnExpr::Match(match_body) => resolve_source_function_return_match_value(
            scope,
            function,
            match_body,
            substitutions,
            local_bindings,
            bindings,
            depth + 1,
        ),
    }
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

fn concrete_source_record_value<'a>(
    function_name: &str,
    usage: &str,
    value: &'a ValueExpr,
) -> Result<&'a RecordValue> {
    match value {
        ValueExpr::Record(record) => Ok(record),
        ValueExpr::Identifier(_) | ValueExpr::Call { .. } | ValueExpr::EnumVariant { .. } => {
            Err(Error::new(format!(
                "function {function_name} {usage} requires a concrete record value argument"
            )))
        }
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
