use super::*;
use crate::language::checker::symbols::BuiltinValueShape;
use mantle_artifact::ArtifactPrimitiveType;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EqualityOperandKind {
    Structural,
    BuiltinVariantPatternOnly,
}

pub(super) fn validate_source_equality_expr(
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
    validate_source_equality_operands(scope, &operand_type, left, right)?;
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

pub(super) fn source_equality_operand_pair_type(
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
            let right_type =
                source_equality_operand_type(scope, right, bindings, Some(&left_type))?;
            validate_matching_source_equality_operand_types(scope, left_type, right_type)
        }
        (Err(_), Ok(right_type)) => {
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
                        "equality operand {name} must be a Bool, String, Bytes, scalar value, or fieldless enum value: {err}"
                    ))
                })
        }
        ValueExpr::StringLiteral(_) => Ok(TypeRef::Named(Identifier::new(
            ArtifactPrimitiveType::String.source_name(),
        )?)),
        ValueExpr::BytesLiteral(_) => Ok(TypeRef::Named(Identifier::new(
            ArtifactPrimitiveType::Bytes.source_name(),
        )?)),
        ValueExpr::ScalarLiteral(_) | ValueExpr::ScalarArithmetic { .. } => {
            source_scalar_expr_type(scope, value, bindings, expected_type)
        }
        ValueExpr::Call { name, .. } => {
            if let Some(expected_type) = expected_type {
                if enum_variant_for_expected_type(scope, expected_type, name)?.is_some() {
                    return Ok(expected_type.clone());
                }
                if scope.semantic_index.scalar_type(expected_type)?.is_some() {
                    return source_scalar_expr_type(scope, value, bindings, Some(expected_type));
                }
                if scope
                    .semantic_index
                    .primitive_type(expected_type)?
                    .is_some()
                    && let Some(function) = source_function_group_option(scope, name)?
                        .and_then(|functions| functions.first())
                    && scope
                        .semantic_index
                        .same_type(&function.return_type, expected_type)
                {
                    return Ok(expected_type.clone());
                }
            } else if let Some(function) =
                source_function_group_option(scope, name)?.and_then(|functions| functions.first())
            {
                return Ok(function.return_type.clone());
            }
            Err(Error::new(
                "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
            ))
        }
        ValueExpr::IfElse { .. } => {
            let Some(expected_type) = expected_type else {
                return Err(Error::new(
                    "scalar equality operand type is ambiguous; use a typed local binding or scalar literal",
                ));
            };
            if scope.semantic_index.scalar_type(expected_type)?.is_some() {
                return source_scalar_expr_type(scope, value, bindings, Some(expected_type));
            }
            if scope
                .semantic_index
                .primitive_type(expected_type)?
                .is_some()
            {
                return Ok(expected_type.clone());
            }
            Err(Error::new(
                "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
            ))
        }
        ValueExpr::Grouped { value } => {
            source_equality_operand_type(scope, value, bindings, expected_type)
        }
        ValueExpr::EnumVariant { name, .. } => {
            if let Some(expected_type) = expected_type
                && enum_variant_for_expected_type(scope, expected_type, name)?.is_some()
            {
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
        ValueExpr::Record(_)
        | ValueExpr::List(_)
        | ValueExpr::Map(_)
        | ValueExpr::Equality { .. }
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. } => Err(Error::new(
            "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
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
) -> Result<EqualityOperandKind> {
    validate_source_equality_operand_type_at_depth(scope, operand_type, 0)
}

fn validate_source_equality_operand_type_at_depth(
    scope: &SourceFunctionScope<'_>,
    operand_type: &TypeRef,
    depth: usize,
) -> Result<EqualityOperandKind> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "equality operand type nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if scope.semantic_index.is_unit_type(operand_type)? {
        return Ok(EqualityOperandKind::Structural);
    }
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    if scope.semantic_index.same_type(operand_type, &bool_type) {
        return Ok(EqualityOperandKind::Structural);
    }
    if scope.semantic_index.scalar_type(operand_type)?.is_some() {
        validate_source_scalar_operand_type(scope, operand_type)?;
        return Ok(EqualityOperandKind::Structural);
    }
    if scope.semantic_index.primitive_type(operand_type)?.is_some() {
        return Ok(EqualityOperandKind::Structural);
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
        return Err(Error::new("list and map equality are not supported"));
    }
    if scope
        .semantic_index
        .record_decl(scope.module, operand_type)
        .is_ok()
    {
        return Err(Error::new("record equality is not supported"));
    }
    if let Some(BuiltinValueShape::Enum(value_enum)) =
        scope.semantic_index.builtin_value_shape(operand_type)?
    {
        return Ok(
            if value_enum
                .variants
                .iter()
                .any(|variant| variant.payload_type.is_some())
            {
                EqualityOperandKind::BuiltinVariantPatternOnly
            } else {
                EqualityOperandKind::Structural
            },
        );
    }
    let value_enum = scope
        .semantic_index
        .value_enum(scope.module, operand_type)?;
    if value_enum
        .variants
        .iter()
        .any(|variant| variant.payload_type.is_some())
    {
        return Err(Error::new(format!(
            "equality type {operand_type} must not declare payload-bearing enum variants"
        )));
    }
    Ok(EqualityOperandKind::Structural)
}

