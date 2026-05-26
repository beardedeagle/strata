use std::collections::BTreeMap;

use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField, ArtifactValue};

use super::super::super::super::checked::{
    CheckedEffectOutcomeId, CheckedPayloadValue, CheckedProcess, CheckedProcessRefId,
    CheckedScalarArithmeticOperator, CheckedScalarOrderingOperator, CheckedTypeRef,
    CheckedValueBooleanOperator, CheckedValueEqualityOperator, CheckedValueTemplate,
};
use super::super::super::super::diagnostic::{Error, Result};
use super::{
    StaticEffectOutcomeBinding, StaticLoopElementBinding, StaticProcessId,
    resolve_static_process_ref,
};

pub(super) fn evaluate_checked_runtime_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    loop_elements: &[StaticLoopElementBinding],
    effect_outcomes: &[StaticEffectOutcomeBinding],
) -> Result<CheckedPayloadValue> {
    match template {
        CheckedValueTemplate::Literal(value) => Ok(value.clone()),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "received payload has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            Ok(payload.clone())
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            let payload = current_state_payload.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "current state payload has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            Ok(payload.clone())
        }
        CheckedValueTemplate::EffectOutcome { ty, outcome } => {
            resolve_static_effect_outcome(process, *outcome, ty, effect_outcomes)
        }
        CheckedValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_checked_runtime_template(
                value,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let variant = value.ty().enum_variant_label(*variant)?;
            let payload = checked_payload_value(&value)?
                .project_enum_payload(variant.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), payload))
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_checked_runtime_template(
                record,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let value = checked_payload_value(&record)?
                .project_record_field(field.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let value = checked_payload_value(&list)?
                .project_list_element(*index, *len)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let value = checked_payload_value(&list)?
                .project_list_prefix_element(*index, *prefix_len)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            let list = evaluate_checked_runtime_template(
                list,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let value = checked_payload_value(&list)?
                .project_list_rest(*prefix_len)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let map = evaluate_checked_runtime_template(
                map,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let value = checked_payload_value(&map)?
                .project_map_value(key, keys, *projection)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            let map = evaluate_checked_runtime_template(
                map,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let value = checked_payload_value(&map)?
                .project_map_rest(excluded_keys)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
        } => {
            let pid = resolve_static_process_ref(process, process_refs, *process_ref)?;
            Ok(CheckedPayloadValue::process_ref(
                ty.clone(),
                format!("{ty}#{}", pid.as_u32()),
                *target,
                u64::from(pid.as_u32()),
            ))
        }
        CheckedValueTemplate::LoopElement { ty, element } => {
            let value = loop_elements
                .iter()
                .find(|binding| binding.id == *element)
                .map(|binding| &binding.value)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} references inactive loop element id {}",
                        process.debug_name(),
                        element.as_u32()
                    ))
                })?;
            if value.ty() != ty {
                return Err(Error::new(format!(
                    "loop element id {} has type {}, expected {}",
                    element.as_u32(),
                    value.ty(),
                    ty
                )));
            }
            Ok(value.clone())
        }
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_checked_runtime_template(
                payload,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::EnumVariant {
                    variant: ty.enum_variant_label(*variant)?.to_string(),
                    payload: Box::new(checked_payload_value(&payload)?),
                },
            ))
        }
        CheckedValueTemplate::Record { ty, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            for (index, field) in fields.iter().enumerate() {
                let value = evaluate_checked_runtime_template(
                    field.value(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                if fields[..index]
                    .iter()
                    .any(|previous| previous.name() == field.name())
                {
                    return Err(Error::new(format!(
                        "record template duplicates field {}",
                        field.name()
                    )));
                }
                values.push(ArtifactRecordField {
                    name: field.name().to_string(),
                    value: checked_payload_value(&value)?,
                });
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Record {
                    constructor: ty.label().to_string(),
                    fields: values,
                },
            ))
        }
        CheckedValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value = evaluate_checked_runtime_template(
                    item,
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                values.push(checked_payload_value(&value)?);
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::List(values),
            ))
        }
        CheckedValueTemplate::Map { ty, entries } => {
            let mut values: Vec<ArtifactMapEntry> = Vec::with_capacity(entries.len());
            for entry in entries {
                let key = evaluate_checked_runtime_template(
                    entry.key(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                let value = evaluate_checked_runtime_template(
                    entry.value(),
                    received_payload,
                    current_state_payload,
                    process,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                let key_value = checked_payload_value(&key)?;
                let item_value = checked_payload_value(&value)?;
                if values.iter().any(|previous| previous.key == key_value) {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key_value.label()
                    )));
                }
                values.push(ArtifactMapEntry {
                    key: key_value,
                    value: item_value,
                });
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Map(values),
            ))
        }
        CheckedValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => {
            let condition = evaluate_checked_runtime_template(
                condition,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let selected = if checked_runtime_bool_value(&condition)? {
                then_value
            } else {
                else_value
            };
            let value = evaluate_checked_runtime_template(
                selected,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if value.ty() != ty {
                return Err(Error::new(format!(
                    "if expression branch has type {}, expected {}",
                    value.ty(),
                    ty
                )));
            }
            Ok(value)
        }
        CheckedValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_runtime_template(
                left,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if left.ty() != operand_ty {
                return Err(Error::new(format!(
                    "equality left operand has type {}, expected {}",
                    left.ty(),
                    operand_ty
                )));
            }
            let left_value = checked_payload_value_ref(&left)?;
            let right = evaluate_checked_runtime_template(
                right,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if right.ty() != operand_ty {
                return Err(Error::new(format!(
                    "equality right operand has type {}, expected {}",
                    right.ty(),
                    operand_ty
                )));
            }
            let right_value = checked_payload_value_ref(&right)?;
            let is_equal = left_value == right_value;
            let selected = match operator {
                CheckedValueEqualityOperator::Equal => is_equal,
                CheckedValueEqualityOperator::NotEqual => !is_equal,
            };
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(selected)),
            ))
        }
        CheckedValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_runtime_template(
                left,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if left.ty() != ty {
                return Err(Error::new(format!(
                    "scalar arithmetic left operand has type {}, expected {}",
                    left.ty(),
                    ty
                )));
            }
            let right = evaluate_checked_runtime_template(
                right,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if right.ty() != ty {
                return Err(Error::new(format!(
                    "scalar arithmetic right operand has type {}, expected {}",
                    right.ty(),
                    ty
                )));
            }
            let (ArtifactValue::Scalar(left), ArtifactValue::Scalar(right)) = (
                checked_payload_value(&left)?,
                checked_payload_value(&right)?,
            ) else {
                return Err(Error::new(
                    "scalar arithmetic operands must produce scalar values",
                ));
            };
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Scalar(
                    mantle_artifact::ArtifactScalarValue::checked_arithmetic(
                        scalar_arithmetic_operator(*operator),
                        left,
                        right,
                    )
                    .map_err(|err| Error::new(err.to_string()))?,
                ),
            ))
        }
        CheckedValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_runtime_template(
                left,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if left.ty() != operand_ty {
                return Err(Error::new(format!(
                    "scalar ordering left operand has type {}, expected {}",
                    left.ty(),
                    operand_ty
                )));
            }
            let right = evaluate_checked_runtime_template(
                right,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if right.ty() != operand_ty {
                return Err(Error::new(format!(
                    "scalar ordering right operand has type {}, expected {}",
                    right.ty(),
                    operand_ty
                )));
            }
            let (ArtifactValue::Scalar(left), ArtifactValue::Scalar(right)) = (
                checked_payload_value(&left)?,
                checked_payload_value(&right)?,
            ) else {
                return Err(Error::new(
                    "scalar ordering operands must produce scalar values",
                ));
            };
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(
                    mantle_artifact::ArtifactScalarValue::compare(
                        scalar_ordering_operator(*operator),
                        left,
                        right,
                    )
                    .map_err(|err| Error::new(err.to_string()))?,
                )),
            ))
        }
        CheckedValueTemplate::BooleanNot { ty, operand } => {
            let value = evaluate_checked_runtime_template(
                operand,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(!checked_runtime_bool_value(&value)?)),
            ))
        }
        CheckedValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_runtime_template(
                left,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let left = checked_runtime_bool_value(&left)?;
            let selected = match operator {
                CheckedValueBooleanOperator::And => {
                    left && checked_runtime_bool_value(&evaluate_checked_runtime_template(
                        right,
                        received_payload,
                        current_state_payload,
                        process,
                        process_refs,
                        loop_elements,
                        effect_outcomes,
                    )?)?
                }
                CheckedValueBooleanOperator::Or => {
                    left || checked_runtime_bool_value(&evaluate_checked_runtime_template(
                        right,
                        received_payload,
                        current_state_payload,
                        process,
                        process_refs,
                        loop_elements,
                        effect_outcomes,
                    )?)?
                }
            };
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(selected)),
            ))
        }
    }
}

