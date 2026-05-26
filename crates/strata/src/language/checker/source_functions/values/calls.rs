use super::super::collection_patterns::collection_pattern_type;
use super::super::record_patterns::record_pattern_type;
use super::*;
use crate::language::checker::symbols::ValueEnumVariantInfo;

pub(super) fn validate_source_function_call_or_constructor(
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
            && let Ok(value_enum) = scope.semantic_index.value_enum(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                value_enum.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    validate_source_function_call(scope, expected_type, name, arg, bindings, functions)
}

fn validate_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    functions: SourceFunctionGroup<'_, '_>,
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
        SourceFunctionParamKind::EnumPattern => {
            let enum_type = infer_pattern_function_enum_type(
                scope.module,
                scope.semantic_index,
                "source",
                functions.iter(),
                functions.first().map(|function| &function.name),
            )?;
            validate_source_function_value_expr(scope, &enum_type, arg, bindings)
        }
        SourceFunctionParamKind::RecordPattern => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate record pattern clauses"
                )));
            }
            let record_type = record_pattern_type(first)?;
            validate_source_function_value_expr(scope, &record_type, arg, bindings)
        }
        SourceFunctionParamKind::ListPattern | SourceFunctionParamKind::MapPattern => {
            let collection_type = collection_pattern_type(first)?;
            validate_source_function_value_expr(scope, &collection_type, arg, bindings)
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
    let Some(payload_type) = variant.payload_type.as_ref() else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    validate_source_function_value_expr(scope, payload_type, payload, bindings)
}

pub(super) fn enum_variant_for_expected_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<ValueEnumVariantInfo>> {
    scope
        .semantic_index
        .value_enum_variant_option(scope.module, expected_type, name)
}

pub(super) fn enum_value_error(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Error {
    match scope
        .semantic_index
        .value_enum_name(scope.module, expected_type)
    {
        Ok(enum_name) => Error::new(format!(
            "value {name} is not a variant of enum {}",
            enum_name
        )),
        Err(_) => Error::new(format!(
            "value {name} cannot construct non-enum value of type {expected_type}"
        )),
    }
}

pub(super) fn identifier_starts_uppercase(name: &Identifier) -> bool {
    name.as_str()
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

pub(super) fn source_function_group_option<'a, 'name>(
    scope: &SourceFunctionScope<'a>,
    name: &'name Identifier,
) -> Result<Option<SourceFunctionGroup<'a, 'name>>> {
    let local_len = named_source_function_count(scope.process_functions, name);
    let module_len = named_source_function_count(&scope.module.functions, name);

    match (local_len, module_len) {
        (local_len, 0) if local_len > 0 => Ok(Some(SourceFunctionGroup::new(
            scope.process_functions,
            name,
            local_len,
        ))),
        (0, module_len) if module_len > 0 => Ok(Some(SourceFunctionGroup::new(
            &scope.module.functions,
            name,
            module_len,
        ))),
        (local_len, module_len) if local_len > 0 && module_len > 0 => Err(Error::new(format!(
            "{} function {name} conflicts with module function {name}",
            source_function_scope_label(scope)
        ))),
        _ => Ok(None),
    }
}

fn named_source_function_count(functions: &[Function], name: &Identifier) -> usize {
    functions
        .iter()
        .filter(|function| function.name == *name)
        .count()
}

fn source_function_scope_label(scope: &SourceFunctionScope<'_>) -> String {
    scope
        .process_name
        .map(|name| format!("process {name}"))
        .unwrap_or_else(|| "module".to_string())
}
