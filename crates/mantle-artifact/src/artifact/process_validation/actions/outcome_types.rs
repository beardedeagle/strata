use crate::{
    ArtifactEnumVariant, ArtifactValueShape, Error, MantleArtifact, ProcessId, Result, TypeId,
};

pub(super) fn validate_send_outcome_type(
    artifact: &MantleArtifact,
    outcome_ty: TypeId,
    message_ty: TypeId,
) -> Result<()> {
    let (ok_ty, err_ty) = result_payload_types(artifact, "send outcome type", outcome_ty)?;
    validate_unit_type(artifact, ok_ty)?;
    validate_error_enum_payloads(
        artifact,
        "send outcome error type",
        err_ty,
        &["Full", "Stopped", "Crashed", "MailboxClosed"],
        message_ty,
    )
}

pub(super) fn validate_spawn_outcome_type(
    artifact: &MantleArtifact,
    outcome_ty: TypeId,
    target: ProcessId,
) -> Result<()> {
    let (ok_ty, err_ty) = result_payload_types(artifact, "spawn outcome type", outcome_ty)?;
    artifact.validate_process_ref_type_id_target("spawn outcome success type", ok_ty, target)?;
    let unit_ty = validate_error_enum_shared_payload_type(
        artifact,
        "spawn outcome error type",
        err_ty,
        &["Denied", "Exhausted", "BackendUnavailable"],
    )?;
    validate_unit_type(artifact, unit_ty)
}

fn result_payload_types(
    artifact: &MantleArtifact,
    field: &str,
    outcome_ty: TypeId,
) -> Result<(TypeId, TypeId)> {
    let variants = enum_variants(artifact, field, outcome_ty)?;
    let [ok, err] = variants else {
        return Err(Error::new(format!(
            "{field} type id {} must have exactly Ok and Err variants",
            outcome_ty.as_u32()
        )));
    };
    if ok.label != "Ok" || err.label != "Err" {
        return Err(Error::new(format!(
            "{field} type id {} must declare Ok then Err variants",
            outcome_ty.as_u32()
        )));
    }
    let ok_ty = ok.payload_type.ok_or_else(|| {
        Error::new(format!(
            "{field} type id {} Ok variant must carry a success value",
            outcome_ty.as_u32()
        ))
    })?;
    let err_ty = err.payload_type.ok_or_else(|| {
        Error::new(format!(
            "{field} type id {} Err variant must carry an error value",
            outcome_ty.as_u32()
        ))
    })?;
    Ok((ok_ty, err_ty))
}

fn validate_error_enum_shared_payload_type(
    artifact: &MantleArtifact,
    field: &str,
    error_ty: TypeId,
    expected_labels: &[&str],
) -> Result<TypeId> {
    let variants = enum_variants(artifact, field, error_ty)?;
    if variants.len() != expected_labels.len() {
        return Err(Error::new(format!(
            "{field} type id {} has {} variants, expected {}",
            error_ty.as_u32(),
            variants.len(),
            expected_labels.len()
        )));
    }
    let mut payload_ty: Option<TypeId> = None;
    for (variant, expected_label) in variants.iter().zip(expected_labels) {
        if variant.label != *expected_label {
            return Err(Error::new(format!(
                "{field} type id {} declares variant {}, expected {}",
                error_ty.as_u32(),
                variant.label,
                expected_label
            )));
        }
        let Some(variant_payload_ty) = variant.payload_type else {
            return Err(Error::new(format!(
                "{field} variant {} must carry a payload",
                variant.label
            )));
        };
        match payload_ty {
            Some(expected) if expected != variant_payload_ty => {
                return Err(Error::new(format!(
                    "{field} variant {} carries payload type id {}, expected {}",
                    variant.label,
                    variant_payload_ty.as_u32(),
                    expected.as_u32()
                )));
            }
            Some(_) => {}
            None => payload_ty = Some(variant_payload_ty),
        }
    }
    payload_ty.ok_or_else(|| Error::new(format!("{field} must declare at least one variant")))
}

fn validate_unit_type(artifact: &MantleArtifact, ty: TypeId) -> Result<()> {
    let entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::Atom = entry.value_shape()? else {
        return Err(Error::new(format!(
            "effect outcome Unit type id {} must be an atom value type",
            ty.as_u32()
        )));
    };
    if entry.label != "Unit" {
        return Err(Error::new(format!(
            "effect outcome Unit type id {} must be labeled Unit",
            ty.as_u32()
        )));
    }
    Ok(())
}

fn validate_error_enum_payloads(
    artifact: &MantleArtifact,
    field: &str,
    error_ty: TypeId,
    expected_labels: &[&str],
    payload_ty: TypeId,
) -> Result<()> {
    let variants = enum_variants(artifact, field, error_ty)?;
    if variants.len() != expected_labels.len() {
        return Err(Error::new(format!(
            "{field} type id {} has {} variants, expected {}",
            error_ty.as_u32(),
            variants.len(),
            expected_labels.len()
        )));
    }
    for (variant, expected_label) in variants.iter().zip(expected_labels) {
        if variant.label != *expected_label {
            return Err(Error::new(format!(
                "{field} type id {} declares variant {}, expected {}",
                error_ty.as_u32(),
                variant.label,
                expected_label
            )));
        }
        if variant.payload_type != Some(payload_ty) {
            return Err(Error::new(format!(
                "{field} variant {} must preserve payload type id {}",
                variant.label,
                payload_ty.as_u32()
            )));
        }
    }
    Ok(())
}

fn enum_variants<'a>(
    artifact: &'a MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<&'a [ArtifactEnumVariant]> {
    artifact.validate_value_type(field, ty)?;
    let entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::Enum { variants } = entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field} type id {} must be an enum value type",
            ty.as_u32()
        )));
    };
    Ok(variants)
}
