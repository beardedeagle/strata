use super::*;

pub(in crate::artifact) fn validate_template_loop_elements(
    artifact: &MantleArtifact,
    template: &ArtifactValueTemplate,
    active_loop_elements: &[ActiveArtifactLoopElement],
    field: &str,
) -> Result<()> {
    match template {
        ArtifactValueTemplate::Literal { .. }
        | ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::ProcessRef { .. } => Ok(()),
        ArtifactValueTemplate::LoopElement { ty, element } => {
            artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
            let Some(active) = active_loop_elements
                .iter()
                .find(|active| active.id == *element)
            else {
                return Err(Error::new(format!(
                    "{field} references inactive loop element id {}",
                    element.as_u32()
                )));
            };
            if active.ty != *ty {
                return Err(Error::new(format!(
                    "{field} loop element id {} has type id {}, expected {}",
                    element.as_u32(),
                    active.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(())
        }
        ArtifactValueTemplate::EnumPayload { value, .. } => {
            validate_template_loop_elements(artifact, value, active_loop_elements, field)
        }
        ArtifactValueTemplate::RecordField { record, .. } => {
            validate_template_loop_elements(artifact, record, active_loop_elements, field)
        }
        ArtifactValueTemplate::ListElement { list, .. }
        | ArtifactValueTemplate::ListPrefixElement { list, .. }
        | ArtifactValueTemplate::ListRest { list, .. } => {
            validate_template_loop_elements(artifact, list, active_loop_elements, field)
        }
        ArtifactValueTemplate::MapValue { map, .. } => {
            validate_template_loop_elements(artifact, map, active_loop_elements, field)
        }
        ArtifactValueTemplate::MapRest { map, .. } => {
            validate_template_loop_elements(artifact, map, active_loop_elements, field)
        }
        ArtifactValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            validate_template_loop_elements(
                artifact,
                condition,
                active_loop_elements,
                &format!("{field}.condition"),
            )?;
            validate_template_loop_elements(
                artifact,
                then_value,
                active_loop_elements,
                &format!("{field}.then"),
            )?;
            validate_template_loop_elements(
                artifact,
                else_value,
                active_loop_elements,
                &format!("{field}.else"),
            )
        }
        ArtifactValueTemplate::Equality { left, right, .. }
        | ArtifactValueTemplate::ScalarArithmetic { left, right, .. }
        | ArtifactValueTemplate::ScalarOrdering { left, right, .. } => {
            validate_template_loop_elements(
                artifact,
                left,
                active_loop_elements,
                &format!("{field}.left"),
            )?;
            validate_template_loop_elements(
                artifact,
                right,
                active_loop_elements,
                &format!("{field}.right"),
            )
        }
        ArtifactValueTemplate::BooleanNot { operand, .. } => validate_template_loop_elements(
            artifact,
            operand,
            active_loop_elements,
            &format!("{field}.operand"),
        ),
        ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
            validate_template_loop_elements(
                artifact,
                left,
                active_loop_elements,
                &format!("{field}.left"),
            )?;
            validate_template_loop_elements(
                artifact,
                right,
                active_loop_elements,
                &format!("{field}.right"),
            )
        }
        ArtifactValueTemplate::EnumVariant { payload, .. } => {
            validate_template_loop_elements(artifact, payload, active_loop_elements, field)
        }
        ArtifactValueTemplate::Record { fields, .. } => {
            for record_field in fields {
                validate_template_loop_elements(
                    artifact,
                    &record_field.value,
                    active_loop_elements,
                    &format!("{field}.{}", record_field.name),
                )?;
            }
            Ok(())
        }
        ArtifactValueTemplate::List { items, .. } => {
            for (index, item) in items.iter().enumerate() {
                validate_template_loop_elements(
                    artifact,
                    item,
                    active_loop_elements,
                    &format!("{field}.{index}"),
                )?;
            }
            Ok(())
        }
        ArtifactValueTemplate::Map { entries, .. } => {
            for (index, entry) in entries.iter().enumerate() {
                validate_template_loop_elements(
                    artifact,
                    &entry.key,
                    active_loop_elements,
                    &format!("{field}.{index}.key"),
                )?;
                validate_template_loop_elements(
                    artifact,
                    &entry.value,
                    active_loop_elements,
                    &format!("{field}.{index}.value"),
                )?;
            }
            Ok(())
        }
    }
}

pub(in crate::artifact) fn validate_bool_condition_template(
    artifact: &MantleArtifact,
    field: &str,
    condition: &ArtifactValueTemplate,
    received_payload_type: Option<TypeId>,
    current_state_payload: Option<&ArtifactPayload>,
) -> Result<()> {
    let bool_type = condition.result_type();
    let ty = artifact.type_entry(bool_type)?;
    let is_bool_contract = matches!(
        ty.value_shape(),
        Ok(ArtifactValueShape::Enum { variants })
            if variants.len() == 2
                && variants[0].label == "False"
                && variants[0].payload_type.is_none()
                && variants[1].label == "True"
                && variants[1].payload_type.is_none()
    );
    if !is_bool_contract {
        return Err(Error::new(format!(
            "{field} must have type enum Bool {{ False, True }}"
        )));
    }
    validate_bool_condition_template_shape(field, condition)?;
    condition.validate_for_received_payload(
        artifact,
        field,
        ValueTemplatePayloadValidation::new(
            Some(bool_type),
            received_payload_type,
            current_state_payload.map(|payload| payload.ty),
            false,
        ),
        0,
    )?;
    validate_static_bool_condition_value(artifact, field, condition, current_state_payload)
}

pub(in crate::artifact) fn validate_for_each_collection_type(
    artifact: &MantleArtifact,
    field: &str,
    collection: &ArtifactValueTemplate,
    element_type: TypeId,
) -> Result<()> {
    let collection_type = collection.result_type();
    let type_entry = artifact.type_entry(collection_type)?;
    let ArtifactValueShape::List { element, .. } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field} type id {} must be a list type",
            collection_type.as_u32()
        )));
    };
    if *element != element_type {
        return Err(Error::new(format!(
            "{field} element type id {}, expected {}",
            element.as_u32(),
            element_type.as_u32()
        )));
    }
    Ok(())
}

