use crate::fields::ArtifactFields;
use crate::{
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactScalarType,
    ArtifactValueShape, ArtifactValueTemplate, Result,
};

use super::decode_value_template;

pub(super) fn decode_scalar_shape(
    fields: &mut ArtifactFields,
    prefix: &str,
) -> Result<ArtifactValueShape> {
    Ok(ArtifactValueShape::Scalar {
        scalar: ArtifactScalarType::parse_artifact_name(
            &format!("{prefix}.scalar_type"),
            &fields.take_required(&format!("{prefix}.scalar_type"))?,
        )?,
    })
}

pub(super) fn decode_scalar_arithmetic_template(
    fields: &mut ArtifactFields,
    prefix: &str,
    depth: usize,
) -> Result<ArtifactValueTemplate> {
    let operator_field = format!("{prefix}.operator");
    Ok(ArtifactValueTemplate::ScalarArithmetic {
        ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
        operator: ArtifactScalarArithmeticOperator::parse(
            &operator_field,
            &fields.take_required(&operator_field)?,
        )?,
        left: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.left"),
            depth + 1,
        )?),
        right: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.right"),
            depth + 1,
        )?),
    })
}

pub(super) fn decode_scalar_ordering_template(
    fields: &mut ArtifactFields,
    prefix: &str,
    depth: usize,
) -> Result<ArtifactValueTemplate> {
    let operator_field = format!("{prefix}.operator");
    Ok(ArtifactValueTemplate::ScalarOrdering {
        ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
        operand_ty: fields.take_type_id(&format!("{prefix}.operand_type_id"))?,
        operator: ArtifactScalarOrderingOperator::parse(
            &operator_field,
            &fields.take_required(&operator_field)?,
        )?,
        left: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.left"),
            depth + 1,
        )?),
        right: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.right"),
            depth + 1,
        )?),
    })
}
