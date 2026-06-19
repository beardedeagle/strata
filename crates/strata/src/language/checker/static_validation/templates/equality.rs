use mantle_artifact::ArtifactValue;

use super::validate_checked_bool_contract_type;
use crate::language::MAX_VALUE_NESTING;
use crate::language::checked::{
    CheckedEnumVariant, CheckedPayloadValue, CheckedTypeKind, CheckedTypeRef, CheckedValueShape,
    CheckedValueTemplate,
};
use crate::language::diagnostic::{Error, Result};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EqualityOperandKind {
    Structural,
    BuiltinVariantPatternOnly,
}

pub(super) fn validate_checked_equality_template(
    result_ty: &CheckedTypeRef,
    operand_ty: &CheckedTypeRef,
    left: &CheckedValueTemplate,
    right: &CheckedValueTemplate,
) -> Result<()> {
    validate_checked_bool_contract_type(result_ty)?;
    if left.result_type() != operand_ty {
        return Err(Error::new(format!(
            "equality left operand has type {}, expected {}",
            left.result_type(),
            operand_ty
        )));
    }
    if right.result_type() != operand_ty {
        return Err(Error::new(format!(
            "equality right operand has type {}, expected {}",
            right.result_type(),
            operand_ty
        )));
    }
    match validate_checked_equality_operand_type(operand_ty)? {
        EqualityOperandKind::Structural => Ok(()),
        EqualityOperandKind::BuiltinVariantPatternOnly
            if (checked_template_is_builtin_variant_pattern(operand_ty, left)?
                || checked_template_is_builtin_variant_pattern(operand_ty, right)?) =>
        {
            Ok(())
        }
        EqualityOperandKind::BuiltinVariantPatternOnly => Err(Error::new(format!(
            "equality over built-in payload enum {operand_ty} requires one operand to be a safe built-in variant pattern"
        ))),
    }
}

fn validate_checked_equality_operand_type(
    operand_ty: &CheckedTypeRef,
) -> Result<EqualityOperandKind> {
    validate_checked_equality_operand_type_at_depth(operand_ty, 0)
}

fn validate_checked_equality_operand_type_at_depth(
    operand_ty: &CheckedTypeRef,
    depth: usize,
) -> Result<EqualityOperandKind> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "equality operand type nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    match operand_ty.kind() {
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Atom,
        } if operand_ty.label() == "Unit" => Ok(EqualityOperandKind::Structural),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Scalar(_) | CheckedValueShape::Primitive(_),
        } => Ok(EqualityOperandKind::Structural),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum { variants },
        } if variants
            .iter()
            .all(|variant| variant.payload_type.is_none()) =>
        {
            Ok(EqualityOperandKind::Structural)
        }
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum { variants },
        } if is_recognized_checked_builtin_equality_enum(variants) => {
            Ok(EqualityOperandKind::BuiltinVariantPatternOnly)
        }
        _ => Err(Error::new(format!(
            "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values, found {operand_ty}"
        ))),
    }
}

fn checked_template_is_builtin_variant_pattern(
    operand_ty: &CheckedTypeRef,
    template: &CheckedValueTemplate,
) -> Result<bool> {
    checked_template_is_builtin_variant_pattern_at_depth(operand_ty, template, 0)
}

fn checked_template_is_builtin_variant_pattern_at_depth(
    operand_ty: &CheckedTypeRef,
    template: &CheckedValueTemplate,
    depth: usize,
) -> Result<bool> {
    match template {
        CheckedValueTemplate::Literal(value) if value.ty() == operand_ty => {
            checked_value_is_builtin_variant_pattern_at_depth(operand_ty, value, depth)
        }
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } if ty == operand_ty => {
            let CheckedTypeKind::Value {
                shape: CheckedValueShape::Enum { variants },
            } = operand_ty.kind()
            else {
                return Ok(false);
            };
            if !is_recognized_checked_builtin_equality_enum(variants) {
                return Ok(false);
            }
            if operand_ty
                .enum_variant_payload_type(*variant)?
                .is_none_or(|payload_ty| payload_ty != payload.result_type().id())
            {
                return Ok(false);
            }
            checked_payload_template_pattern_is_safe(payload, depth + 1)
        }
        _ => Ok(false),
    }
}

