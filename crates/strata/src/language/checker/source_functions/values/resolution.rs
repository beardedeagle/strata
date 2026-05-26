use super::*;

#[derive(Clone, Copy)]
struct SourceBooleanBinaryExpr<'a> {
    operator: ValueBooleanOperator,
    left: &'a ValueExpr,
    right: &'a ValueExpr,
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
        ValueExpr::ScalarLiteral(_) => Ok(value.clone()),
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
        ValueExpr::List(list) => {
            resolve_list_source_value_expr(scope, expected_type, list, bindings, depth + 1)
        }
        ValueExpr::Map(map) => {
            resolve_map_source_value_expr(scope, expected_type, map, bindings, depth + 1)
        }
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => resolve_if_else_source_value_expr(
            scope,
            expected_type,
            condition,
            then_branch,
            else_branch,
            bindings,
            depth + 1,
        ),
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => resolve_source_equality_value_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
            depth + 1,
        ),
        ValueExpr::ScalarArithmetic {
            operator,
            left,
            right,
        } => resolve_source_scalar_arithmetic_value_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
            depth + 1,
        ),
        ValueExpr::ScalarOrdering {
            operator,
            left,
            right,
        } => resolve_source_scalar_ordering_value_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
            depth + 1,
        ),
        ValueExpr::BooleanNot { operand } => resolve_source_boolean_not_value_expr(
            scope,
            expected_type,
            operand,
            bindings,
            depth + 1,
        ),
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => resolve_source_boolean_binary_value_expr(
            scope,
            expected_type,
            bindings,
            depth + 1,
            SourceBooleanBinaryExpr {
                operator: *operator,
                left,
                right,
            },
        ),
        ValueExpr::Grouped { value } => {
            resolve_source_grouped_value_expr(scope, expected_type, value, bindings, depth + 1)
        }
    }
}

