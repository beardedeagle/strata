use super::super::super::super::ast::{
    Identifier, TypeRef, ValueExpr, ValueScalarArithmeticOperator, ValueScalarOrderingOperator,
};
use super::super::super::super::diagnostic::{Error, Result};
use super::super::super::symbols::SemanticIndex;
use super::{CanonicalValueScope, ValueBinding, canonical_value};
use mantle_artifact::ArtifactValue;

pub(super) fn canonical_scalar_arithmetic_value(
    scope: CanonicalValueScope<'_, '_>,
    expected_type: &TypeRef,
    operator: ValueScalarArithmeticOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    depth: usize,
) -> Result<ArtifactValue> {
    if scope.semantic_index.scalar_type(expected_type)?.is_none() {
        return Err(Error::new(format!(
            "scalar expression must be resolved before checking value of type {expected_type}"
        )));
    }
    let left = canonical_value(
        scope.module,
        scope.semantic_index,
        expected_type,
        left,
        scope.bindings,
        scope.context,
        depth + 1,
    )?;
    let right = canonical_value(
        scope.module,
        scope.semantic_index,
        expected_type,
        right,
        scope.bindings,
        scope.context,
        depth + 1,
    )?;
    let (ArtifactValue::Scalar(left), ArtifactValue::Scalar(right)) = (left, right) else {
        return Err(Error::new(
            "scalar arithmetic operands must be scalar values",
        ));
    };
    let value = mantle_artifact::ArtifactScalarValue::checked_arithmetic(
        operator.artifact_operator(),
        left,
        right,
    )
    .map_err(|err| Error::new(err.to_string()))?;
    Ok(ArtifactValue::Scalar(value))
}

pub(super) fn canonical_scalar_ordering_value(
    scope: CanonicalValueScope<'_, '_>,
    operator: ValueScalarOrderingOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    depth: usize,
) -> Result<ArtifactValue> {
    let operand_type =
        canonical_scalar_operand_pair_type(scope.semantic_index, left, right, scope.bindings)?;
    let left = canonical_value(
        scope.module,
        scope.semantic_index,
        &operand_type,
        left,
        scope.bindings,
        scope.context,
        depth + 1,
    )?;
    let right = canonical_value(
        scope.module,
        scope.semantic_index,
        &operand_type,
        right,
        scope.bindings,
        scope.context,
        depth + 1,
    )?;
    let (ArtifactValue::Scalar(left), ArtifactValue::Scalar(right)) = (left, right) else {
        return Err(Error::new("scalar ordering operands must be scalar values"));
    };
    let selected =
        mantle_artifact::ArtifactScalarValue::compare(operator.artifact_operator(), left, right)
            .map_err(|err| Error::new(err.to_string()))?;
    Ok(ArtifactValue::Atom(bool_label(selected).to_string()))
}

fn canonical_scalar_operand_pair_type(
    semantic_index: &SemanticIndex,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueBinding<'_>],
) -> Result<TypeRef> {
    let left_type = canonical_scalar_expr_type(semantic_index, left, bindings, None);
    let right_type = canonical_scalar_expr_type(semantic_index, right, bindings, None);
    match (left_type, right_type) {
        (Ok(left_type), Ok(right_type)) => {
            canonical_matching_scalar_operand_type(semantic_index, left_type, right_type)
        }
        (Ok(left_type), Err(_)) => {
            validate_canonical_scalar_type(semantic_index, &left_type)?;
            let right_type =
                canonical_scalar_expr_type(semantic_index, right, bindings, Some(&left_type))?;
            canonical_matching_scalar_operand_type(semantic_index, left_type, right_type)
        }
        (Err(_), Ok(right_type)) => {
            validate_canonical_scalar_type(semantic_index, &right_type)?;
            let left_type =
                canonical_scalar_expr_type(semantic_index, left, bindings, Some(&right_type))?;
            canonical_matching_scalar_operand_type(semantic_index, left_type, right_type)
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

fn canonical_scalar_expr_type(
    semantic_index: &SemanticIndex,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    expected_type: Option<&TypeRef>,
) -> Result<TypeRef> {
    match value {
        ValueExpr::ScalarLiteral(literal) => {
            Ok(TypeRef::Named(Identifier::new(literal.ty().source_name())?))
        }
        ValueExpr::Identifier(name) => {
            let Some(binding) = bindings.iter().find(|binding| binding.name == name) else {
                return Err(Error::new(format!(
                    "scalar operand {name} must be a scalar value binding"
                )));
            };
            validate_canonical_scalar_type(semantic_index, binding.ty)?;
            Ok(binding.ty.clone())
        }
        ValueExpr::ScalarArithmetic { left, right, .. } => {
            if let Some(expected_type) = expected_type {
                validate_canonical_scalar_type(semantic_index, expected_type)?;
                return Ok(expected_type.clone());
            }
            canonical_scalar_operand_pair_type(semantic_index, left, right, bindings)
        }
        ValueExpr::Grouped { value } => {
            canonical_scalar_expr_type(semantic_index, value, bindings, expected_type)
        }
        ValueExpr::IfElse { .. } => {
            let Some(expected_type) = expected_type else {
                return Err(Error::new(
                    "scalar operand type is ambiguous; use a typed local binding or scalar literal",
                ));
            };
            validate_canonical_scalar_type(semantic_index, expected_type)?;
            Ok(expected_type.clone())
        }
        ValueExpr::Call { .. }
        | ValueExpr::StringLiteral(_)
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

fn canonical_matching_scalar_operand_type(
    semantic_index: &SemanticIndex,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    validate_canonical_scalar_type(semantic_index, &left_type)?;
    validate_canonical_scalar_type(semantic_index, &right_type)?;
    if !semantic_index.same_type(&left_type, &right_type) {
        return Err(Error::new(format!(
            "scalar operands must have the same type; left has {left_type}, right has {right_type}"
        )));
    }
    Ok(left_type)
}

fn validate_canonical_scalar_type(semantic_index: &SemanticIndex, ty: &TypeRef) -> Result<()> {
    if semantic_index.scalar_type(ty)?.is_some() {
        return Ok(());
    }
    Err(Error::new(format!(
        "type {ty} is not a scalar integer type"
    )))
}

fn bool_label(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}
