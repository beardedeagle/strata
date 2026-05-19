use super::value_resolution::{
    resolve_binding_source_function_call, resolve_collection_pattern_source_function_call,
    resolve_pattern_source_function_call, resolve_record_pattern_source_function_call,
};
use super::*;
use crate::language::ast::{ValueBooleanOperator, ValueEqualityOperator};

mod body_matches;
mod calls;
mod resolution;
mod return_matches;
mod type_check;

use body_matches::validate_source_function_body_match_values;
use calls::{
    enum_value_error, enum_variant_for_expected_type, identifier_starts_uppercase,
    source_function_group_option, validate_source_function_call_or_constructor,
};
pub(in crate::language::checker) use resolution::resolve_source_value_expr;
use return_matches::validate_source_function_return_match;
pub(in crate::language::checker) use type_check::check_source_value_type;

pub(super) fn validate_source_function_body_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match source_function_body(function)? {
        FunctionBody::Block(body) => validate_source_function_return_expr(
            scope,
            function,
            &function.return_type,
            &body.returns,
            bindings,
        ),
        FunctionBody::Match(match_body) => validate_source_function_body_match_values(
            scope,
            function,
            &function.return_type,
            match_body,
            bindings,
        ),
    }
}

fn validate_source_function_return_expr(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    returns: &ReturnExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match returns {
        ReturnExpr::Value(value) => {
            validate_source_function_value_expr(scope, expected_type, value, bindings)
        }
        ReturnExpr::Call { name, arg } => validate_source_function_value_expr(
            scope,
            expected_type,
            &ValueExpr::Call {
                name: name.clone(),
                arg: Box::new(arg.clone()),
            },
            bindings,
        ),
        ReturnExpr::Match(match_body) => validate_source_function_return_match(
            scope,
            function,
            expected_type,
            match_body,
            bindings,
        ),
        ReturnExpr::IfElse { .. } => Err(Error::new(format!(
            "source function {} must return a pure value expression",
            function.name
        ))),
    }
}

pub(in crate::language::checker) fn validate_source_function_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match value {
        ValueExpr::Identifier(_) | ValueExpr::EnumVariant { .. } => {
            check_source_value_type(scope, expected_type, value, bindings)
        }
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => validate_source_equality_expr(scope, expected_type, *operator, left, right, bindings),
        ValueExpr::BooleanNot { operand } => {
            validate_source_boolean_not_expr(scope, expected_type, operand, bindings)
        }
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => validate_source_boolean_binary_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
        ),
        ValueExpr::Grouped { value } => {
            validate_source_grouped_value_expr(scope, expected_type, value, bindings)
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
        ValueExpr::List(list) => {
            let Some(CollectionType::List { element, capacity }) =
                scope.semantic_index.collection_type(expected_type)?
            else {
                return check_source_value_type(scope, expected_type, value, bindings);
            };
            validate_list_value_type(scope.semantic_index, expected_type, list, element, capacity)?;
            for item in &list.items {
                validate_source_function_value_expr(scope, element, item, bindings)?;
            }
            Ok(())
        }
        ValueExpr::Map(map) => {
            let Some(CollectionType::Map {
                key,
                value: item,
                capacity,
            }) = scope.semantic_index.collection_type(expected_type)?
            else {
                return check_source_value_type(scope, expected_type, value, bindings);
            };
            validate_map_value_type(
                scope.semantic_index,
                expected_type,
                map,
                key,
                item,
                capacity,
            )?;
            validate_concrete_map_value_keys(scope, key, map, bindings)?;
            for entry in &map.entries {
                validate_source_function_value_expr(scope, key, &entry.key, bindings)?;
                validate_source_function_value_expr(scope, item, &entry.value, bindings)?;
            }
            Ok(())
        }
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => validate_source_function_if_else(
            scope,
            expected_type,
            condition,
            then_branch,
            else_branch,
            bindings,
        ),
    }
}

