use super::value_resolution::{
    resolve_binding_source_function_call, resolve_pattern_source_function_call,
};
use super::*;

pub(super) fn validate_source_function_body_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match source_function_body(function)? {
        FunctionBody::Block(body) => validate_source_function_return_expr(
            scope,
            &function.return_type,
            &body.returns,
            bindings,
        ),
        FunctionBody::Match(match_body) => {
            let FunctionParam::Binding(param) = &function.params[0] else {
                return Err(Error::new(format!(
                    "function {} match body requires a binding parameter",
                    function.name
                )));
            };
            if match_body.scrutinee != param.name {
                return Err(Error::new(format!(
                    "function {} match scrutinee {} must be parameter {}",
                    function.name, match_body.scrutinee, param.name
                )));
            }
            for arm in &match_body.arms {
                let mut arm_bindings = bindings.to_vec();
                if let Pattern::Constructor {
                    binding: Some(payload),
                    ..
                } = &arm.pattern
                {
                    if bindings.iter().any(|binding| binding.name == &payload.name) {
                        return Err(Error::new(format!(
                            "function {} match payload binding {} conflicts with an existing source value binding",
                            function.name, payload.name
                        )));
                    }
                    arm_bindings.push(SourceValueBinding {
                        name: &payload.name,
                        ty: &payload.ty,
                    });
                }
                validate_source_function_return_expr(
                    scope,
                    &function.return_type,
                    &arm.body.returns,
                    &arm_bindings,
                )?;
            }
            Ok(())
        }
    }
}

fn validate_source_function_return_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    returns: &ReturnExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let value = match returns {
        ReturnExpr::Value(value) => value.clone(),
        ReturnExpr::Call { name, arg } => ValueExpr::Call {
            name: name.clone(),
            arg: Box::new(arg.clone()),
        },
    };
    validate_source_function_value_expr(scope, expected_type, &value, bindings)
}

fn validate_source_function_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match value {
        ValueExpr::Identifier(_) | ValueExpr::EnumVariant { .. } => {
            check_source_value_type(scope, expected_type, value, bindings)
        }
        ValueExpr::Call { name, arg } => {
            validate_source_function_call_or_constructor(scope, expected_type, name, arg, bindings)
        }
        ValueExpr::Record(record) => {
            let record_decl = scope
                .semantic_index
                .record_decl(scope.module, expected_type)?;
            if record.name != record_decl.name {
                return Err(Error::new(format!(
                    "expected record value {}, found {}",
                    record_decl.name, record.name
                )));
            }
            let mut seen = BTreeSet::new();
            for field in &record.fields {
                let Some(field_decl) = record_decl
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field.name)
                else {
                    return Err(Error::new(format!(
                        "record {} has no field {}",
                        record.name, field.name
                    )));
                };
                if !seen.insert(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record {} field {} is assigned more than once",
                        record.name, field.name
                    )));
                }
                validate_source_function_value_expr(scope, &field_decl.ty, &field.value, bindings)?;
            }
            for field in &record_decl.fields {
                if !seen.contains(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record {} value is missing field {}",
                        record_decl.name, field.name
                    )));
                }
            }
            Ok(())
        }
    }
}

fn validate_source_function_call_or_constructor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let constructor = enum_variant_for_expected_type(scope, expected_type, name)?;
    let functions = source_function_group_option(scope, name)?;
    if constructor.is_some() && functions.is_some() {
        return Err(Error::new(format!(
            "value expression {name}(...) is ambiguous between an enum constructor and source function"
        )));
    }
    if constructor.is_some() {
        return validate_source_enum_payload_value(scope, expected_type, name, arg, bindings);
    }
    let Some(functions) = functions else {
        if identifier_starts_uppercase(name)
            && let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    validate_source_function_call(scope, expected_type, name, arg, bindings, &functions)
}

fn validate_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    functions: &[&Function],
) -> Result<()> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new(format!("function {name} is not declared")))?;
    if !scope
        .semantic_index
        .same_type(&first.return_type, expected_type)
    {
        return Err(Error::new(format!(
            "function {name} returns {}, expected {}",
            first.return_type, expected_type
        )));
    }
    match source_function_param_kind(first)? {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate binding clauses"
                )));
            }
            let FunctionParam::Binding(param) = &first.params[0] else {
                return Err(Error::new(format!(
                    "function {name} must declare a binding parameter"
                )));
            };
            validate_source_function_value_expr(scope, &param.ty, arg, bindings)
        }
        SourceFunctionParamKind::Pattern => {
            let enum_type = infer_pattern_function_enum_type(
                scope.module,
                scope.semantic_index,
                "source",
                functions,
            )?;
            validate_source_function_value_expr(scope, &enum_type, arg, bindings)
        }
    }
}

fn validate_source_enum_payload_value(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    payload: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let variant = enum_variant_for_expected_type(scope, expected_type, name)?
        .ok_or_else(|| enum_value_error(scope, expected_type, name))?;
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    validate_source_function_value_expr(scope, payload_type, payload, bindings)
}

