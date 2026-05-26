use crate::language::ast::{ValueBooleanOperator, ValueEqualityOperator};
use crate::language::checked::{CheckedValueBooleanOperator, CheckedValueEqualityOperator};
use crate::language::checker::symbols::{BuiltinValueShape, ValueEnumVariant};

use super::scalars::scalar_template_expr_type;
use super::*;

#[derive(Clone, Copy)]
pub(super) struct CheckedTemplateInput<'a, 'binding> {
    pub(super) module: &'a Module,
    pub(super) semantic_index: &'a SemanticIndex,
    pub(super) expected_type: &'a TypeRef,
    pub(super) bindings: &'a [ValueTemplateBinding<'binding>],
    pub(super) depth: usize,
}

#[derive(Clone, Copy)]
pub(super) struct CheckedBinaryTemplate<'a> {
    pub(super) operator: ValueBooleanOperator,
    pub(super) left: &'a ValueExpr,
    pub(super) right: &'a ValueExpr,
}

#[derive(Clone, Copy)]
pub(super) struct CheckedEqualityTemplate<'a> {
    pub(super) operator: ValueEqualityOperator,
    pub(super) left: &'a ValueExpr,
    pub(super) right: &'a ValueExpr,
}

pub(super) fn checked_equality_template(
    types: &mut CheckedTypeInterner<'_>,
    input: CheckedTemplateInput<'_, '_>,
    equality: CheckedEqualityTemplate<'_>,
) -> Result<CheckedValueTemplate> {
    let bool_type = input.semantic_index.bool_type(input.module)?;
    if !input
        .semantic_index
        .same_type(input.expected_type, &bool_type)
    {
        return Err(Error::new(format!(
            "equality expression produces {bool_type}, expected {}",
            input.expected_type
        )));
    }
    let operand_type = equality_template_operand_pair_type(
        input.module,
        input.semantic_index,
        equality.left,
        equality.right,
        input.bindings,
    )?;
    validate_equality_template_operands(
        input.module,
        input.semantic_index,
        &operand_type,
        equality.left,
        equality.right,
    )?;
    let operand_ty = types.intern(&operand_type)?;
    Ok(CheckedValueTemplate::Equality {
        ty: types.intern(&bool_type)?,
        operand_ty,
        operator: checked_equality_operator(equality.operator),
        left: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            &operand_type,
            equality.left,
            input.bindings,
            input.depth + 1,
        )?),
        right: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            &operand_type,
            equality.right,
            input.bindings,
            input.depth + 1,
        )?),
    })
}

fn checked_equality_operator(operator: ValueEqualityOperator) -> CheckedValueEqualityOperator {
    match operator {
        ValueEqualityOperator::Equal => CheckedValueEqualityOperator::Equal,
        ValueEqualityOperator::NotEqual => CheckedValueEqualityOperator::NotEqual,
    }
}

pub(super) fn checked_boolean_not_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    operand: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    validate_boolean_template_result_type(semantic_index, expected_type, &bool_type)?;
    let ty = types.intern(&bool_type)?;
    Ok(CheckedValueTemplate::BooleanNot {
        ty,
        operand: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &bool_type,
            operand,
            bindings,
            depth + 1,
        )?),
    })
}

pub(super) fn checked_boolean_binary_template(
    types: &mut CheckedTypeInterner<'_>,
    input: CheckedTemplateInput<'_, '_>,
    binary: CheckedBinaryTemplate<'_>,
) -> Result<CheckedValueTemplate> {
    let bool_type = input.semantic_index.bool_type(input.module)?;
    validate_boolean_template_result_type(input.semantic_index, input.expected_type, &bool_type)?;
    let ty = types.intern(&bool_type)?;
    Ok(CheckedValueTemplate::BooleanBinary {
        ty,
        operator: checked_boolean_operator(binary.operator),
        left: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            &bool_type,
            binary.left,
            input.bindings,
            input.depth + 1,
        )?),
        right: Box::new(checked_value_template(
            input.module,
            input.semantic_index,
            types,
            &bool_type,
            binary.right,
            input.bindings,
            input.depth + 1,
        )?),
    })
}

