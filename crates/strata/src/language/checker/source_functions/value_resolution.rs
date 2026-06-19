use super::collection_patterns::{
    collection_pattern_type, resolve_collection_pattern_value_bindings,
    source_nested_pattern_substitutions,
};
use super::record_patterns::{check_record_pattern_bindings, record_pattern_type};
use super::values::{check_source_value_type, resolve_source_value_expr};
use super::*;

mod body_matches;
mod return_matches;
mod substitution;

use body_matches::resolve_source_function_body_match_value;
use return_matches::resolve_source_function_return_match_value;
use substitution::substitute_source_value_bindings;

type RecordPatternValueResolution = (Vec<SourceSubstitution>, Vec<PatternPayloadParam>);

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
        &[SourceSubstitution::new(
            param.name.clone(),
            resolved_arg.clone(),
        )],
        &local_bindings,
        bindings,
        depth + 1,
    )?;
    resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1)
}

pub(super) fn resolve_pattern_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    functions: SourceFunctionGroup<'_, '_>,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new("pattern function group is empty"))?;
    let enum_type = infer_pattern_function_enum_type(
        scope.module,
        scope.semantic_index,
        "source",
        functions.iter(),
        Some(&first.name),
    )?;
    let resolved_arg = resolve_source_value_expr(scope, &enum_type, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &enum_type, &resolved_arg, bindings)?;
    let (variant_name, selected_payload) =
        concrete_source_enum_value(first.name.as_str(), "pattern dispatch", &resolved_arg)?;
    let enum_decl = scope.semantic_index.enum_decl(scope.module, &enum_type)?;
    let selected_variant =
        scope
            .semantic_index
            .enum_variant_index(scope.module, &enum_type, variant_name)?;

    let mut wildcard = None;
    for function in functions.iter() {
        let FunctionParam::Pattern(pattern) = &function.params[0] else {
            return Err(Error::new(format!(
                "function {} cannot mix binding and pattern clauses",
                function.name
            )));
        };
        match pattern {
            Pattern::Constructor {
                name,
                payload: payload_pattern,
            } => {
                let variant =
                    scope
                        .semantic_index
                        .enum_variant_index(scope.module, &enum_type, name)?;
                if variant == selected_variant {
                    let (substitutions, pattern_bindings) =
                        resolve_constructor_payload_pattern_bindings(
                            scope,
                            function,
                            "signature",
                            name,
                            &enum_decl.variants[variant],
                            payload_pattern.as_ref(),
                            selected_payload,
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
            Pattern::List(_) | Pattern::Map(_) => {
                return Err(Error::new(format!(
                    "function {} cannot mix enum and collection pattern clauses",
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
        first.name, variant_name, enum_decl.name
    )))
}

fn resolve_constructor_payload_pattern_bindings(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    pattern_context: &str,
    variant_name: &Identifier,
    variant: &EnumVariant,
    payload_pattern: Option<&ConstructorPayloadPattern>,
    selected_payload: Option<&ValueExpr>,
) -> Result<(Vec<SourceSubstitution>, Vec<PatternPayloadParam>)> {
    let Some(payload_pattern) = payload_pattern else {
        return Ok((Vec::new(), Vec::new()));
    };
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "function {} {pattern_context} pattern {} does not carry a payload",
            function.name, variant_name
        )));
    };
    let Some(payload) = selected_payload else {
        return Err(Error::new(format!(
            "function {} {pattern_context} pattern {} requires a payload value",
            function.name, variant_name
        )));
    };
    match payload_pattern {
        ConstructorPayloadPattern::Binding(binding) => Ok((
            vec![SourceSubstitution::new(
                binding.name.clone(),
                payload.clone(),
            )],
            vec![PatternPayloadParam {
                name: binding.name.clone(),
                ty: binding.ty.clone(),
                path: PayloadBindingPath::whole(),
            }],
        )),
        ConstructorPayloadPattern::Destructure(pattern) => resolve_destructured_payload_pattern(
            scope,
            function,
            pattern_context,
            payload_type,
            pattern,
            payload,
        ),
    }
}

fn resolve_destructured_payload_pattern(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    pattern_context: &str,
    payload_type: &TypeRef,
    pattern: &Pattern,
    payload: &ValueExpr,
) -> Result<(Vec<SourceSubstitution>, Vec<PatternPayloadParam>)> {
    match pattern {
        Pattern::Record { name, fields } => {
            let record = scope
                .semantic_index
                .record_decl(scope.module, payload_type)?;
            if record.name != *name {
                return Err(Error::new(format!(
                    "function {} {pattern_context} record payload pattern {name} cannot match record {}",
                    function.name, record.name
                )));
            }
            let ValueExpr::Record(record_value) = payload else {
                return Err(Error::new(format!(
                    "function {} {pattern_context} record payload pattern {name} requires a concrete record value",
                    function.name
                )));
            };
            let subject = format!("function {}", function.name);
            let (substitutions, bindings) = resolve_record_pattern_value_bindings(
                scope.semantic_index,
                &subject,
                record,
                fields,
                record_value,
            )?;
            Ok((substitutions, bindings))
        }
        Pattern::List(_) | Pattern::Map(_) => {
            resolve_collection_payload_pattern(scope, function, payload_type, pattern, payload)
        }
        Pattern::Wildcard => Ok((Vec::new(), Vec::new())),
        Pattern::Constructor { .. } => {
            let subject = format!("function {}", function.name);
            let mut seen_bindings = BTreeSet::new();
            let nested_context = format!("{pattern_context} payload pattern");
            let mut nested_scope = NestedPatternBindingScope {
                module: scope.module,
                semantic_index: scope.semantic_index,
                binding_context: PatternBindingContext::Source { owner: &subject },
                context: &nested_context,
                seen_bindings: &mut seen_bindings,
            };
            let bindings = check_nested_pattern_bindings(
                &mut nested_scope,
                payload_type,
                pattern,
                &PayloadBindingPath::whole(),
                EmptyConstructorPattern::Allow,
            )?;
            let Some(substitutions) = source_nested_pattern_substitutions(
                scope.module,
                scope.semantic_index,
                payload_type,
                pattern,
                payload,
            )?
            else {
                return Err(Error::new(format!(
                    "function {} {pattern_context} nested payload pattern does not match concrete {}",
                    function.name, payload
                )));
            };
            Ok((substitutions, bindings))
        }
    }
}

fn resolve_collection_payload_pattern(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    payload_type: &TypeRef,
    pattern: &Pattern,
    payload: &ValueExpr,
) -> Result<(Vec<SourceSubstitution>, Vec<PatternPayloadParam>)> {
    let Some(resolution) = resolve_collection_pattern_value_bindings(
        scope.module,
        scope.semantic_index,
        function.name.as_str(),
        "signature payload pattern",
        payload_type,
        pattern,
        payload,
    )?
    else {
        return Err(Error::new(format!(
            "function {} signature collection payload pattern does not match concrete {}",
            function.name, payload
        )));
    };
    Ok((resolution.substitutions, resolution.bindings))
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

pub(super) fn resolve_collection_pattern_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    functions: SourceFunctionGroup<'_, '_>,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new("collection pattern function group is empty"))?;
    let collection_type = collection_pattern_type(first)?;
    let resolved_arg =
        resolve_source_value_expr(scope, &collection_type, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &collection_type, &resolved_arg, bindings)?;

    for function in functions.iter() {
        let FunctionParam::Pattern(pattern) = &function.params[0] else {
            return Err(Error::new(format!(
                "function {} cannot mix binding and collection pattern clauses",
                function.name
            )));
        };
        let next_type = collection_pattern_type(function)?;
        if !scope.semantic_index.same_type(&collection_type, &next_type) {
            return Err(Error::new(format!(
                "function {} collection pattern has type {}, expected {}",
                function.name, next_type, collection_type
            )));
        }
        let Some(resolution) = resolve_collection_pattern_value_bindings(
            scope.module,
            scope.semantic_index,
            function.name.as_str(),
            "pattern dispatch",
            &collection_type,
            pattern,
            &resolved_arg,
        )?
        else {
            continue;
        };
        let local_bindings = resolution
            .bindings
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
            &resolution.substitutions,
            &local_bindings,
            bindings,
            depth + 1,
        )?;
        return resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1);
    }

    Err(Error::new(format!(
        "function {} has no collection pattern for concrete {}",
        first.name, resolved_arg
    )))
}