fn resolve_static_effect_outcome(
    process: &CheckedProcess,
    outcome: CheckedEffectOutcomeId,
    ty: &CheckedTypeRef,
    effect_outcomes: &[StaticEffectOutcomeBinding],
) -> Result<CheckedPayloadValue> {
    let value = effect_outcomes
        .iter()
        .find(|binding| binding.id == outcome)
        .map(|binding| &binding.value)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} references unbound effect outcome id {}",
                process.debug_name(),
                outcome.as_u32()
            ))
        })?;
    if value.ty() != ty {
        return Err(Error::new(format!(
            "effect outcome id {} has type {}, expected {}",
            outcome.as_u32(),
            value.ty(),
            ty
        )));
    }
    Ok(value.clone())
}

fn scalar_arithmetic_operator(
    operator: CheckedScalarArithmeticOperator,
) -> mantle_artifact::ArtifactScalarArithmeticOperator {
    match operator {
        CheckedScalarArithmeticOperator::Add => {
            mantle_artifact::ArtifactScalarArithmeticOperator::Add
        }
        CheckedScalarArithmeticOperator::Subtract => {
            mantle_artifact::ArtifactScalarArithmeticOperator::Subtract
        }
        CheckedScalarArithmeticOperator::Multiply => {
            mantle_artifact::ArtifactScalarArithmeticOperator::Multiply
        }
        CheckedScalarArithmeticOperator::Divide => {
            mantle_artifact::ArtifactScalarArithmeticOperator::Divide
        }
        CheckedScalarArithmeticOperator::Modulo => {
            mantle_artifact::ArtifactScalarArithmeticOperator::Modulo
        }
    }
}

