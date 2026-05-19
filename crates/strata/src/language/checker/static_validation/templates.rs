use std::collections::BTreeSet;

use mantle_artifact::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactValue, MAX_VALUE_TEMPLATE_FIELDS,
};

use super::process_refs::{
    message_payload_type, process_ref_target, validate_process_ref_type_target,
};
use crate::language::checked::{
    CheckedMessageId, CheckedNextState, CheckedPayloadValue, CheckedProcess, CheckedProcessRefId,
    CheckedStateId, CheckedTypeKind, CheckedTypeRef, CheckedValueBooleanOperator,
    CheckedValueEqualityOperator, CheckedValueShape, CheckedValueTemplate,
    CheckedValueTemplateField, CheckedValueTemplateMapEntry,
};
use crate::language::diagnostic::{Error, Result};

mod evaluation;
mod payload_labels;
mod process_refs;

pub(super) use evaluation::{
    checked_template_depends_on_loop_element, checked_template_depends_on_received_payload,
    evaluate_checked_template, resolve_checked_next_state, resolve_checked_template_state,
};
pub(super) use payload_labels::validate_value_template_payload_labels;
pub(super) use process_refs::{
    reject_process_ref_template_in_next_state, validate_value_template_process_refs,
};

pub(super) fn validate_next_state(
    process: &CheckedProcess,
    current_message: CheckedMessageId,
    current_state: Option<CheckedStateId>,
    next_state: &CheckedNextState,
) -> Result<()> {
    match next_state {
        CheckedNextState::Current => Ok(()),
        CheckedNextState::Value(state) => {
            if state.index() >= process.state_values().len() {
                return Err(Error::new(format!(
                    "process {} next_state id {} is not a valid state value",
                    process.debug_name(),
                    state.as_u32()
                )));
            }
            Ok(())
        }
        CheckedNextState::Template(template) => {
            if template.result_type() != process.state_type() {
                return Err(Error::new(format!(
                    "process {} next_state template has type {}, expected {}",
                    process.debug_name(),
                    template.result_type(),
                    process.state_type()
                )));
            }
            validate_value_template_binding_types(
                template,
                message_payload_type(process, current_message)?,
                current_state_payload_type(process, current_state)?,
            )?;
            validate_value_template_payload_labels(template)?;
            reject_process_ref_template_in_next_state(template)?;
            if !checked_template_depends_on_received_payload(template) {
                resolve_checked_template_state(
                    process,
                    template,
                    None,
                    current_state
                        .and_then(|state| process.state_values().get(state.index()))
                        .and_then(|state| state.payload()),
                )?;
            }
            Ok(())
        }
        CheckedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            validate_bool_condition_template(process, condition)?;
            validate_value_template_binding_types(
                condition,
                message_payload_type(process, current_message)?,
                current_state_payload_type(process, current_state)?,
            )?;
            validate_value_template_payload_labels(condition)?;
            reject_process_ref_template_in_next_state(condition)?;
            validate_static_bool_condition_value(
                process,
                condition,
                current_state_payload(process, current_state)?,
            )?;
            validate_next_state(process, current_message, current_state, then_state)?;
            validate_next_state(process, current_message, current_state, else_state)
        }
    }
}

pub(super) fn validate_bool_condition_template(
    process: &CheckedProcess,
    condition: &CheckedValueTemplate,
) -> Result<()> {
    let CheckedTypeKind::Value {
        shape: CheckedValueShape::Enum { variants },
    } = condition.result_type().kind()
    else {
        return Err(Error::new(format!(
            "process {} if condition requires enum Bool {{ False, True }}",
            process.debug_name()
        )));
    };
    let is_bool_contract = variants.len() == 2
        && variants[0].name.as_str() == "False"
        && variants[0].payload_type.is_none()
        && variants[1].name.as_str() == "True"
        && variants[1].payload_type.is_none();
    if !is_bool_contract {
        return Err(Error::new(format!(
            "process {} if condition requires enum Bool {{ False, True }}",
            process.debug_name()
        )));
    }
    validate_bool_condition_template_shape(process, condition)
}

pub(super) fn validate_static_bool_condition_value(
    process: &CheckedProcess,
    condition: &CheckedValueTemplate,
    current_state_payload: Option<&CheckedPayloadValue>,
) -> Result<()> {
    if checked_template_depends_on_received_payload(condition)
        || checked_template_depends_on_loop_element(condition)
    {
        return Ok(());
    }

    let value = evaluate_checked_template(condition, None, current_state_payload)?;
    let Some(value) = value.value() else {
        return Err(Error::new(format!(
            "process {} if condition produced a process reference payload",
            process.debug_name()
        )));
    };
    match value {
        ArtifactValue::Atom(label) if label == "False" || label == "True" => Ok(()),
        _ => Err(Error::new(format!(
            "process {} if condition must evaluate to unit Bool value False or True",
            process.debug_name()
        ))),
    }
}

