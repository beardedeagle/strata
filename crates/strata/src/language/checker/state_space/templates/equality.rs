use crate::language::ast::{ValueBooleanOperator, ValueEqualityOperator};
use crate::language::checked::{CheckedValueBooleanOperator, CheckedValueEqualityOperator};

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn checked_equality_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    operator: ValueEqualityOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    if !semantic_index.same_type(expected_type, &bool_type) {
        return Err(Error::new(format!(
            "equality expression produces {bool_type}, expected {expected_type}"
        )));
    }
    let operand_type =
        equality_template_operand_pair_type(module, semantic_index, left, right, bindings)?;
    validate_equality_template_operand_type(module, semantic_index, &operand_type)?;
    let operand_ty = types.intern(&operand_type)?;
    Ok(CheckedValueTemplate::Equality {
        ty: types.intern(&bool_type)?,
        operand_ty,
        operator: checked_equality_operator(operator),
        left: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &operand_type,
            left,
            bindings,
            depth + 1,
        )?),
        right: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &operand_type,
            right,
            bindings,
            depth + 1,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn checked_boolean_binary_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    operator: ValueBooleanOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    validate_boolean_template_result_type(semantic_index, expected_type, &bool_type)?;
    let ty = types.intern(&bool_type)?;
    Ok(CheckedValueTemplate::BooleanBinary {
        ty,
        operator: checked_boolean_operator(operator),
        left: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &bool_type,
            left,
            bindings,
            depth + 1,
        )?),
        right: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &bool_type,
            right,
            bindings,
            depth + 1,
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
    let bool_type = semantic_index.bool_type(module)?;
    validate_boolean_template_result_type(semantic_index, expected_type, &bool_type)?;
    checked_value_template(
        module,
        semantic_index,
        types,
        &bool_type,
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
        (Ok(left_type), Ok(right_type)) => validate_matching_equality_template_operand_types(
            module,
            semantic_index,
            left_type,
            right_type,
        ),
        (Ok(left_type), Err(_)) => {
            validate_equality_template_operand_type(module, semantic_index, &left_type)?;
            let right_type = equality_template_operand_type(
                module,
                semantic_index,
                right,
                bindings,
                Some(&left_type),
            )?;
            validate_matching_equality_template_operand_types(
                module,
                semantic_index,
                left_type,
                right_type,
            )
        }
        (Err(_), Ok(right_type)) => {
            validate_equality_template_operand_type(module, semantic_index, &right_type)?;
            let left_type = equality_template_operand_type(
                module,
                semantic_index,
                left,
                bindings,
                Some(&right_type),
            )?;
            validate_matching_equality_template_operand_types(
                module,
                semantic_index,
                left_type,
                right_type,
            )
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

fn validate_matching_equality_template_operand_types(
    module: &Module,
    semantic_index: &SemanticIndex,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    validate_equality_template_operand_type(module, semantic_index, &left_type)?;
    validate_equality_template_operand_type(module, semantic_index, &right_type)?;
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
                        "equality operand {name} must be a Bool or fieldless enum value: {err}"
                    ))
                })
        }
        ValueExpr::EnumVariant { name, .. } => {
            if let Some(expected_type) = expected_type
                && let Some(variant) =
                    enum_variant_for_expected_type(module, semantic_index, expected_type, name)?
            {
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "equality operand enum variant {name} carries a payload"
                    )));
                }
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
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => Err(Error::new(
            "equality operands must be Bool or fieldless enum values",
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

fn enum_variant_for_expected_type<'module>(
    module: &'module Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<&'module crate::language::ast::EnumVariant>> {
    let Ok(enum_decl) = semantic_index.enum_decl(module, expected_type) else {
        return Ok(None);
    };
    Ok(enum_decl
        .variants
        .iter()
        .find(|variant| variant.name == *name))
}

fn validate_equality_template_operand_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    operand_type: &TypeRef,
) -> Result<()> {
    let bool_type = semantic_index.bool_type(module)?;
    if semantic_index.same_type(operand_type, &bool_type) {
        return Ok(());
    }
    if semantic_index
        .process_ref_target_type(operand_type)?
        .is_some()
    {
        return Err(Error::new("process-reference equality is not supported"));
    }
    if semantic_index.collection_type(operand_type)?.is_some() {
        return Err(Error::new(
            "list and map equality are not supported in this source slice",
        ));
    }
    if semantic_index.record_decl(module, operand_type).is_ok() {
        return Err(Error::new(
            "record equality is not supported in this source slice",
        ));
    }
    let enum_decl = semantic_index.enum_decl(module, operand_type)?;
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