fn scalar_ordering_operator(
    operator: CheckedScalarOrderingOperator,
) -> mantle_artifact::ArtifactScalarOrderingOperator {
    match operator {
        CheckedScalarOrderingOperator::Less => {
            mantle_artifact::ArtifactScalarOrderingOperator::Less
        }
        CheckedScalarOrderingOperator::LessEqual => {
            mantle_artifact::ArtifactScalarOrderingOperator::LessEqual
        }
        CheckedScalarOrderingOperator::Greater => {
            mantle_artifact::ArtifactScalarOrderingOperator::Greater
        }
        CheckedScalarOrderingOperator::GreaterEqual => {
            mantle_artifact::ArtifactScalarOrderingOperator::GreaterEqual
        }
    }
}

fn bool_atom(value: bool) -> String {
    if value {
        "True".to_string()
    } else {
        "False".to_string()
    }
}

fn checked_payload_value_ref(payload: &CheckedPayloadValue) -> Result<&ArtifactValue> {
    payload
        .value()
        .ok_or_else(|| Error::new("process reference payloads are not valid state values"))
}

fn checked_runtime_bool_value(payload: &CheckedPayloadValue) -> Result<bool> {
    let value = checked_payload_value_ref(payload)?;
    match value {
        ArtifactValue::Atom(label) if label == "True" => Ok(true),
        ArtifactValue::Atom(label) if label == "False" => Ok(false),
        _ => Err(Error::new(format!(
            "boolean predicate operand produced non-Bool value {}",
            value.label()
        ))),
    }
}

pub(super) fn checked_payload_value(payload: &CheckedPayloadValue) -> Result<ArtifactValue> {
    payload
        .value()
        .cloned()
        .ok_or_else(|| Error::new("process reference payloads are not valid state values"))
}