fn validate_source_function_if_else(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    condition: &ValueExpr,
    then_branch: &ValueExpr,
    else_branch: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_function_if_else_with_bool_type(
        scope,
        expected_type,
        &bool_type,
        condition,
        then_branch,
        else_branch,
        bindings,
    )
}

fn validate_source_function_if_else_with_bool_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    bool_type: &TypeRef,
    condition: &ValueExpr,
    then_branch: &ValueExpr,
    else_branch: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    validate_source_function_value_expr(scope, bool_type, condition, bindings)
        .map_err(|err| Error::new(format!("if condition must have type {bool_type}: {err}")))?;
    validate_source_function_value_expr(scope, expected_type, then_branch, bindings).map_err(
        |err| {
            Error::new(format!(
                "if then branch must produce {expected_type}: {err}"
            ))
        },
    )?;
    validate_source_function_value_expr(scope, expected_type, else_branch, bindings).map_err(
        |err| {
            Error::new(format!(
                "if else branch must produce {expected_type}: {err}"
            ))
        },
    )?;
    Ok(())
}

fn validate_source_equality_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    _operator: ValueEqualityOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    if !scope.semantic_index.same_type(expected_type, &bool_type) {
        return Err(Error::new(format!(
            "equality expression produces {bool_type}, expected {expected_type}"
        )));
    }
    let operand_type = source_equality_operand_pair_type(scope, left, right, bindings)?;
    validate_source_equality_operand_type(scope, &operand_type)?;
    validate_source_function_value_expr(scope, &operand_type, left, bindings).map_err(|err| {
        Error::new(format!(
            "left equality operand must produce {operand_type}: {err}"
        ))
    })?;
    validate_source_function_value_expr(scope, &operand_type, right, bindings).map_err(|err| {
        Error::new(format!(
            "right equality operand must produce {operand_type}: {err}"
        ))
    })
}

fn validate_source_boolean_not_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operand: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_boolean_result_type(scope, expected_type, &bool_type)?;
    validate_source_function_value_expr(scope, &bool_type, operand, bindings)
        .map_err(|err| Error::new(format!("boolean ! operand must produce {bool_type}: {err}")))
}

fn validate_source_boolean_binary_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueBooleanOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_boolean_result_type(scope, expected_type, &bool_type)?;
    validate_source_function_value_expr(scope, &bool_type, left, bindings).map_err(|err| {
        Error::new(format!(
            "left operand of {} must produce {bool_type}: {err}",
            operator.as_str()
        ))
    })?;
    validate_source_function_value_expr(scope, &bool_type, right, bindings).map_err(|err| {
        Error::new(format!(
            "right operand of {} must produce {bool_type}: {err}",
            operator.as_str()
        ))
    })
}

fn validate_source_boolean_result_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    bool_type: &TypeRef,
) -> Result<()> {
    if scope.semantic_index.same_type(expected_type, bool_type) {
        return Ok(());
    }
    Err(Error::new(format!(
        "boolean predicate expression produces {bool_type}, expected {expected_type}"
    )))
}

fn validate_source_grouped_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    if !scope.semantic_index.same_type(expected_type, &bool_type) {
        return Err(Error::new(format!(
            "parenthesized predicate grouping produces {bool_type}, expected {expected_type}"
        )));
    }
    validate_source_function_value_expr(scope, &bool_type, value, bindings).map_err(|err| {
        Error::new(format!(
            "parenthesized predicate operand must produce {bool_type}: {err}"
        ))
    })
}

