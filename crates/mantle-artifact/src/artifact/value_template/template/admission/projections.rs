use super::*;

pub(super) fn reject_projected_process_ref_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<()> {
    if matches!(
        artifact.type_entry(ty)?.kind,
        ArtifactTypeKind::ProcessRef { .. }
    ) {
        return Err(Error::new(format!(
            "{field} process reference template must be a direct message payload"
        )));
    }
    Ok(())
}

pub(super) fn reject_type_containing_process_ref(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<()> {
    if type_contains_process_ref_at_depth(artifact, ty, 0)? {
        return Err(Error::new(format!(
            "{field} process reference outcome must remain step-local"
        )));
    }
    Ok(())
}

fn type_contains_process_ref_at_depth(
    artifact: &MantleArtifact,
    ty: TypeId,
    depth: usize,
) -> Result<bool> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "type id {} nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}",
            ty.as_u32()
        )));
    }
    let type_entry = artifact.type_entry(ty)?;
    match &type_entry.kind {
        ArtifactTypeKind::ProcessRef { .. } => Ok(true),
        ArtifactTypeKind::Value => match type_entry.value_shape()? {
            ArtifactValueShape::Atom
            | ArtifactValueShape::Primitive { .. }
            | ArtifactValueShape::Scalar { .. } => Ok(false),
            ArtifactValueShape::Record { fields } => {
                for field in fields {
                    if type_contains_process_ref_at_depth(artifact, field.ty, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ArtifactValueShape::Enum { variants } => {
                for variant in variants {
                    if let Some(payload_type) = variant.payload_type
                        && type_contains_process_ref_at_depth(artifact, payload_type, depth + 1)?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ArtifactValueShape::List { element, .. } => {
                type_contains_process_ref_at_depth(artifact, *element, depth + 1)
            }
            ArtifactValueShape::Map { key, value, .. } => {
                Ok(
                    type_contains_process_ref_at_depth(artifact, *key, depth + 1)?
                        || type_contains_process_ref_at_depth(artifact, *value, depth + 1)?,
                )
            }
        },
    }
}

pub(super) fn validate_record_field_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    record_ty: TypeId,
    field_id: RecordFieldId,
    projected_ty: TypeId,
) -> Result<()> {
    let record_type = artifact.type_entry(record_ty)?;
    let ArtifactValueShape::Record { fields } = record_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.record type id {} must be a record type",
            record_ty.as_u32()
        )));
    };
    let Some(expected) = fields.get(field_id.index()) else {
        return Err(Error::new(format!(
            "{field}.field_id {} is not declared by type id {}",
            field_id.as_u32(),
            record_ty.as_u32()
        )));
    };
    if expected.ty != projected_ty {
        return Err(Error::new(format!(
            "{field}.type_id has type id {}, expected record field type id {}",
            projected_ty.as_u32(),
            expected.ty.as_u32()
        )));
    }
    Ok(())
}

pub(super) fn validate_list_element_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    list_ty: TypeId,
    projected_ty: TypeId,
) -> Result<()> {
    let list_type = artifact.type_entry(list_ty)?;
    let ArtifactValueShape::List { element, .. } = list_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.list type id {} must be a list type",
            list_ty.as_u32()
        )));
    };
    if *element != projected_ty {
        return Err(Error::new(format!(
            "{field}.type_id has type id {}, expected list element type id {}",
            projected_ty.as_u32(),
            element.as_u32()
        )));
    }
    Ok(())
}

pub(super) fn validate_list_rest_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    list_ty: TypeId,
    projected_ty: TypeId,
) -> Result<()> {
    let source_type = artifact.type_entry(list_ty)?;
    let ArtifactValueShape::List { element, .. } = source_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.list type id {} must be a list type",
            list_ty.as_u32()
        )));
    };
    let projected_type = artifact.type_entry(projected_ty)?;
    let ArtifactValueShape::List {
        element: projected_element,
        ..
    } = projected_type.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a list type",
            projected_ty.as_u32()
        )));
    };
    if element != projected_element {
        return Err(Error::new(format!(
            "{field}.type_id has list element type id {}, expected {}",
            projected_element.as_u32(),
            element.as_u32()
        )));
    }
    Ok(())
}