fn validate_bool_condition_template_shape(
    process: &CheckedProcess,
    condition: &CheckedValueTemplate,
) -> Result<()> {
    match condition {
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::EnumPayload { .. }
        | CheckedValueTemplate::RecordField { .. }
        | CheckedValueTemplate::ListElement { .. }
        | CheckedValueTemplate::ListPrefixElement { .. }
        | CheckedValueTemplate::MapValue { .. }
        | CheckedValueTemplate::LoopElement { .. }
        | CheckedValueTemplate::Equality { .. }
        | CheckedValueTemplate::BooleanNot { .. }
        | CheckedValueTemplate::BooleanBinary { .. } => Ok(()),
        CheckedValueTemplate::ListRest { .. }
        | CheckedValueTemplate::MapRest { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::EnumVariant { .. }
        | CheckedValueTemplate::Record { .. }
        | CheckedValueTemplate::List { .. }
        | CheckedValueTemplate::Map { .. } => Err(Error::new(format!(
            "process {} if condition must evaluate to unit Bool value False or True",
            process.debug_name()
        ))),
    }
}

pub(super) fn current_state_payload_type(
    process: &CheckedProcess,
    current_state: Option<CheckedStateId>,
) -> Result<Option<&CheckedTypeRef>> {
    let Some(current_state) = current_state else {
        return Ok(None);
    };
    let state = process
        .state_values()
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} current_state id {} is not a valid state value",
                process.debug_name(),
                current_state.as_u32()
            ))
        })?;
    Ok(state.payload().map(CheckedPayloadValue::ty))
}

fn current_state_payload(
    process: &CheckedProcess,
    current_state: Option<CheckedStateId>,
) -> Result<Option<&CheckedPayloadValue>> {
    let Some(current_state) = current_state else {
        return Ok(None);
    };
    let state = process
        .state_values()
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} current_state id {} is not a valid state value",
                process.debug_name(),
                current_state.as_u32()
            ))
        })?;
    Ok(state.payload())
}