fn validate_boolean_template_result_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    bool_type: &TypeRef,
) -> Result<()> {
    if semantic_index.same_type(expected_type, bool_type) {
        return Ok(());
    }
    Err(Error::new(format!(
        "boolean predicate expression produces {bool_type}, expected {expected_type}"
    )))
}

fn checked_boolean_operator(operator: ValueBooleanOperator) -> CheckedValueBooleanOperator {
    match operator {
        ValueBooleanOperator::And => CheckedValueBooleanOperator::And,
        ValueBooleanOperator::Or => CheckedValueBooleanOperator::Or,
    }
}

pub(super) fn checked_grouped_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    checked_value_template(
        module,
        semantic_index,
        types,
        expected_type,
        value,
        bindings,
        depth + 1,
    )
}

fn equality_template_operand_pair_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<TypeRef> {
    let left_type = equality_template_operand_type(module, semantic_index, left, bindings, None);
    let right_type = equality_template_operand_type(module, semantic_index, right, bindings, None);
    match (left_type, right_type) {
        (Ok(left_type), Ok(right_type)) => {
            validate_matching_equality_template_operand_types(semantic_index, left_type, right_type)
        }
        (Ok(left_type), Err(_)) => {
            let right_type = equality_template_operand_type(
                module,
                semantic_index,
                right,
                bindings,
                Some(&left_type),
            )?;
            validate_matching_equality_template_operand_types(semantic_index, left_type, right_type)
        }
        (Err(_), Ok(right_type)) => {
            let left_type = equality_template_operand_type(
                module,
                semantic_index,
                left,
                bindings,
                Some(&right_type),
            )?;
            validate_matching_equality_template_operand_types(semantic_index, left_type, right_type)
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

fn validate_matching_equality_template_operand_types(
    semantic_index: &SemanticIndex,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    if !semantic_index.same_type(&left_type, &right_type) {
        return Err(Error::new(format!(
            "equality operands must have the same type; left has {left_type}, right has {right_type}"
        )));
    }
    Ok(left_type)
}

fn equality_template_operand_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    expected_type: Option<&TypeRef>,
) -> Result<TypeRef> {
    match value {
        ValueExpr::Identifier(name) => {
            if let Some(binding) = bindings.iter().find(|binding| name == binding.name) {
                return Ok(binding.ty.clone());
            }
            if let Some(expected_type) = expected_type
                && equality_template_fieldless_variant_matches_type(
                    module,
                    semantic_index,
                    expected_type,
                    name,
                )?
            {
                return Ok(expected_type.clone());
            }
            semantic_index
                .equality_fieldless_enum_variant_type(module, name)
                .map_err(|err| {
                    Error::new(format!(
                        "equality operand {name} must be a Bool, scalar value, or fieldless enum value: {err}"
                    ))
                })
        }
        ValueExpr::ScalarLiteral(literal) => {
            Ok(TypeRef::Named(Identifier::new(literal.ty().source_name())?))
        }
        ValueExpr::ScalarArithmetic { .. } => {
            scalar_template_expr_type(semantic_index, value, bindings, expected_type)
        }
        ValueExpr::Grouped { value } => {
            equality_template_operand_type(module, semantic_index, value, bindings, expected_type)
        }
        ValueExpr::EnumVariant { name, .. } => {
            if let Some(expected_type) = expected_type
                && enum_variant_for_expected_type(module, semantic_index, expected_type, name)?
                    .is_some()
            {
                return Ok(expected_type.clone());
            }
            let ty = semantic_index.enum_variant_type(module, name)?;
            let enum_decl = semantic_index.enum_decl(module, &ty)?;
            let variant_index = semantic_index.enum_variant_index(module, &ty, name)?;
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
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. } => Err(Error::new(
            "equality operands must be Bool, scalar values, or fieldless enum values",
        )),
    }
}

fn equality_template_fieldless_variant_matches_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<bool> {
    let Some(variant) =
        enum_variant_for_expected_type(module, semantic_index, expected_type, name)?
    else {
        return Ok(false);
    };
    if variant.payload_type.is_some() {
        return Err(Error::new(format!(
            "equality operand enum variant {name} carries a payload"
        )));
    }
    Ok(true)
}

fn enum_variant_for_expected_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<ValueEnumVariant>> {
    let Ok(value_enum) = semantic_index.value_enum(module, expected_type) else {
        return Ok(None);
    };
    Ok(value_enum
        .variants
        .into_iter()
        .find(|variant| variant.name == *name))
}

fn validate_equality_template_operand_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    operand_type: &TypeRef,
) -> Result<()> {
    validate_equality_template_operand_type_at_depth(module, semantic_index, operand_type, 0)
}

fn validate_equality_template_operand_type_at_depth(
    module: &Module,
    semantic_index: &SemanticIndex,
    operand_type: &TypeRef,
    depth: usize,
) -> Result<()> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "equality operand type nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if semantic_index.is_unit_type(operand_type)? {
        return Ok(());
    }
    let bool_type = semantic_index.bool_type(module)?;
    if semantic_index.same_type(operand_type, &bool_type) {
        return Ok(());
    }
    if semantic_index.scalar_type(operand_type)?.is_some() {
        return Ok(());
    }
    if semantic_index
        .process_ref_target_type(operand_type)?
        .is_some()
    {
        return Err(Error::new("process-reference equality is not supported"));
    }
    if semantic_index.collection_type(operand_type)?.is_some() {
        return Err(Error::new("list and map equality are not supported"));
    }
    if semantic_index.record_decl(module, operand_type).is_ok() {
        return Err(Error::new("record equality is not supported"));
    }
    if let Some(BuiltinValueShape::Enum(value_enum)) =
        semantic_index.builtin_value_shape(operand_type)?
    {
        for variant in value_enum.variants {
            if let Some(payload_type) = variant.payload_type {
                validate_equality_template_operand_type_at_depth(
                    module,
                    semantic_index,
                    &payload_type,
                    depth + 1,
                )
                .map_err(|err| {
                    Error::new(format!(
                        "equality payload type {payload_type} is not supported: {err}"
                    ))
                })?;
            }
        }
        return Ok(());
    }
    let value_enum = semantic_index.value_enum(module, operand_type)?;
    if value_enum
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