pub(in crate::language::checker) fn resolve_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }

    match value {
        ValueExpr::Identifier(_) => Ok(value.clone()),
        ValueExpr::Call { name, arg } => {
            resolve_source_call_or_constructor(scope, expected_type, name, arg, bindings, depth + 1)
        }
        ValueExpr::EnumVariant { name, payload } => resolve_source_enum_payload_value(
            scope,
            expected_type,
            name,
            payload,
            bindings,
            depth + 1,
        ),
        ValueExpr::Record(record) => {
            resolve_record_source_value_expr(scope, expected_type, record, bindings, depth + 1)
        }
    }
}

fn resolve_record_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    record: &RecordValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Ok(record_decl) = scope
        .semantic_index
        .record_decl(scope.module, expected_type)
    else {
        return Ok(ValueExpr::Record(record.clone()));
    };
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let Some(field_decl) = record_decl
            .fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            fields.push(field.clone());
            continue;
        };
        fields.push(RecordValueField {
            name: field.name.clone(),
            value: resolve_source_value_expr(
                scope,
                &field_decl.ty,
                &field.value,
                bindings,
                depth + 1,
            )?,
        });
    }
    Ok(ValueExpr::Record(RecordValue {
        name: record.name.clone(),
        fields,
    }))
}

fn enum_variant_for_expected_type<'module>(
    scope: &SourceFunctionScope<'module>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<&'module EnumVariant>> {
    let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type) else {
        return Ok(None);
    };
    Ok(enum_decl
        .variants
        .iter()
        .find(|variant| variant.name == *name))
}

fn enum_value_error(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Error {
    match scope.semantic_index.enum_decl(scope.module, expected_type) {
        Ok(enum_decl) => Error::new(format!(
            "value {name} is not a variant of enum {}",
            enum_decl.name
        )),
        Err(_) => Error::new(format!(
            "value {name} cannot construct non-enum value of type {expected_type}"
        )),
    }
}

fn identifier_starts_uppercase(name: &Identifier) -> bool {
    name.as_str()
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn resolve_source_call_or_constructor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let constructor = enum_variant_for_expected_type(scope, expected_type, name)?;
    let functions = source_function_group_option(scope, name)?;
    if constructor.is_some() && functions.is_some() {
        return Err(Error::new(format!(
            "value expression {name}(...) is ambiguous between an enum constructor and source function"
        )));
    }
    if constructor.is_some() {
        return resolve_source_enum_payload_value(scope, expected_type, name, arg, bindings, depth);
    }
    let Some(functions) = functions else {
        if identifier_starts_uppercase(name)
            && let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    resolve_source_function_call(scope, expected_type, name, arg, bindings, depth, &functions)
}

fn resolve_source_enum_payload_value(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    payload: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let variant = enum_variant_for_expected_type(scope, expected_type, name)?
        .ok_or_else(|| enum_value_error(scope, expected_type, name))?;
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    let payload = resolve_source_value_expr(scope, payload_type, payload, bindings, depth + 1)?;
    if scope
        .semantic_index
        .process_ref_target_type(payload_type)?
        .is_none()
    {
        check_source_value_type(scope, payload_type, &payload, bindings)?;
    }
    Ok(ValueExpr::EnumVariant {
        name: name.clone(),
        payload: Box::new(payload),
    })
}

fn resolve_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
    functions: &[&Function],
) -> Result<ValueExpr> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new(format!("function {name} is not declared")))?;
    if !scope
        .semantic_index
        .same_type(&first.return_type, expected_type)
    {
        return Err(Error::new(format!(
            "function {name} returns {}, expected {}",
            first.return_type, expected_type
        )));
    }

    match source_function_param_kind(first)? {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate binding clauses"
                )));
            }
            resolve_binding_source_function_call(
                scope,
                expected_type,
                first,
                arg,
                bindings,
                depth + 1,
            )
        }
        SourceFunctionParamKind::Pattern => resolve_pattern_source_function_call(
            scope,
            expected_type,
            functions,
            arg,
            bindings,
            depth + 1,
        ),
    }
}

fn source_function_group_option<'a>(
    scope: &SourceFunctionScope<'a>,
    name: &Identifier,
) -> Result<Option<Vec<&'a Function>>> {
    let local: Vec<_> = scope
        .process_functions
        .iter()
        .filter(|function| function.name == *name)
        .collect();
    let module: Vec<_> = scope
        .module
        .functions
        .iter()
        .filter(|function| function.name == *name)
        .collect();

    match (local.is_empty(), module.is_empty()) {
        (false, false) => Err(Error::new(format!(
            "{} function {name} conflicts with module function {name}",
            source_function_scope_label(scope)
        ))),
        (false, true) => Ok(Some(local)),
        (true, false) => Ok(Some(module)),
        (true, true) => Ok(None),
    }
}

fn source_function_scope_label(scope: &SourceFunctionScope<'_>) -> String {
    scope
        .process_name
        .map(|name| format!("process {name}"))
        .unwrap_or_else(|| "module".to_string())
}

pub(in crate::language::checker) fn check_source_value_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let value_bindings = bindings
        .iter()
        .map(|binding| ValueBinding {
            name: binding.name,
            ty: binding.ty,
            label: binding.name.as_str(),
        })
        .collect::<Vec<_>>();
    canonical_source_value_with_bindings(
        scope.module,
        scope.semantic_index,
        expected_type,
        value,
        &value_bindings,
    )?;
    Ok(())
}