fn resolve_record_pattern_value_bindings(
    semantic_index: &SemanticIndex,
    subject: &str,
    record_decl: &Record,
    fields: &[RecordPatternField],
    record_value: &RecordValue,
) -> Result<RecordPatternValueResolution> {
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
        substitutions.push(SourceSubstitution::new(
            field.binding.clone(),
            value_field.value.clone(),
        ));
    }
    Ok((substitutions, pattern_bindings))
}

fn resolve_source_function_body_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    substitutions: &[SourceSubstitution],
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
    substitutions: &[SourceSubstitution],
    local_bindings: &[SourceValueBinding<'_>],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let mut block_substitutions = substitutions.to_vec();
    let mut block_bindings = local_bindings.to_vec();
    for statement in &body.statements {
        let Statement::LetValue { name, ty, value } = statement else {
            return Err(Error::new(
                "source function body must not perform runtime statements",
            ));
        };
        let value = substitute_source_value_bindings(value.clone(), &block_substitutions);
        let resolved = resolve_source_value_expr(scope, ty, &value, bindings, depth + 1)?;
        check_source_value_type(scope, ty, &resolved, bindings)?;
        block_substitutions.push(SourceSubstitution::new(name.clone(), resolved));
        block_bindings.push(SourceValueBinding { name, ty });
    }

    let context = SourceFunctionReturnContext {
        scope,
        function,
        substitutions: &block_substitutions,
        local_bindings: &block_bindings,
        bindings,
    };
    resolve_source_function_return_value(&context, &body.returns, depth + 1)
}