fn source_equality_operand_pair_type(
    scope: &SourceFunctionScope<'_>,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<TypeRef> {
    let left_type = source_equality_operand_type(scope, left, bindings, None);
    let right_type = source_equality_operand_type(scope, right, bindings, None);
    match (left_type, right_type) {
        (Ok(left_type), Ok(right_type)) => {
            validate_matching_source_equality_operand_types(scope, left_type, right_type)
        }
        (Ok(left_type), Err(_)) => {
            validate_source_equality_operand_type(scope, &left_type)?;
            let right_type =
                source_equality_operand_type(scope, right, bindings, Some(&left_type))?;
            validate_matching_source_equality_operand_types(scope, left_type, right_type)
        }
        (Err(_), Ok(right_type)) => {
            validate_source_equality_operand_type(scope, &right_type)?;
            let left_type = source_equality_operand_type(scope, left, bindings, Some(&right_type))?;
            validate_matching_source_equality_operand_types(scope, left_type, right_type)
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

fn validate_matching_source_equality_operand_types(
    scope: &SourceFunctionScope<'_>,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    validate_source_equality_operand_type(scope, &left_type)?;
    validate_source_equality_operand_type(scope, &right_type)?;
    if !scope.semantic_index.same_type(&left_type, &right_type) {
        return Err(Error::new(format!(
            "equality operands must have the same type; left has {left_type}, right has {right_type}"
        )));
    }
    Ok(left_type)
}

fn source_equality_operand_type(
    scope: &SourceFunctionScope<'_>,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    expected_type: Option<&TypeRef>,
) -> Result<TypeRef> {
    match value {
        ValueExpr::Identifier(name) => {
            if let Some(binding) = bindings.iter().find(|binding| binding.name == name) {
                return Ok(binding.ty.clone());
            }
            if let Some(expected_type) = expected_type
                && source_equality_fieldless_variant_matches_type(scope, expected_type, name)?
            {
                return Ok(expected_type.clone());
            }
            scope
                .semantic_index
                .equality_fieldless_enum_variant_type(scope.module, name)
                .map_err(|err| {
                    Error::new(format!(
                        "equality operand {name} must be a Bool or fieldless enum value: {err}"
                    ))
                })
        }
        ValueExpr::EnumVariant { name, .. } => {
            if let Some(expected_type) = expected_type
                && let Some(variant) = enum_variant_for_expected_type(scope, expected_type, name)?
            {
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "equality operand enum variant {name} carries a payload"
                    )));
                }
                return Ok(expected_type.clone());
            }
            let ty = scope.semantic_index.enum_variant_type(scope.module, name)?;
            let enum_decl = scope.semantic_index.enum_decl(scope.module, &ty)?;
            let variant_index = scope
                .semantic_index
                .enum_variant_index(scope.module, &ty, name)?;
            let variant = enum_decl.variants.get(variant_index).ok_or_else(|| {
                Error::new(format!(
                    "enum {} variant index {variant_index} is not declared",
                    enum_decl.name
                ))
            })?;
            if variant.payload_type.is_some() {
                return Err(Error::new(format!(
                    "equality operand enum variant {name} carries a payload"
                )));
            }
            Ok(ty)
        }
        ValueExpr::Call { .. }
        | ValueExpr::Record(_)
        | ValueExpr::List(_)
        | ValueExpr::Map(_)
        | ValueExpr::IfElse { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => Err(Error::new(
            "equality operands must be Bool or fieldless enum values",
        )),
    }
}

fn source_equality_fieldless_variant_matches_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<bool> {
    let Some(variant) = enum_variant_for_expected_type(scope, expected_type, name)? else {
        return Ok(false);
    };
    if variant.payload_type.is_some() {
        return Err(Error::new(format!(
            "equality operand enum variant {name} carries a payload"
        )));
    }
    Ok(true)
}

