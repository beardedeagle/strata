use crate::language::ast::{ValueScalarArithmeticOperator, ValueScalarOrderingOperator};
use crate::language::checked::{CheckedScalarArithmeticOperator, CheckedScalarOrderingOperator};

use super::*;

#[derive(Clone, Copy)]
pub(super) struct CheckedScalarArithmeticTemplate<'a> {
    pub(super) operator: ValueScalarArithmeticOperator,
    pub(super) left: &'a ValueExpr,
    pub(super) right: &'a ValueExpr,
}

#[derive(Clone, Copy)]
pub(super) struct CheckedScalarOrderingTemplate<'a> {
    pub(super) operator: ValueScalarOrderingOperator,
    pub(super) left: &'a ValueExpr,
    pub(super) right: &'a ValueExpr,
}

pub(super) fn checked_scalar_arithmetic_template(
    types: &mut CheckedTypeInterner<'_>,
    input: CheckedTemplateInput<'_, '_>,
    arithmetic: CheckedScalarArithmeticTemplate<'_>,
) -> Result<CheckedValueTemplate> {
    validate_scalar_template_operand_type(input.semantic_index, input.expected_type)?;
    let ty = types.intern(input.expected_type)?;
    Ok(CheckedValueTemplate::ScalarArithmetic {
        ty,
        operator: checked_scalar_arithmetic_operator(arithmetic.operator),
        left: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            input.expected_type,
            arithmetic.left,
            input.bindings,
            input.depth + 1,
        )?),
        right: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            input.expected_type,
            arithmetic.right,
            input.bindings,
            input.depth + 1,
        )?),
    })
}

pub(super) fn checked_scalar_ordering_template(
    types: &mut CheckedTypeInterner<'_>,
    input: CheckedTemplateInput<'_, '_>,
    ordering: CheckedScalarOrderingTemplate<'_>,
) -> Result<CheckedValueTemplate> {
    let bool_type = input.semantic_index.bool_type(input.module)?;
    if !input
        .semantic_index
        .same_type(input.expected_type, &bool_type)
    {
        return Err(Error::new(format!(
            "scalar ordering expression produces {bool_type}, expected {}",
            input.expected_type
        )));
    }
    let operand_type = scalar_template_operand_pair_type(
        input.semantic_index,
        ordering.left,
        ordering.right,
        input.bindings,
    )?;
    validate_scalar_template_operand_type(input.semantic_index, &operand_type)?;
    Ok(CheckedValueTemplate::ScalarOrdering {
        ty: types.intern(&bool_type)?,
        operand_ty: types.intern(&operand_type)?,
        operator: checked_scalar_ordering_operator(ordering.operator),
        left: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            &operand_type,
            ordering.left,
            input.bindings,
            input.depth + 1,
        )?),
        right: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            &operand_type,
            ordering.right,
            input.bindings,
            input.depth + 1,
        )?),
    })
}

fn scalar_template_operand_pair_type(
    semantic_index: &SemanticIndex,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<TypeRef> {
    let left_type = scalar_template_operand_type(semantic_index, left, bindings, None);
    let right_type = scalar_template_operand_type(semantic_index, right, bindings, None);
    match (left_type, right_type) {
        (Ok(left_type), Ok(right_type)) => {
            validate_matching_scalar_template_operand_types(semantic_index, left_type, right_type)
        }
        (Ok(left_type), Err(_)) => {
            validate_scalar_template_operand_type(semantic_index, &left_type)?;
            let right_type =
                scalar_template_operand_type(semantic_index, right, bindings, Some(&left_type))?;
            validate_matching_scalar_template_operand_types(semantic_index, left_type, right_type)
        }
        (Err(_), Ok(right_type)) => {
            validate_scalar_template_operand_type(semantic_index, &right_type)?;
            let left_type =
                scalar_template_operand_type(semantic_index, left, bindings, Some(&right_type))?;
            validate_matching_scalar_template_operand_types(semantic_index, left_type, right_type)
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

pub(super) fn scalar_template_expr_type(
    semantic_index: &SemanticIndex,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
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
            validate_scalar_template_operand_type(semantic_index, binding.ty)?;
            Ok(binding.ty.clone())
        }
        ValueExpr::Call { .. } | ValueExpr::IfElse { .. } => {
            let Some(expected_type) = expected_type else {
                return Err(Error::new(
                    "scalar operand type is ambiguous; use a typed local binding or scalar literal",
                ));
            };
            validate_scalar_template_operand_type(semantic_index, expected_type)?;
            Ok(expected_type.clone())
        }
        ValueExpr::ScalarArithmetic { left, right, .. } => {
            if let Some(expected_type) = expected_type {
                validate_scalar_template_operand_type(semantic_index, expected_type)?;
                return Ok(expected_type.clone());
            }
            scalar_template_operand_pair_type(semantic_index, left, right, bindings)
        }
        ValueExpr::Grouped { value } => {
            scalar_template_expr_type(semantic_index, value, bindings, expected_type)
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

fn scalar_template_operand_type(
    semantic_index: &SemanticIndex,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    expected_type: Option<&TypeRef>,
) -> Result<TypeRef> {
    scalar_template_expr_type(semantic_index, value, bindings, expected_type)
}

fn validate_matching_scalar_template_operand_types(
    semantic_index: &SemanticIndex,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    validate_scalar_template_operand_type(semantic_index, &left_type)?;
    validate_scalar_template_operand_type(semantic_index, &right_type)?;
    if !semantic_index.same_type(&left_type, &right_type) {
        return Err(Error::new(format!(
            "scalar operands must have the same type; left has {left_type}, right has {right_type}"
        )));
    }
    Ok(left_type)
}

fn validate_scalar_template_operand_type(
    semantic_index: &SemanticIndex,
    ty: &TypeRef,
) -> Result<()> {
    if semantic_index.scalar_type(ty)?.is_some() {
        return Ok(());
    }
    Err(Error::new(format!(
        "type {ty} is not a scalar integer type"
    )))
}

fn checked_scalar_arithmetic_operator(
    operator: ValueScalarArithmeticOperator,
) -> CheckedScalarArithmeticOperator {
    match operator {
        ValueScalarArithmeticOperator::Add => CheckedScalarArithmeticOperator::Add,
        ValueScalarArithmeticOperator::Subtract => CheckedScalarArithmeticOperator::Subtract,
        ValueScalarArithmeticOperator::Multiply => CheckedScalarArithmeticOperator::Multiply,
        ValueScalarArithmeticOperator::Divide => CheckedScalarArithmeticOperator::Divide,
        ValueScalarArithmeticOperator::Modulo => CheckedScalarArithmeticOperator::Modulo,
    }
}

fn checked_scalar_ordering_operator(
    operator: ValueScalarOrderingOperator,
) -> CheckedScalarOrderingOperator {
    match operator {
        ValueScalarOrderingOperator::Less => CheckedScalarOrderingOperator::Less,
        ValueScalarOrderingOperator::LessEqual => CheckedScalarOrderingOperator::LessEqual,
        ValueScalarOrderingOperator::Greater => CheckedScalarOrderingOperator::Greater,
        ValueScalarOrderingOperator::GreaterEqual => CheckedScalarOrderingOperator::GreaterEqual,
    }
}