fn validate_bool_condition_template_shape(
    field: &str,
    condition: &ArtifactValueTemplate,
) -> Result<()> {
    match condition {
        ArtifactValueTemplate::Literal { .. }
        | ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::EnumPayload { .. }
        | ArtifactValueTemplate::RecordField { .. }
        | ArtifactValueTemplate::ListElement { .. }
        | ArtifactValueTemplate::ListPrefixElement { .. }
        | ArtifactValueTemplate::MapValue { .. }
        | ArtifactValueTemplate::LoopElement { .. }
        | ArtifactValueTemplate::Equality { .. }
        | ArtifactValueTemplate::ScalarOrdering { .. }
        | ArtifactValueTemplate::IfElse { .. }
        | ArtifactValueTemplate::BooleanNot { .. }
        | ArtifactValueTemplate::BooleanBinary { .. } => Ok(()),
        ArtifactValueTemplate::ListRest { .. }
        | ArtifactValueTemplate::MapRest { .. }
        | ArtifactValueTemplate::ProcessRef { .. }
        | ArtifactValueTemplate::EnumVariant { .. }
        | ArtifactValueTemplate::ScalarArithmetic { .. }
        | ArtifactValueTemplate::Record { .. }
        | ArtifactValueTemplate::List { .. }
        | ArtifactValueTemplate::Map { .. } => Err(Error::new(format!(
            "{field} must evaluate to unit Bool value False or True"
        ))),
    }
}

fn validate_static_bool_condition_value(
    artifact: &MantleArtifact,
    field: &str,
    condition: &ArtifactValueTemplate,
    current_state_payload: Option<&ArtifactPayload>,
) -> Result<()> {
    if condition.depends_on_received_payload() || condition.depends_on_loop_element() {
        return Ok(());
    }

    let value =
        artifact.evaluate_state_value_with_current_state(condition, None, current_state_payload)?;
    validate_bool_atom_value(field, &value.value)
}

fn validate_bool_atom_value(field: &str, value: &ArtifactValue) -> Result<()> {
    match value {
        ArtifactValue::Atom(label) if label == "False" || label == "True" => Ok(()),
        _ => Err(Error::new(format!(
            "{field} must evaluate to unit Bool value False or True"
        ))),
    }
}

pub(in crate::artifact) fn transition_payload_guard_key(
    payload_guard: &Option<ArtifactPayload>,
) -> TransitionPayloadGuardKey {
    payload_guard
        .as_ref()
        .map(|payload| (payload.ty.as_u32(), payload.value.clone()))
}

pub(in crate::artifact) fn transition_payload_guard_label(
    payload_guard: &Option<ArtifactPayload>,
) -> String {
    payload_guard
        .as_ref()
        .map(ArtifactPayload::label)
        .unwrap_or_else(|| "<none>".to_string())
}
