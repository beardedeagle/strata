use super::*;

pub(super) fn validate_record_template_type<'a>(
    artifact: &'a MantleArtifact,
    field: &str,
    ty: TypeId,
    actual_fields: &[ArtifactValueTemplateField],
) -> Result<&'a [ArtifactTypeField]> {
    let type_entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::Record { fields } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a record type",
            ty.as_u32()
        )));
    };
    if actual_fields.len() != fields.len() {
        return Err(Error::new(format!(
            "{field}.field_count is {}, expected {}",
            actual_fields.len(),
            fields.len()
        )));
    }
    for actual in actual_fields {
        if fields.get(actual.field.index()).is_none() {
            return Err(Error::new(format!(
                "{field}.field_id {} is not declared by type id {}",
                actual.field.as_u32(),
                ty.as_u32()
            )));
        }
    }
    Ok(fields)
}

pub(super) fn validate_list_template_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<(TypeId, usize)> {
    let type_entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::List { element, capacity } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a list type",
            ty.as_u32()
        )));
    };
    Ok((*element, *capacity))
}

pub(super) fn validate_map_template_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<(TypeId, TypeId, usize)> {
    let type_entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::Map {
        key,
        value,
        capacity,
    } = type_entry.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a map type",
            ty.as_u32()
        )));
    };
    Ok((*key, *value, *capacity))
}