fn validate_equality_template_operands(
    module: &Module,
    semantic_index: &SemanticIndex,
    operand_type: &TypeRef,
    left: &ValueExpr,
    right: &ValueExpr,
) -> Result<()> {
    let full_error =
        match validate_equality_template_operand_type(module, semantic_index, operand_type) {
            Ok(()) => return Ok(()),
            Err(err) => err,
        };
    if template_builtin_variant_equality_pattern(module, semantic_index, operand_type, left)?
        || template_builtin_variant_equality_pattern(module, semantic_index, operand_type, right)?
    {
        Ok(())
    } else {
        Err(full_error)
    }
}

fn template_builtin_variant_equality_pattern(
    module: &Module,
    semantic_index: &SemanticIndex,
    operand_type: &TypeRef,
    value: &ValueExpr,
) -> Result<bool> {
    let value = match value {
        ValueExpr::Grouped { value } => value.as_ref(),
        _ => value,
    };
    let Some(BuiltinValueShape::Enum(value_enum)) =
        semantic_index.builtin_value_shape(operand_type)?
    else {
        return Ok(false);
    };
    match value {
        ValueExpr::Identifier(name) => {
            let Some(variant) = value_enum
                .variants
                .iter()
                .find(|variant| variant.name == *name)
            else {
                return Ok(false);
            };
            Ok(variant.payload_type.is_none())
        }
        ValueExpr::EnumVariant { name, payload } => {
            let Some(variant) = value_enum
                .variants
                .iter()
                .find(|variant| variant.name == *name)
            else {
                return Ok(false);
            };
            let Some(payload_type) = &variant.payload_type else {
                return Ok(false);
            };
            template_equality_payload_pattern_is_safe(module, semantic_index, payload_type, payload)
        }
        _ => Ok(false),
    }
}

fn template_equality_payload_pattern_is_safe(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload_type: &TypeRef,
    payload: &ValueExpr,
) -> Result<bool> {
    if validate_equality_template_operand_type(module, semantic_index, payload_type).is_ok() {
        return Ok(true);
    }
    template_builtin_variant_equality_pattern(module, semantic_index, payload_type, payload)
}
