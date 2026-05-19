use super::*;

pub(super) fn validate_bool_contract_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<()> {
    let type_entry = artifact.type_entry(ty)?;
    if matches!(type_entry.value_shape(), Ok(shape) if is_bool_contract_shape(shape)) {
        return Ok(());
    }
    Err(Error::new(format!(
        "{field} must have type enum Bool {{ False, True }}"
    )))
}

pub(super) fn validate_equality_operand_type(
    artifact: &MantleArtifact,
    field: &str,
    operand_ty: TypeId,
) -> Result<()> {
    let type_entry = artifact.type_entry(operand_ty)?;
    match &type_entry.kind {
        ArtifactTypeKind::ProcessRef { .. } => Err(Error::new(format!(
            "{field}.operand_type_id must be Bool or a fieldless enum value type"
        ))),
        ArtifactTypeKind::Value => match type_entry.value_shape()? {
            ArtifactValueShape::Enum { variants }
                if variants
                    .iter()
                    .all(|variant| variant.payload_type.is_none()) =>
            {
                Ok(())
            }
            _ => Err(Error::new(format!(
                "{field}.operand_type_id must be Bool or a fieldless enum value type"
            ))),
        },
    }
}

pub(super) fn validate_equality_operand_template(
    field: &str,
    side: &str,
    operand_ty: TypeId,
    operand: &ArtifactValueTemplate,
) -> Result<()> {
    if operand.result_type() != operand_ty {
        return Err(Error::new(format!(
            "{field}.{side} has type id {}, expected {}",
            operand.result_type().as_u32(),
            operand_ty.as_u32()
        )));
    }
    Ok(())
}

pub(super) fn validate_boolean_operand_template(
    field: &str,
    side: &str,
    bool_ty: TypeId,
    operand: &ArtifactValueTemplate,
) -> Result<()> {
    if operand.result_type() != bool_ty {
        return Err(Error::new(format!(
            "{field}.{side} has type id {}, expected {}",
            operand.result_type().as_u32(),
            bool_ty.as_u32()
        )));
    }
    Ok(())
}

fn is_bool_contract_shape(shape: &ArtifactValueShape) -> bool {
    matches!(
        shape,
        ArtifactValueShape::Enum { variants }
            if variants.len() == 2
                && variants[0].label == "False"
                && variants[0].payload_type.is_none()
                && variants[1].label == "True"
                && variants[1].payload_type.is_none()
    )
}
