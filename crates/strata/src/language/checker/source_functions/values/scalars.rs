use mantle_artifact::ArtifactScalarValue;

use super::*;

pub(super) fn validate_source_scalar_literal_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    literal: ArtifactScalarValue,
) -> Result<()> {
    let Some(expected_scalar) = scope.semantic_index.scalar_type(expected_type)? else {
        return Err(Error::new(format!(
            "scalar literal {} produces {}, expected {}",
            literal.label(),
            literal.ty().source_name(),
            expected_type
        )));
    };
    if literal.ty() != expected_scalar {
        return Err(Error::new(format!(
            "scalar literal {} has type {}, expected {}",
            literal.label(),
            literal.ty().source_name(),
            expected_type
        )));
    }
    Ok(())
}

pub(super) fn validate_source_scalar_arithmetic_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueScalarArithmeticOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    validate_source_scalar_operand_type(scope, expected_type)?;
    validate_source_function_value_expr(scope, expected_type, left, bindings).map_err(|err| {
        Error::new(format!(
            "left operand of {} must produce {expected_type}: {err}",
            operator.as_str()
        ))
    })?;
    validate_source_function_value_expr(scope, expected_type, right, bindings).map_err(|err| {
        Error::new(format!(
            "right operand of {} must produce {expected_type}: {err}",
            operator.as_str()
        ))
    })
}

pub(super) fn validate_source_scalar_ordering_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    operator: ValueScalarOrderingOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    validate_source_boolean_result_type(scope, expected_type, &bool_type)?;
    let operand_type = source_scalar_operand_pair_type(scope, left, right, bindings)?;
    validate_source_scalar_operand_type(scope, &operand_type)?;
    validate_source_function_value_expr(scope, &operand_type, left, bindings).map_err(|err| {
        Error::new(format!(
            "left operand of {} must produce {operand_type}: {err}",
            operator.as_str()
        ))
    })?;
    validate_source_function_value_expr(scope, &operand_type, right, bindings).map_err(|err| {
        Error::new(format!(
            "right operand of {} must produce {operand_type}: {err}",
            operator.as_str()
        ))
    })
}

pub(super) fn validate_source_scalar_operand_type(
    scope: &SourceFunctionScope<'_>,
    ty: &TypeRef,
) -> Result<()> {
    if scope.semantic_index.scalar_type(ty)?.is_some() {
        return Ok(());
    }
    Err(Error::new(format!(
        "type {ty} is not a scalar integer type"
    )))
}

pub(super) fn source_scalar_operand_pair_type(
    scope: &SourceFunctionScope<'_>,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<TypeRef> {
    let left_type = source_scalar_expr_type(scope, left, bindings, None);
    let right_type = source_scalar_expr_type(scope, right, bindings, None);
    match (left_type, right_type) {
        (Ok(left_type), Ok(right_type)) => {
            validate_matching_source_scalar_operand_types(scope, left_type, right_type)
        }
        (Ok(left_type), Err(_)) => {
            validate_source_scalar_operand_type(scope, &left_type)?;
            let right_type = source_scalar_expr_type(scope, right, bindings, Some(&left_type))?;
            validate_matching_source_scalar_operand_types(scope, left_type, right_type)
        }
        (Err(_), Ok(right_type)) => {
            validate_source_scalar_operand_type(scope, &right_type)?;
            let left_type = source_scalar_expr_type(scope, left, bindings, Some(&right_type))?;
            validate_matching_source_scalar_operand_types(scope, left_type, right_type)
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

pub(super) fn source_scalar_expr_type(
    scope: &SourceFunctionScope<'_>,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    expected_type: Option<&TypeRef>,
) -> Result<TypeRef> {
    match value {
        ValueExpr::ScalarLiteral(literal) => scalar_type_ref(literal.ty()),
        ValueExpr::Identifier(name) => {
            let Some(binding) = bindings.iter().find(|binding| binding.name == name) else {
                return Err(Error::new(format!(
                    "scalar operand {name} must be a scalar value binding"
                )));
            };
            validate_source_scalar_operand_type(scope, binding.ty)?;
            Ok(binding.ty.clone())
        }
        ValueExpr::Call { name, .. } => {
            if let Some(expected_type) = expected_type {
                validate_source_scalar_operand_type(scope, expected_type)?;
                return Ok(expected_type.clone());
            }
            let Some(function) =
                source_function_group_option(scope, name)?.and_then(|functions| functions.first())
            else {
                return Err(Error::new(format!("function {name} is not declared")));
            };
            validate_source_scalar_operand_type(scope, &function.return_type)?;
            Ok(function.return_type.clone())
        }
        ValueExpr::IfElse { .. } => {
            let Some(expected_type) = expected_type else {
                return Err(Error::new(
                    "scalar operand type is ambiguous; use a typed local binding or scalar literal",
                ));
            };
            validate_source_scalar_operand_type(scope, expected_type)?;
            Ok(expected_type.clone())
        }
        ValueExpr::ScalarArithmetic { left, right, .. } => {
            if let Some(expected_type) = expected_type {
                validate_source_scalar_operand_type(scope, expected_type)?;
                return Ok(expected_type.clone());
            }
            source_scalar_operand_pair_type(scope, left, right, bindings)
        }
        ValueExpr::Grouped { value } => {
            source_scalar_expr_type(scope, value, bindings, expected_type)
        }
        ValueExpr::StringLiteral(_)
        | ValueExpr::BytesLiteral(_)
        | ValueExpr::Record(_)
        | ValueExpr::List(_)
        | ValueExpr::Map(_)
        | ValueExpr::EnumVariant { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. } => {
            Err(Error::new("scalar operands must be scalar integer values"))
        }
    }
}

fn validate_matching_source_scalar_operand_types(
    scope: &SourceFunctionScope<'_>,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    validate_source_scalar_operand_type(scope, &left_type)?;
    validate_source_scalar_operand_type(scope, &right_type)?;
    if !scope.semantic_index.same_type(&left_type, &right_type) {
        return Err(Error::new(format!(
            "scalar operands must have the same type; left has {left_type}, right has {right_type}"
        )));
    }
    Ok(left_type)
}

fn scalar_type_ref(ty: mantle_artifact::ArtifactScalarType) -> Result<TypeRef> {
    Ok(TypeRef::Named(Identifier::new(ty.source_name())?))
}