fn checked_value_is_builtin_variant_pattern_at_depth(
    operand_ty: &CheckedTypeRef,
    value: &CheckedPayloadValue,
    depth: usize,
) -> Result<bool> {
    let CheckedTypeKind::Value {
        shape: CheckedValueShape::Enum { variants },
    } = operand_ty.kind()
    else {
        return Ok(false);
    };
    if !is_recognized_checked_builtin_equality_enum(variants) {
        return Ok(false);
    }
    let Some(value) = value.value() else {
        return Ok(false);
    };
    artifact_value_is_safe_builtin_variant_pattern(value, depth)
}

fn checked_payload_template_pattern_is_safe(
    template: &CheckedValueTemplate,
    depth: usize,
) -> Result<bool> {
    match validate_checked_equality_operand_type_at_depth(template.result_type(), depth)? {
        EqualityOperandKind::Structural => Ok(true),
        EqualityOperandKind::BuiltinVariantPatternOnly => {
            checked_template_is_builtin_variant_pattern_at_depth(
                template.result_type(),
                template,
                depth,
            )
        }
    }
}

fn artifact_value_is_safe_builtin_variant_pattern(
    value: &ArtifactValue,
    depth: usize,
) -> Result<bool> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "equality operand type nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    match value {
        ArtifactValue::Atom(_)
        | ArtifactValue::String(_)
        | ArtifactValue::Bytes(_)
        | ArtifactValue::Scalar(_) => Ok(true),
        ArtifactValue::EnumVariant { variant, payload }
            if is_builtin_equality_variant_label(variant) =>
        {
            artifact_value_is_safe_builtin_variant_pattern(payload, depth + 1)
        }
        ArtifactValue::EnumVariant { .. }
        | ArtifactValue::Record { .. }
        | ArtifactValue::List(_)
        | ArtifactValue::Map(_)
        | ArtifactValue::ProcessRef { .. } => Ok(false),
    }
}

fn is_builtin_equality_variant_label(label: &str) -> bool {
    matches!(
        label,
        "None"
            | "Some"
            | "Ok"
            | "Err"
            | "Full"
            | "Stopped"
            | "Crashed"
            | "MailboxClosed"
            | "Denied"
            | "Exhausted"
            | "BackendUnavailable"
    )
}

fn is_recognized_checked_builtin_equality_enum(variants: &[CheckedEnumVariant]) -> bool {
    match variants {
        [none, some] => {
            none.name.as_str() == "None"
                && none.payload_type.is_none()
                && some.name.as_str() == "Some"
                && some.payload_type.is_some()
                || none.name.as_str() == "Ok"
                    && none.payload_type.is_some()
                    && some.name.as_str() == "Err"
                    && some.payload_type.is_some()
        }
        [first, second, third] => {
            first.payload_type.is_some()
                && second.payload_type.is_some()
                && third.payload_type.is_some()
                && first.payload_type == second.payload_type
                && second.payload_type == third.payload_type
                && first.name.as_str() == "Denied"
                && second.name.as_str() == "Exhausted"
                && third.name.as_str() == "BackendUnavailable"
        }
        [first, second, third, fourth] => {
            first.payload_type.is_some()
                && second.payload_type.is_some()
                && third.payload_type.is_some()
                && fourth.payload_type.is_some()
                && first.payload_type == second.payload_type
                && second.payload_type == third.payload_type
                && third.payload_type == fourth.payload_type
                && first.name.as_str() == "Full"
                && second.name.as_str() == "Stopped"
                && third.name.as_str() == "Crashed"
                && fourth.name.as_str() == "MailboxClosed"
        }
        _ => false,
    }
}