fn validate_source_equality_operand_type(
    scope: &SourceFunctionScope<'_>,
    operand_type: &TypeRef,
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    if scope.semantic_index.same_type(operand_type, &bool_type) {
        return Ok(());
    }
    if scope
        .semantic_index
        .process_ref_target_type(operand_type)?
        .is_some()
    {
        return Err(Error::new("process-reference equality is not supported"));
    }
    if scope
        .semantic_index
        .collection_type(operand_type)?
        .is_some()
    {
        return Err(Error::new(
            "list and map equality are not supported in this source slice",
        ));
    }
    if scope
        .semantic_index
        .record_decl(scope.module, operand_type)
        .is_ok()
    {
        return Err(Error::new(
            "record equality is not supported in this source slice",
        ));
    }
    let enum_decl = scope.semantic_index.enum_decl(scope.module, operand_type)?;
    if enum_decl
        .variants
        .iter()
        .any(|variant| variant.payload_type.is_some())
    {
        return Err(Error::new(format!(
            "equality type {operand_type} must not declare payload-bearing enum variants"
        )));
    }
    Ok(())
}

fn validate_list_value_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    list: &ListValue,
    element_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(declared_element) = &list.element_type
        && !semantic_index.same_type(declared_element, element_type)
    {
        return Err(Error::new(format!(
            "list value has element type {declared_element}, expected {element_type} for {expected_type}"
        )));
    }
    if let Some(declared_capacity) = list.capacity
        && declared_capacity != capacity
    {
        return Err(Error::new(format!(
            "list value has capacity {declared_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if list.items.len() > capacity {
        return Err(Error::new(format!(
            "list value length {} exceeds capacity {capacity} for {expected_type}",
            list.items.len()
        )));
    }
    Ok(())
}

fn validate_map_value_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    map: &MapValue,
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(declared_key) = &map.key_type
        && !semantic_index.same_type(declared_key, key_type)
    {
        return Err(Error::new(format!(
            "map value has key type {declared_key}, expected {key_type} for {expected_type}"
        )));
    }
    if let Some(declared_value) = &map.value_type
        && !semantic_index.same_type(declared_value, value_type)
    {
        return Err(Error::new(format!(
            "map value has value type {declared_value}, expected {value_type} for {expected_type}"
        )));
    }
    if let Some(declared_capacity) = map.capacity
        && declared_capacity != capacity
    {
        return Err(Error::new(format!(
            "map value has capacity {declared_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if map.entries.len() > capacity {
        return Err(Error::new(format!(
            "map value entry count {} exceeds capacity {capacity} for {expected_type}",
            map.entries.len()
        )));
    }
    Ok(())
}

fn validate_concrete_map_value_keys(
    scope: &SourceFunctionScope<'_>,
    key_type: &TypeRef,
    map: &MapValue,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for entry in &map.entries {
        if source_value_requires_resolution(&entry.key)
            || source_value_uses_any_binding(&entry.key, bindings)
        {
            continue;
        }
        let key = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            key_type,
            &entry.key,
            &[],
        )?;
        if !seen.insert(key.clone()) {
            return Err(Error::new(format!(
                "map value duplicates key {}",
                key.label()
            )));
        }
    }
    Ok(())
}

fn source_value_uses_any_binding(value: &ValueExpr, bindings: &[SourceValueBinding<'_>]) -> bool {
    bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
}

fn source_value_requires_resolution(value: &ValueExpr) -> bool {
    match value {
        ValueExpr::Identifier(_) => false,
        ValueExpr::Call { .. } => true,
        ValueExpr::EnumVariant { payload, .. } => source_value_requires_resolution(payload),
        ValueExpr::Record(record) => record
            .fields
            .iter()
            .any(|field| source_value_requires_resolution(&field.value)),
        ValueExpr::List(list) => list.items.iter().any(source_value_requires_resolution),
        ValueExpr::Map(map) => map.entries.iter().any(|entry| {
            source_value_requires_resolution(&entry.key)
                || source_value_requires_resolution(&entry.value)
        }),
        ValueExpr::IfElse { .. } => true,
        ValueExpr::Equality { .. } => true,
        ValueExpr::BooleanNot { .. } | ValueExpr::BooleanBinary { .. } => true,
        ValueExpr::Grouped { .. } => true,
    }
}
