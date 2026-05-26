use super::value_resolution::{
    resolve_binding_source_function_call, resolve_collection_pattern_source_function_call,
    resolve_pattern_source_function_call, resolve_record_pattern_source_function_call,
};
use super::*;
use crate::language::ast::{
    ValueBooleanOperator, ValueEqualityOperator, ValueScalarArithmeticOperator,
    ValueScalarOrderingOperator,
};

mod body_matches;
mod calls;
mod dependencies;
mod equality;
mod local_bindings;
mod resolution;
mod return_matches;
mod scalars;
mod type_check;

use body_matches::validate_source_function_body_match_values;
use calls::{
    enum_value_error, enum_variant_for_expected_type, identifier_starts_uppercase,
    source_function_group_option, validate_source_function_call_or_constructor,
};
use dependencies::{source_value_requires_resolution, source_value_uses_any_binding};
use equality::{source_equality_operand_pair_type, validate_source_equality_expr};
use local_bindings::validate_source_function_block_values;
pub(in crate::language::checker) use resolution::resolve_source_value_expr;
use return_matches::validate_source_function_return_match;
use scalars::{
    source_scalar_expr_type, source_scalar_operand_pair_type,
    validate_source_scalar_arithmetic_expr, validate_source_scalar_literal_expr,
    validate_source_scalar_operand_type, validate_source_scalar_ordering_expr,
};
pub(in crate::language::checker) use type_check::check_source_value_type;

pub(super) fn validate_source_function_body_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match source_function_body(function)? {
        FunctionBody::Block(body) => validate_source_function_block_values(
            scope,
            function,
            &function.return_type,
            body,
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
        ReturnExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => validate_source_function_return_if_else(
            scope,
            function,
            expected_type,
            condition,
            then_branch,
            else_branch,
            bindings,
        ),
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
        ValueExpr::ScalarLiteral(literal) => {
            validate_source_scalar_literal_expr(scope, expected_type, *literal)
        }
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => validate_source_equality_expr(scope, expected_type, *operator, left, right, bindings),
        ValueExpr::ScalarArithmetic {
            operator,
            left,
            right,
        } => validate_source_scalar_arithmetic_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
        ),
        ValueExpr::ScalarOrdering {
            operator,
            left,
            right,
        } => validate_source_scalar_ordering_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
        ),
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
            for (index, field) in record.fields.iter().enumerate() {
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
                if record.fields[..index]
                    .iter()
                    .any(|previous| previous.name == field.name)
                {
                    return Err(Error::new(format!(
                        "record {} field {} is assigned more than once",
                        record.name, field.name
                    )));
                }
                validate_source_function_value_expr(scope, &field_decl.ty, &field.value, bindings)?;
            }
            for field in &record_decl.fields {
                if !record
                    .fields
                    .iter()
                    .any(|provided| provided.name == field.name)
                {
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

fn validate_source_function_return_if_else(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    condition: &ValueExpr,
    then_branch: &FunctionBlock,
    else_branch: &FunctionBlock,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_function_value_expr(scope, &bool_type, condition, bindings)
        .map_err(|err| Error::new(format!("if condition must have type {bool_type}: {err}")))?;
    validate_source_function_return_if_branch(
        scope,
        function,
        expected_type,
        "then",
        then_branch,
        bindings,
    )?;
    validate_source_function_return_if_branch(
        scope,
        function,
        expected_type,
        "else",
        else_branch,
        bindings,
    )
}

fn validate_source_function_return_if_branch(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    branch: &str,
    body: &FunctionBlock,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    if body
        .statements
        .iter()
        .any(|statement| !matches!(statement, Statement::LetValue { .. }))
    {
        return Err(Error::new(format!(
            "source function {} return-if {branch} branch must not perform statements",
            function.name
        )));
    }
    validate_source_function_block_values(scope, function, expected_type, body, bindings).map_err(
        |err| {
            Error::new(format!(
                "if {branch} branch must produce {expected_type}: {err}"
            ))
        },
    )
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
    validate_source_function_value_expr(scope, expected_type, value, bindings).map_err(|err| {
        Error::new(format!(
            "parenthesized value operand must produce {expected_type}: {err}"
        ))
    })
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
    let mut seen = Vec::with_capacity(map.entries.len());
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
        if seen.iter().any(|previous| previous == &key) {
            return Err(Error::new(format!(
                "map value duplicates key {}",
                key.label()
            )));
        }
        seen.push(key);
    }
    Ok(())
}
