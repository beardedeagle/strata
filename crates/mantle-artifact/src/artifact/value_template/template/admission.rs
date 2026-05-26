use super::*;
use predicates::{
    validate_bool_contract_type, validate_boolean_operand_template,
    validate_equality_operand_template, validate_equality_operands, validate_scalar_value_type,
};
use projections::{
    reject_projected_process_ref_type, reject_type_containing_process_ref,
    validate_enum_payload_projection, validate_enum_variant_payload,
    validate_list_element_projection_type, validate_list_rest_projection_type,
    validate_map_rest_projection_type, validate_map_value_projection_type,
    validate_record_field_projection_type,
};
use static_keys::{is_static_map_key_template, static_map_key_template_value};
use template_types::{
    validate_list_template_type, validate_map_template_type, validate_record_template_type,
};

mod predicates;
mod projections;
mod static_keys;
mod template_types;

impl ArtifactValueTemplate {
    pub(in crate::artifact) fn validate_for_received_payload(
        &self,
        artifact: &MantleArtifact,
        field: &str,
        validation: ValueTemplatePayloadValidation,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        artifact.type_entry(self.result_type())?;
        if let Some(expected_type) = validation.expected_type {
            if self.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type id {}, expected {}",
                    self.result_type().as_u32(),
                    expected_type.as_u32()
                )));
            }
        }
        match self {
            Self::Literal { ty, value } => artifact.validate_value_matches_type(field, *ty, value),
            Self::ReceivedPayload { ty } => {
                let Some(received_payload_type) = validation.received_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing transition message"
                    )));
                };
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "{field} has received payload type id {}, expected {}",
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                if !validation.allow_direct_process_ref
                    && matches!(
                        artifact.type_entry(*ty)?.kind,
                        ArtifactTypeKind::ProcessRef { .. }
                    )
                {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                Ok(())
            }
            Self::CurrentStatePayload { ty } => {
                let Some(current_state_payload_type) = validation.current_state_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing current state"
                    )));
                };
                if *ty != current_state_payload_type {
                    return Err(Error::new(format!(
                        "{field} has current state payload type id {}, expected {}",
                        ty.as_u32(),
                        current_state_payload_type.as_u32()
                    )));
                }
                if !validation.allow_direct_process_ref
                    && matches!(
                        artifact.type_entry(*ty)?.kind,
                        ArtifactTypeKind::ProcessRef { .. }
                    )
                {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                Ok(())
            }
            Self::EnumPayload { ty, value, variant } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_enum_payload_projection(
                    artifact,
                    field,
                    value.result_type(),
                    *variant,
                    *ty,
                )?;
                value.validate_for_received_payload(
                    artifact,
                    &format!("{field}.value"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::RecordField {
                ty,
                record,
                field: field_name,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_ident_field(&format!("{field}.field_name"), field_name)?;
                validate_record_field_projection_type(
                    artifact,
                    field,
                    record.result_type(),
                    field_name,
                    *ty,
                )?;
                record.validate_for_received_payload(
                    artifact,
                    &format!("{field}.record"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::ListElement {
                ty,
                list,
                index,
                len,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_list_element_projection_type(artifact, field, list.result_type(), *ty)?;
                validate_count(&format!("{field}.len"), *len, 1, MAX_VALUE_TEMPLATE_FIELDS)?;
                if *index >= *len {
                    return Err(Error::new(format!(
                        "{field}.index {index} is outside list length {len}"
                    )));
                }
                list.validate_for_received_payload(
                    artifact,
                    &format!("{field}.list"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_list_element_projection_type(artifact, field, list.result_type(), *ty)?;
                validate_count(
                    &format!("{field}.prefix_len"),
                    *prefix_len,
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                if *index >= *prefix_len {
                    return Err(Error::new(format!(
                        "{field}.index {index} is outside list prefix length {prefix_len}"
                    )));
                }
                list.validate_for_received_payload(
                    artifact,
                    &format!("{field}.list"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::ListRest {
                ty,
                list,
                prefix_len,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_list_rest_projection_type(artifact, field, list.result_type(), *ty)?;
                validate_count(
                    &format!("{field}.prefix_len"),
                    *prefix_len,
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                list.validate_for_received_payload(
                    artifact,
                    &format!("{field}.list"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::MapValue {
                ty,
                map,
                key,
                keys,
                projection: _,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_map_value_projection_type(
                    artifact,
                    field,
                    map.result_type(),
                    key,
                    keys,
                    *ty,
                )?;
                validate_projection_keys(field, key, keys)?;
                map.validate_for_received_payload(
                    artifact,
                    &format!("{field}.map"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::MapRest {
                ty,
                map,
                excluded_keys,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_map_rest_projection_type(
                    artifact,
                    field,
                    map.result_type(),
                    excluded_keys,
                    *ty,
                )?;
                validate_projection_key_set(field, excluded_keys, ProjectionKeySetKind::Excluded)?;
                map.validate_for_received_payload(
                    artifact,
                    &format!("{field}.map"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::ProcessRef {
                ty, target_process, ..
            } => {
                if let Some(expected_type) = validation.expected_type
                    && *ty != expected_type
                {
                    return Err(Error::new(format!(
                        "{field} has type id {}, expected {}",
                        ty.as_u32(),
                        expected_type.as_u32()
                    )));
                }
                if !validation.allow_direct_process_ref {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                artifact.validate_process_ref_type_id_target(
                    &format!("{field}.type_id"),
                    *ty,
                    *target_process,
                )
            }
            Self::LoopElement { ty, .. } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)
            }
            Self::EffectOutcome { ty, .. } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                if !validation.allow_process_ref_effect_outcome {
                    reject_type_containing_process_ref(artifact, field, *ty)?;
                }
                Ok(())
            }
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_enum_variant_payload(
                    artifact,
                    field,
                    *ty,
                    *variant,
                    payload.result_type(),
                )?;
                payload.validate_for_received_payload(
                    artifact,
                    &format!("{field}.payload"),
                    validation.nested(),
                    depth + 1,
                )
            }
            Self::Record { ty, fields } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let expected_fields = validate_record_template_type(artifact, field, *ty, fields)?;
                validate_count(
                    &format!("{field}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, record_field) in fields.iter().enumerate() {
                    validate_ident_field(&format!("{field}.field"), &record_field.name)?;
                    if fields[..index]
                        .iter()
                        .any(|previous| previous.name == record_field.name)
                    {
                        return Err(Error::new(format!(
                            "{field} duplicates field {}",
                            record_field.name
                        )));
                    }
                    let Some(expected) = expected_fields
                        .iter()
                        .find(|expected| expected.name == record_field.name)
                    else {
                        return Err(Error::new(format!(
                            "{field}.field {} is not declared by type id {}",
                            record_field.name,
                            ty.as_u32()
                        )));
                    };
                    record_field.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.field.{}", record_field.name),
                        validation.nested().with_expected_type(Some(expected.ty)),
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::List { ty, items } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let (element, capacity) = validate_list_template_type(artifact, field, *ty)?;
                validate_count(
                    &format!("{field}.item_count"),
                    items.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                if items.len() > capacity {
                    return Err(Error::new(format!(
                        "{field}.item_count is {}, capacity is {}",
                        items.len(),
                        capacity
                    )));
                }
                for (index, item) in items.iter().enumerate() {
                    item.validate_for_received_payload(
                        artifact,
                        &format!("{field}.item.{index}"),
                        validation.nested().with_expected_type(Some(element)),
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::Map { ty, entries } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let (key_type, value_type, capacity) =
                    validate_map_template_type(artifact, field, *ty)?;
                validate_count(
                    &format!("{field}.entry_count"),
                    entries.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                if entries.len() > capacity {
                    return Err(Error::new(format!(
                        "{field}.entry_count is {}, capacity is {}",
                        entries.len(),
                        capacity
                    )));
                }
                let mut keys = Vec::with_capacity(entries.len());
                for (index, entry) in entries.iter().enumerate() {
                    if !is_static_map_key_template(&entry.key) {
                        return Err(Error::new(format!(
                            "{field}.entry.{index}.key must be a static value template"
                        )));
                    }
                    entry.key.validate_for_received_payload(
                        artifact,
                        &format!("{field}.entry.{index}.key"),
                        validation.nested().with_expected_type(Some(key_type)),
                        depth + 1,
                    )?;
                    let key = static_map_key_template_value(artifact, &entry.key)?;
                    if keys.iter().any(|previous| previous == &key) {
                        return Err(Error::new(format!(
                            "{field} duplicates key {}",
                            key.label()
                        )));
                    }
                    keys.push(key);
                    entry.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.entry.{index}.value"),
                        validation.nested().with_expected_type(Some(value_type)),
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::IfElse {
                ty,
                condition,
                then_value,
                else_value,
            } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let bool_ty = condition.result_type();
                validate_bool_contract_type(
                    artifact,
                    &format!("{field}.condition.type_id"),
                    bool_ty,
                )?;
                condition.validate_for_received_payload(
                    artifact,
                    &format!("{field}.condition"),
                    validation.nested().with_expected_type(Some(bool_ty)),
                    depth + 1,
                )?;
                let nested = validation.nested().with_expected_type(Some(*ty));
                then_value.validate_for_received_payload(
                    artifact,
                    &format!("{field}.then"),
                    nested,
                    depth + 1,
                )?;
                else_value.validate_for_received_payload(
                    artifact,
                    &format!("{field}.else"),
                    nested,
                    depth + 1,
                )
            }
            Self::Equality {
                ty,
                operand_ty,
                left,
                right,
                ..
            } => {
                validate_bool_contract_type(artifact, &format!("{field}.type_id"), *ty)?;
                let equality_admission =
                    validate_equality_operands(artifact, field, *operand_ty, left, right)?;
                validate_equality_operand_template(field, "left", *operand_ty, left)?;
                validate_equality_operand_template(field, "right", *operand_ty, right)?;
                let nested = validation
                    .nested()
                    .with_expected_type(Some(*operand_ty))
                    .with_process_ref_effect_outcome(
                        equality_admission.allow_process_ref_effect_outcome,
                    );
                left.validate_for_received_payload(
                    artifact,
                    &format!("{field}.left"),
                    nested,
                    depth + 1,
                )?;
                right.validate_for_received_payload(
                    artifact,
                    &format!("{field}.right"),
                    nested,
                    depth + 1,
                )
            }
            Self::ScalarArithmetic {
                ty, left, right, ..
            } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_scalar_value_type(artifact, &format!("{field}.type_id"), *ty)?;
                let nested = validation.nested().with_expected_type(Some(*ty));
                left.validate_for_received_payload(
                    artifact,
                    &format!("{field}.left"),
                    nested,
                    depth + 1,
                )?;
                right.validate_for_received_payload(
                    artifact,
                    &format!("{field}.right"),
                    nested,
                    depth + 1,
                )
            }
            Self::ScalarOrdering {
                ty,
                operand_ty,
                left,
                right,
                ..
            } => {
                validate_bool_contract_type(artifact, &format!("{field}.type_id"), *ty)?;
                validate_scalar_value_type(
                    artifact,
                    &format!("{field}.operand_type_id"),
                    *operand_ty,
                )?;
                validate_equality_operand_template(field, "left", *operand_ty, left)?;
                validate_equality_operand_template(field, "right", *operand_ty, right)?;
                let nested = validation.nested().with_expected_type(Some(*operand_ty));
                left.validate_for_received_payload(
                    artifact,
                    &format!("{field}.left"),
                    nested,
                    depth + 1,
                )?;
                right.validate_for_received_payload(
                    artifact,
                    &format!("{field}.right"),
                    nested,
                    depth + 1,
                )
            }
            Self::BooleanNot { ty, operand } => {
                validate_bool_contract_type(artifact, &format!("{field}.type_id"), *ty)?;
                validate_boolean_operand_template(field, "operand", *ty, operand)?;
                operand.validate_for_received_payload(
                    artifact,
                    &format!("{field}.operand"),
                    validation.nested().with_expected_type(Some(*ty)),
                    depth + 1,
                )
            }
            Self::BooleanBinary {
                ty, left, right, ..
            } => {
                validate_bool_contract_type(artifact, &format!("{field}.type_id"), *ty)?;
                validate_boolean_operand_template(field, "left", *ty, left)?;
                validate_boolean_operand_template(field, "right", *ty, right)?;
                let nested = validation.nested().with_expected_type(Some(*ty));
                left.validate_for_received_payload(
                    artifact,
                    &format!("{field}.left"),
                    nested,
                    depth + 1,
                )?;
                right.validate_for_received_payload(
                    artifact,
                    &format!("{field}.right"),
                    nested,
                    depth + 1,
                )
            }
        }
    }
}