struct SourceFunctionReturnContext<'ctx, 'scope, 'local, 'outer> {
    scope: &'ctx SourceFunctionScope<'scope>,
    function: &'ctx Function,
    substitutions: &'ctx [SourceSubstitution],
    local_bindings: &'ctx [SourceValueBinding<'local>],
    bindings: &'ctx [SourceValueBinding<'outer>],
}

fn resolve_source_function_return_value(
    context: &SourceFunctionReturnContext<'_, '_, '_, '_>,
    returns: &ReturnExpr,
    depth: usize,
) -> Result<ValueExpr> {
    match returns {
        ReturnExpr::Value(value) => Ok(substitute_source_value_bindings(
            value.clone(),
            context.substitutions,
        )),
        ReturnExpr::Call { name, arg } => Ok(substitute_source_value_bindings(
            ValueExpr::Call {
                name: name.clone(),
                arg: Box::new(arg.clone()),
            },
            context.substitutions,
        )),
        ReturnExpr::Match(match_body) => resolve_source_function_return_match_value(
            context.scope,
            context.function,
            match_body,
            context.substitutions,
            context.local_bindings,
            context.bindings,
            depth + 1,
        ),
        ReturnExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => resolve_source_function_return_if_else_value(
            context,
            condition,
            then_branch,
            else_branch,
            depth + 1,
        ),
    }
}

fn resolve_source_function_return_if_else_value(
    context: &SourceFunctionReturnContext<'_, '_, '_, '_>,
    condition: &ValueExpr,
    then_branch: &FunctionBlock,
    else_branch: &FunctionBlock,
    depth: usize,
) -> Result<ValueExpr> {
    let bool_type = context.semantic_index().bool_type(context.scope.module)?;
    let condition = substitute_source_value_bindings(condition.clone(), context.substitutions);
    let condition = resolve_source_value_expr(
        context.scope,
        &bool_type,
        &condition,
        context.bindings,
        depth + 1,
    )?;
    let then_value = resolve_source_function_block_return_value(
        context.scope,
        context.function,
        then_branch,
        context.substitutions,
        context.local_bindings,
        context.bindings,
        depth + 1,
    )?;
    let else_value = resolve_source_function_block_return_value(
        context.scope,
        context.function,
        else_branch,
        context.substitutions,
        context.local_bindings,
        context.bindings,
        depth + 1,
    )?;
    Ok(
        if concrete_source_return_bool_value(context.scope, &bool_type, &condition)? {
            then_value
        } else {
            else_value
        },
    )
}

impl SourceFunctionReturnContext<'_, '_, '_, '_> {
    fn semantic_index(&self) -> &SemanticIndex {
        self.scope.semantic_index
    }
}

fn concrete_source_return_bool_value(
    scope: &SourceFunctionScope<'_>,
    bool_type: &TypeRef,
    value: &ValueExpr,
) -> Result<bool> {
    let ValueExpr::Identifier(name) = value else {
        return Err(Error::new("if condition requires a concrete Bool value"));
    };
    let variant = scope
        .semantic_index
        .enum_variant_index(scope.module, bool_type, name)
        .map_err(|_| Error::new("if condition requires a concrete Bool value"))?;
    match variant {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::new("if condition requires a concrete Bool value")),
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
        ValueExpr::Call { .. }
        | ValueExpr::StringLiteral(_)
        | ValueExpr::BytesLiteral(_)
        | ValueExpr::ScalarLiteral(_)
        | ValueExpr::Record(_)
        | ValueExpr::IfElse { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::ScalarArithmetic { .. }
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => Err(Error::new(format!(
            "function {function_name} {usage} requires a concrete enum constructor argument"
        ))),
        ValueExpr::List(_) | ValueExpr::Map(_) => Err(Error::new(format!(
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
        ValueExpr::Identifier(_)
        | ValueExpr::StringLiteral(_)
        | ValueExpr::BytesLiteral(_)
        | ValueExpr::ScalarLiteral(_)
        | ValueExpr::Call { .. }
        | ValueExpr::EnumVariant { .. }
        | ValueExpr::IfElse { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::ScalarArithmetic { .. }
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => Err(Error::new(format!(
            "function {function_name} {usage} requires a concrete record value argument"
        ))),
        ValueExpr::List(_) | ValueExpr::Map(_) => Err(Error::new(format!(
            "function {function_name} {usage} requires a concrete record value argument"
        ))),
    }
}