pub(super) fn validate_value_template_binding_types(
    template: &CheckedValueTemplate,
    received_payload_type: Option<&CheckedTypeRef>,
    current_state_payload_type: Option<&CheckedTypeRef>,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_) => Ok(()),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            let Some(received_payload_type) = received_payload_type else {
                return Err(Error::new(
                    "received payload template requires a payload-bearing message",
                ));
            };
            if ty != received_payload_type {
                return Err(Error::new(format!(
                    "received payload template has type {}, expected {}",
                    ty, received_payload_type
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            let Some(current_state_payload_type) = current_state_payload_type else {
                return Err(Error::new(
                    "current state payload template requires a payload-bearing state",
                ));
            };
            if ty != current_state_payload_type {
                return Err(Error::new(format!(
                    "current state payload template has type {}, expected {}",
                    ty, current_state_payload_type
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::LoopElement { .. } => Ok(()),
        CheckedValueTemplate::EnumPayload { ty, value, variant } => {
            validate_checked_enum_payload_projection(ty, value.result_type(), *variant)?;
            validate_value_template_binding_types(
                value,
                received_payload_type,
                current_state_payload_type,
            )
        }
        CheckedValueTemplate::RecordField { record, .. } => validate_value_template_binding_types(
            record,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::ListElement { list, .. } => validate_value_template_binding_types(
            list,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::ListPrefixElement { list, .. } => {
            validate_value_template_binding_types(
                list,
                received_payload_type,
                current_state_payload_type,
            )
        }
        CheckedValueTemplate::ListRest { list, .. } => validate_value_template_binding_types(
            list,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::MapValue { map, .. } => validate_value_template_binding_types(
            map,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::MapRest { map, .. } => validate_value_template_binding_types(
            map,
            received_payload_type,
            current_state_payload_type,
        ),
        CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            validate_checked_enum_variant_payload(ty, *variant, payload.result_type())?;
            validate_value_template_binding_types(
                payload,
                received_payload_type,
                current_state_payload_type,
            )
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_binding_types(
                    field.value(),
                    received_payload_type,
                    current_state_payload_type,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                validate_value_template_binding_types(
                    item,
                    received_payload_type,
                    current_state_payload_type,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                validate_value_template_binding_types(
                    entry.key(),
                    received_payload_type,
                    current_state_payload_type,
                )?;
                validate_value_template_binding_types(
                    entry.value(),
                    received_payload_type,
                    current_state_payload_type,
                )?;
            }
            Ok(())
        }
        CheckedValueTemplate::Equality {
            ty,
            operand_ty,
            left,
            right,
            ..
        } => {
            validate_checked_equality_template(ty, operand_ty, left, right)?;
            validate_value_template_binding_types(
                left,
                received_payload_type,
                current_state_payload_type,
            )?;
            validate_value_template_binding_types(
                right,
                received_payload_type,
                current_state_payload_type,
            )
        }
        CheckedValueTemplate::BooleanNot { ty, operand } => {
            validate_checked_bool_composition_operand(ty, operand)?;
            validate_value_template_binding_types(
                operand,
                received_payload_type,
                current_state_payload_type,
            )
        }
        CheckedValueTemplate::BooleanBinary {
            ty, left, right, ..
        } => {
            validate_checked_bool_composition_operand(ty, left)?;
            validate_checked_bool_composition_operand(ty, right)?;
            validate_value_template_binding_types(
                left,
                received_payload_type,
                current_state_payload_type,
            )?;
            validate_value_template_binding_types(
                right,
                received_payload_type,
                current_state_payload_type,
            )
        }
    }
}

fn validate_checked_equality_template(
    result_ty: &CheckedTypeRef,
    operand_ty: &CheckedTypeRef,
    left: &CheckedValueTemplate,
    right: &CheckedValueTemplate,
) -> Result<()> {
    validate_checked_bool_contract_type(result_ty)?;
    validate_checked_equality_operand_type(operand_ty)?;
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
    Ok(())
}

fn validate_checked_bool_composition_operand(
    result_ty: &CheckedTypeRef,
    operand: &CheckedValueTemplate,
) -> Result<()> {
    validate_checked_bool_contract_type(result_ty)?;
    if operand.result_type() != result_ty {
        return Err(Error::new(format!(
            "boolean predicate operand has type {}, expected {}",
            operand.result_type(),
            result_ty
        )));
    }
    Ok(())
}

fn validate_checked_bool_contract_type(ty: &CheckedTypeRef) -> Result<()> {
    if matches!(
        ty.kind(),
        CheckedTypeKind::Value { shape } if is_checked_bool_contract_shape(shape)
    ) {
        return Ok(());
    }
    Err(Error::new(format!(
        "equality result type must be enum Bool {{ False, True }}, found {ty}"
    )))
}

fn is_checked_bool_contract_shape(shape: &CheckedValueShape) -> bool {
    matches!(
        shape,
        CheckedValueShape::Enum { variants }
            if variants.len() == 2
                && variants[0].name.as_str() == "False"
                && variants[0].payload_type.is_none()
                && variants[1].name.as_str() == "True"
                && variants[1].payload_type.is_none()
    )
}

fn validate_checked_equality_operand_type(operand_ty: &CheckedTypeRef) -> Result<()> {
    match operand_ty.kind() {
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum { variants },
        } if variants
            .iter()
            .all(|variant| variant.payload_type.is_none()) =>
        {
            Ok(())
        }
        _ => Err(Error::new(format!(
            "equality operands must be Bool or fieldless enum values, found {operand_ty}"
        ))),
    }
}

fn validate_checked_enum_payload_projection(
    projected_ty: &CheckedTypeRef,
    enum_ty: &CheckedTypeRef,
    variant: crate::language::checked::CheckedEnumVariantId,
) -> Result<()> {
    let payload_type = enum_ty.enum_variant_payload_type(variant)?;
    match payload_type {
        Some(expected) if expected == projected_ty.id() => Ok(()),
        Some(expected) => Err(Error::new(format!(
            "enum payload projection has type {}, expected checked type id {} from {} variant id {}",
            projected_ty,
            expected.as_u32(),
            enum_ty,
            variant.as_u32()
        ))),
        None => Err(Error::new(format!(
            "enum payload projection requires payload-bearing variant id {} of {}",
            variant.as_u32(),
            enum_ty
        ))),
    }
}

fn validate_checked_enum_variant_payload(
    enum_ty: &CheckedTypeRef,
    variant: crate::language::checked::CheckedEnumVariantId,
    payload_ty: &CheckedTypeRef,
) -> Result<()> {
    let expected = enum_ty.enum_variant_payload_type(variant)?;
    match expected {
        Some(expected) if expected == payload_ty.id() => Ok(()),
        Some(expected) => Err(Error::new(format!(
            "enum variant template payload has type {}, expected checked type id {} for {} variant id {}",
            payload_ty,
            expected.as_u32(),
            enum_ty,
            variant.as_u32()
        ))),
        None => Err(Error::new(format!(
            "enum variant template requires payload-bearing variant id {} of {}",
            variant.as_u32(),
            enum_ty
        ))),
    }
}