fn validate_source_equality_operands(
    scope: &SourceFunctionScope<'_>,
    operand_type: &TypeRef,
    left: &ValueExpr,
    right: &ValueExpr,
) -> Result<()> {
    match validate_source_equality_operand_type(scope, operand_type)? {
        EqualityOperandKind::Structural => Ok(()),
        EqualityOperandKind::BuiltinVariantPatternOnly
            if (source_builtin_variant_equality_pattern(scope, operand_type, left)?
                || source_builtin_variant_equality_pattern(scope, operand_type, right)?) =>
        {
            Ok(())
        }
        EqualityOperandKind::BuiltinVariantPatternOnly => Err(Error::new(format!(
            "equality over built-in payload enum {operand_type} requires one operand to be a safe built-in variant pattern"
        ))),
    }
}

fn source_builtin_variant_equality_pattern(
    scope: &SourceFunctionScope<'_>,
    operand_type: &TypeRef,
    value: &ValueExpr,
) -> Result<bool> {
    source_builtin_variant_equality_pattern_at_depth(scope, operand_type, value, 0)
}

fn source_builtin_variant_equality_pattern_at_depth(
    scope: &SourceFunctionScope<'_>,
    operand_type: &TypeRef,
    value: &ValueExpr,
    depth: usize,
) -> Result<bool> {
    let value = match value {
        ValueExpr::Grouped { value } => value.as_ref(),
        _ => value,
    };
    let Some(BuiltinValueShape::Enum(value_enum)) =
        scope.semantic_index.builtin_value_shape(operand_type)?
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
                if is_builtin_equality_variant_label(name.as_str()) {
                    return Err(Error::new(format!(
                        "value {name} is not a variant of enum {}",
                        value_enum.name
                    )));
                }
                return Ok(false);
            };
            Ok(variant.payload_type.is_none())
        }
        ValueExpr::Call { name, arg } | ValueExpr::EnumVariant { name, payload: arg } => {
            let Some(variant) = value_enum
                .variants
                .iter()
                .find(|variant| variant.name == *name)
            else {
                if is_builtin_equality_variant_label(name.as_str()) {
                    return Err(Error::new(format!(
                        "value {name} is not a variant of enum {}",
                        value_enum.name
                    )));
                }
                return Ok(false);
            };
            let Some(payload_type) = &variant.payload_type else {
                return Ok(false);
            };
            source_equality_payload_pattern_is_safe(scope, payload_type, arg, depth + 1)
        }
        _ => Ok(false),
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

fn source_equality_payload_pattern_is_safe(
    scope: &SourceFunctionScope<'_>,
    payload_type: &TypeRef,
    payload: &ValueExpr,
    depth: usize,
) -> Result<bool> {
    match validate_source_equality_operand_type_at_depth(scope, payload_type, depth)? {
        EqualityOperandKind::Structural => Ok(true),
        EqualityOperandKind::BuiltinVariantPatternOnly => {
            source_builtin_variant_equality_pattern_at_depth(scope, payload_type, payload, depth)
        }
    }
}