pub(super) fn validate_map_value_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    map_ty: TypeId,
    key: &ArtifactValue,
    keys: &[ArtifactValue],
    projected_ty: TypeId,
) -> Result<()> {
    let map_type = artifact.type_entry(map_ty)?;
    let ArtifactValueShape::Map {
        key: key_type,
        value,
        ..
    } = map_type.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.map type id {} must be a map type",
            map_ty.as_u32()
        )));
    };
    if *value != projected_ty {
        return Err(Error::new(format!(
            "{field}.type_id has type id {}, expected map value type id {}",
            projected_ty.as_u32(),
            value.as_u32()
        )));
    }
    artifact.validate_value_matches_type(&format!("{field}.key"), *key_type, key)?;
    for (index, expected_key) in keys.iter().enumerate() {
        artifact.validate_value_matches_type(
            &format!("{field}.expected_key.{index}"),
            *key_type,
            expected_key,
        )?;
    }
    Ok(())
}

pub(super) fn validate_map_rest_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    map_ty: TypeId,
    excluded_keys: &[ArtifactValue],
    projected_ty: TypeId,
) -> Result<()> {
    let map_type = artifact.type_entry(map_ty)?;
    let ArtifactValueShape::Map { key, value, .. } = map_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.map type id {} must be a map type",
            map_ty.as_u32()
        )));
    };
    let projected_type = artifact.type_entry(projected_ty)?;
    let ArtifactValueShape::Map {
        key: projected_key,
        value: projected_value,
        ..
    } = projected_type.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a map type",
            projected_ty.as_u32()
        )));
    };
    if key != projected_key {
        return Err(Error::new(format!(
            "{field}.type_id has map key type id {}, expected {}",
            projected_key.as_u32(),
            key.as_u32()
        )));
    }
    if value != projected_value {
        return Err(Error::new(format!(
            "{field}.type_id has map value type id {}, expected {}",
            projected_value.as_u32(),
            value.as_u32()
        )));
    }
    for (index, excluded_key) in excluded_keys.iter().enumerate() {
        artifact.validate_value_matches_type(
            &format!("{field}.excluded_key.{index}"),
            *key,
            excluded_key,
        )?;
    }
    Ok(())
}

pub(super) fn validate_enum_payload_projection(
    artifact: &MantleArtifact,
    field: &str,
    enum_ty: TypeId,
    variant: crate::EnumVariantId,
    projected_ty: TypeId,
) -> Result<()> {
    artifact.validate_value_type(&format!("{field}.enum_type_id"), enum_ty)?;
    let payload_type = artifact
        .enum_variant_payload_type(enum_ty, variant)
        .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
    match payload_type {
        Some(expected) if expected == projected_ty => Ok(()),
        Some(expected) => Err(Error::new(format!(
            "{field}.type_id has type id {}, expected enum payload type id {}",
            projected_ty.as_u32(),
            expected.as_u32()
        ))),
        None => Err(Error::new(format!(
            "{field}.variant_id {} does not carry a payload",
            variant.as_u32()
        ))),
    }
}

pub(super) fn validate_enum_variant_payload(
    artifact: &MantleArtifact,
    field: &str,
    enum_ty: TypeId,
    variant: crate::EnumVariantId,
    payload_ty: TypeId,
) -> Result<()> {
    artifact.validate_value_type(&format!("{field}.enum_type_id"), enum_ty)?;
    let expected = artifact
        .enum_variant_payload_type(enum_ty, variant)
        .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
    match expected {
        Some(expected) if expected == payload_ty => Ok(()),
        Some(expected) => Err(Error::new(format!(
            "{field}.payload has type id {}, expected {}",
            payload_ty.as_u32(),
            expected.as_u32()
        ))),
        None => Err(Error::new(format!(
            "{field}.variant_id {} does not carry a payload",
            variant.as_u32()
        ))),
    }
}