fn resolve_source_scalar_arithmetic_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueScalarArithmeticOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    validate_source_scalar_arithmetic_expr(scope, expected_type, operator, left, right, bindings)?;
    let left = resolve_source_value_expr(scope, expected_type, left, bindings, depth + 1)?;
    let right = resolve_source_value_expr(scope, expected_type, right, bindings, depth + 1)?;
    reject_static_zero_scalar_divisor(scope, expected_type, operator, &right, bindings)?;
    if !source_value_uses_any_binding(&left, bindings)
        && !source_value_uses_any_binding(&right, bindings)
        && !source_value_requires_resolution(&left)
        && !source_value_requires_resolution(&right)
    {
        let left = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            expected_type,
            &left,
            &[],
        )?;
        let right = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            expected_type,
            &right,
            &[],
        )?;
        let (
            mantle_artifact::ArtifactValue::Scalar(left),
            mantle_artifact::ArtifactValue::Scalar(right),
        ) = (left, right)
        else {
            return Err(Error::new(
                "scalar arithmetic operands must be scalar values",
            ));
        };
        return Ok(ValueExpr::ScalarLiteral(
            mantle_artifact::ArtifactScalarValue::checked_arithmetic(
                operator.artifact_operator(),
                left,
                right,
            )
            .map_err(|err| Error::new(err.to_string()))?,
        ));
    }
    Ok(ValueExpr::ScalarArithmetic {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn reject_static_zero_scalar_divisor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueScalarArithmeticOperator,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let message = match operator {
        ValueScalarArithmeticOperator::Divide => "scalar division by zero",
        ValueScalarArithmeticOperator::Modulo => "scalar modulo by zero",
        ValueScalarArithmeticOperator::Add
        | ValueScalarArithmeticOperator::Subtract
        | ValueScalarArithmeticOperator::Multiply => return Ok(()),
    };
    if concrete_scalar_value_option(scope, expected_type, right, bindings)?
        .is_some_and(|value| value.value() == 0)
    {
        return Err(Error::new(message));
    }
    Ok(())
}

fn concrete_scalar_value_option(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<Option<mantle_artifact::ArtifactScalarValue>> {
    if source_value_uses_any_binding(value, bindings) || source_value_requires_resolution(value) {
        return Ok(None);
    }
    match canonical_source_value_with_bindings(
        scope.module,
        scope.semantic_index,
        expected_type,
        value,
        &[],
    )? {
        mantle_artifact::ArtifactValue::Scalar(value) => Ok(Some(value)),
        _ => Err(Error::new(
            "scalar arithmetic operands must be scalar values",
        )),
    }
}

fn resolve_source_scalar_ordering_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueScalarOrderingOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    validate_source_scalar_ordering_expr(scope, expected_type, operator, left, right, bindings)?;
    let operand_type = source_scalar_operand_pair_type(scope, left, right, bindings)?;
    let left = resolve_source_value_expr(scope, &operand_type, left, bindings, depth + 1)?;
    let right = resolve_source_value_expr(scope, &operand_type, right, bindings, depth + 1)?;
    if !source_value_uses_any_binding(&left, bindings)
        && !source_value_uses_any_binding(&right, bindings)
        && !source_value_requires_resolution(&left)
        && !source_value_requires_resolution(&right)
    {
        let left = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            &operand_type,
            &left,
            &[],
        )?;
        let right = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            &operand_type,
            &right,
            &[],
        )?;
        let (
            mantle_artifact::ArtifactValue::Scalar(left),
            mantle_artifact::ArtifactValue::Scalar(right),
        ) = (left, right)
        else {
            return Err(Error::new("scalar ordering operands must be scalar values"));
        };
        return Ok(ValueExpr::Identifier(bool_identifier(
            mantle_artifact::ArtifactScalarValue::compare(
                operator.artifact_operator(),
                left,
                right,
            )
            .map_err(|err| Error::new(err.to_string()))?,
        )?));
    }
    Ok(ValueExpr::ScalarOrdering {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn resolve_if_else_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    condition: &ValueExpr,
    then_branch: &ValueExpr,
    else_branch: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_function_if_else_with_bool_type(
        scope,
        expected_type,
        &bool_type,
        condition,
        then_branch,
        else_branch,
        bindings,
    )?;
    let condition = resolve_source_value_expr(scope, &bool_type, condition, bindings, depth + 1)?;
    let then_branch =
        resolve_source_value_expr(scope, expected_type, then_branch, bindings, depth + 1)?;
    let else_branch =
        resolve_source_value_expr(scope, expected_type, else_branch, bindings, depth + 1)?;
    if let Some(condition_value) =
        concrete_bool_value_option(scope, &bool_type, &condition, bindings)?
    {
        return Ok(if condition_value {
            then_branch
        } else {
            else_branch
        });
    }
    Ok(ValueExpr::IfElse {
        condition: Box::new(condition),
        then_branch: Box::new(then_branch),
        else_branch: Box::new(else_branch),
    })
}

fn concrete_source_bool_value(
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

fn resolve_source_equality_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueEqualityOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    validate_source_equality_expr(scope, expected_type, operator, left, right, bindings)?;
    let operand_type = source_equality_operand_pair_type(scope, left, right, bindings)?;
    let left = resolve_source_value_expr(scope, &operand_type, left, bindings, depth + 1)?;
    let right = resolve_source_value_expr(scope, &operand_type, right, bindings, depth + 1)?;
    if !source_value_uses_any_binding(&left, bindings)
        && !source_value_uses_any_binding(&right, bindings)
        && !source_value_requires_resolution(&left)
        && !source_value_requires_resolution(&right)
    {
        let left_value = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            &operand_type,
            &left,
            &[],
        )?;
        let right_value = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            &operand_type,
            &right,
            &[],
        )?;
        return Ok(ValueExpr::Identifier(bool_identifier(equality_result(
            operator,
            &left_value,
            &right_value,
        ))?));
    }
    Ok(ValueExpr::Equality {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn resolve_source_boolean_not_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operand: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_boolean_not_expr(scope, expected_type, operand, bindings)?;
    let operand = resolve_source_value_expr(scope, &bool_type, operand, bindings, depth + 1)?;
    if let Some(value) = concrete_bool_value_option(scope, &bool_type, &operand, bindings)? {
        return Ok(ValueExpr::Identifier(bool_identifier(!value)?));
    }
    Ok(ValueExpr::BooleanNot {
        operand: Box::new(operand),
    })
}

fn resolve_source_boolean_binary_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
    binary: SourceBooleanBinaryExpr<'_>,
) -> Result<ValueExpr> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_boolean_binary_expr(
        scope,
        expected_type,
        binary.operator,
        binary.left,
        binary.right,
        bindings,
    )?;
    let left = resolve_source_value_expr(scope, &bool_type, binary.left, bindings, depth + 1)?;
    let right = resolve_source_value_expr(scope, &bool_type, binary.right, bindings, depth + 1)?;
    let left_value = concrete_bool_value_option(scope, &bool_type, &left, bindings)?;
    let right_value = concrete_bool_value_option(scope, &bool_type, &right, bindings)?;
    if let (Some(left_value), Some(right_value)) = (left_value, right_value) {
        return Ok(ValueExpr::Identifier(bool_identifier(boolean_result(
            binary.operator,
            left_value,
            right_value,
        ))?));
    }
    Ok(ValueExpr::BooleanBinary {
        operator: binary.operator,
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn concrete_bool_value_option(
    scope: &SourceFunctionScope<'_>,
    bool_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<Option<bool>> {
    if source_value_uses_any_binding(value, bindings) || source_value_requires_resolution(value) {
        return Ok(None);
    }
    Ok(Some(concrete_source_bool_value(scope, bool_type, value)?))
}

fn boolean_result(operator: ValueBooleanOperator, left: bool, right: bool) -> bool {
    match operator {
        ValueBooleanOperator::And => left && right,
        ValueBooleanOperator::Or => left || right,
    }
}

fn resolve_source_grouped_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    validate_source_grouped_value_expr(scope, expected_type, value, bindings)?;
    resolve_source_value_expr(scope, expected_type, value, bindings, depth + 1)
}

fn equality_result(
    operator: ValueEqualityOperator,
    left: &mantle_artifact::ArtifactValue,
    right: &mantle_artifact::ArtifactValue,
) -> bool {
    match operator {
        ValueEqualityOperator::Equal => left == right,
        ValueEqualityOperator::NotEqual => left != right,
    }
}

fn bool_identifier(value: bool) -> Result<Identifier> {
    Identifier::new(if value { "True" } else { "False" })
}

fn resolve_list_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    list: &ListValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Some(CollectionType::List { element, .. }) =
        scope.semantic_index.collection_type(expected_type)?
    else {
        return Ok(ValueExpr::List(list.clone()));
    };
    let items = list
        .items
        .iter()
        .map(|item| resolve_source_value_expr(scope, element, item, bindings, depth + 1))
        .collect::<Result<Vec<_>>>()?;
    Ok(ValueExpr::List(ListValue {
        element_type: list.element_type.clone(),
        capacity: list.capacity,
        items,
    }))
}

fn resolve_map_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    map: &MapValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Some(CollectionType::Map { key, value, .. }) =
        scope.semantic_index.collection_type(expected_type)?
    else {
        return Ok(ValueExpr::Map(map.clone()));
    };
    let entries = map
        .entries
        .iter()
        .map(|entry| {
            Ok(MapValueEntry {
                key: resolve_source_value_expr(scope, key, &entry.key, bindings, depth + 1)?,
                value: resolve_source_value_expr(scope, value, &entry.value, bindings, depth + 1)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ValueExpr::Map(MapValue {
        key_type: map.key_type.clone(),
        value_type: map.value_type.clone(),
        capacity: map.capacity,
        entries,
    }))
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
        SourceFunctionParamKind::EnumPattern => resolve_pattern_source_function_call(
            scope,
            expected_type,
            functions,
            arg,
            bindings,
            depth + 1,
        ),
        SourceFunctionParamKind::RecordPattern => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate record pattern clauses"
                )));
            }
            resolve_record_pattern_source_function_call(
                scope,
                expected_type,
                first,
                arg,
                bindings,
                depth + 1,
            )
        }
        SourceFunctionParamKind::ListPattern | SourceFunctionParamKind::MapPattern => {
            resolve_collection_pattern_source_function_call(
                scope,
                expected_type,
                functions,
                arg,
                bindings,
                depth + 1,
            )
        }
    }
}
