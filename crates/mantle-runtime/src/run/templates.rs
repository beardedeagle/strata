use mantle_artifact::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactScalarValue, ArtifactValueBooleanOperator,
    ArtifactValueEqualityOperator, Error, Result,
};

use super::model::ActiveStep;
use super::process_refs::LocalProcessRefs;
use super::{RuntimeEffectOutcome, RuntimeLoopElement};
use crate::program::{LoadedProgram, LoadedValueTemplate, RuntimePayload, RuntimeValue};

pub(super) fn evaluate_runtime_template(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
    received_payload: Option<&RuntimePayload>,
    step: &ActiveStep,
    process_refs: &LocalProcessRefs,
    loop_elements: &[RuntimeLoopElement<'_>],
    effect_outcomes: &[RuntimeEffectOutcome],
) -> Result<RuntimePayload> {
    match template {
        LoadedValueTemplate::Literal { ty, value } => {
            program.runtime_payload_value("literal value template", *ty, value.clone())
        }
        LoadedValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            program.validate_runtime_payload_matches_type("received payload", *ty, payload)?;
            Ok(payload.clone())
        }
        LoadedValueTemplate::CurrentStatePayload { ty } => {
            let payload = step.current_state_payload(program)?.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            program.validate_runtime_payload_matches_type("current state payload", *ty, payload)?;
            Ok(payload.clone())
        }
        LoadedValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_runtime_template(
                program,
                value,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let variant = program.enum_variant_label(value.ty, *variant)?;
            program.runtime_payload_value(
                "enum payload projection value",
                *ty,
                value.value.project_enum_payload(variant)?,
            )
        }
        LoadedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_runtime_template(
                program,
                record,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "record field projection value",
                *ty,
                record.value.project_record_field(field)?,
            )
        }
        LoadedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_runtime_template(
                program,
                list,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "list element projection value",
                *ty,
                list.value.project_list_element(*index, *len)?,
            )
        }
        LoadedValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            let list = evaluate_runtime_template(
                program,
                list,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "list prefix projection value",
                *ty,
                list.value
                    .project_list_prefix_element(*index, *prefix_len)?,
            )
        }
        LoadedValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            let list = evaluate_runtime_template(
                program,
                list,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "list rest projection value",
                *ty,
                list.value.project_list_rest(*prefix_len)?,
            )
        }
        LoadedValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let map = evaluate_runtime_template(
                program,
                map,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "map value projection value",
                *ty,
                map.value.project_map_value(key, keys, *projection)?,
            )
        }
        LoadedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            let map = evaluate_runtime_template(
                program,
                map,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "map rest projection value",
                *ty,
                map.value.project_map_rest(excluded_keys)?,
            )
        }
        LoadedValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            let pid = process_refs.get(*process_ref).ok_or_else(|| {
                Error::new(format!(
                    "process {} sends unbound process reference id {} as payload",
                    step.process_name,
                    process_ref.as_u32()
                ))
            })?;
            RuntimePayload::from_process_ref(*ty, *target_process, pid)
        }
        LoadedValueTemplate::LoopElement { ty, element } => {
            let payload = loop_elements
                .iter()
                .find(|binding| binding.id == *element)
                .map(|binding| binding.payload)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} references inactive loop element id {}",
                        step.process_name,
                        element.as_u32()
                    ))
                })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "loop element id {} has type id {}, expected {}",
                    element.as_u32(),
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        LoadedValueTemplate::EffectOutcome { ty, outcome } => {
            let payload = effect_outcomes
                .iter()
                .find(|binding| binding.id == *outcome)
                .map(|binding| &binding.payload)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} references unbound effect outcome id {}",
                        step.process_name,
                        outcome.as_u32()
                    ))
                })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "effect outcome id {} has type id {}, expected {}",
                    outcome.as_u32(),
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        LoadedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_runtime_template(
                program,
                payload,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "enum variant template value",
                *ty,
                RuntimeValue::EnumVariant {
                    variant: program.enum_variant_label(*ty, *variant)?.to_string(),
                    payload: Box::new(payload.value),
                },
            )
        }
        LoadedValueTemplate::Record { ty, fields } => {
            let type_label = program.type_label(*ty)?;
            let mut values = Vec::with_capacity(fields.len());
            for (field_index, field) in fields.iter().enumerate() {
                let value = evaluate_runtime_template(
                    program,
                    &field.value,
                    received_payload,
                    step,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                if fields[..field_index]
                    .iter()
                    .any(|previous| previous.name == field.name)
                {
                    return Err(Error::new(format!(
                        "record template duplicates field {}",
                        field.name
                    )));
                }
                values.push(ArtifactRecordField {
                    name: field.name.clone(),
                    value: value.value,
                });
            }
            program.runtime_payload_value(
                "record template value",
                *ty,
                RuntimeValue::Record {
                    constructor: type_label.to_string(),
                    fields: values,
                },
            )
        }
        LoadedValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value = evaluate_runtime_template(
                    program,
                    item,
                    received_payload,
                    step,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                values.push(value.value);
            }
            program.runtime_payload_value("list template value", *ty, RuntimeValue::List(values))
        }
        LoadedValueTemplate::Map { ty, entries } => {
            let mut values: Vec<ArtifactMapEntry> = Vec::with_capacity(entries.len());
            for entry in entries {
                let key = evaluate_runtime_template(
                    program,
                    &entry.key,
                    received_payload,
                    step,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                let value = evaluate_runtime_template(
                    program,
                    &entry.value,
                    received_payload,
                    step,
                    process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                if values.iter().any(|previous| previous.key == key.value) {
                    return Err(Error::new(format!(
                        "map template duplicates key {}",
                        key.value.label()
                    )));
                }
                values.push(ArtifactMapEntry {
                    key: key.value,
                    value: value.value,
                });
            }
            program.runtime_payload_value("map template value", *ty, RuntimeValue::Map(values))
        }
        LoadedValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => {
            let condition = evaluate_runtime_template(
                program,
                condition,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let selected = if runtime_bool_value(&condition.value)? {
                then_value
            } else {
                else_value
            };
            let value = evaluate_runtime_template(
                program,
                selected,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if value.ty != *ty {
                return Err(Error::new(format!(
                    "if_else value branch has type id {}, expected {}",
                    value.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(value)
        }
        LoadedValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(
                program,
                left,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if left.ty != *operand_ty {
                return Err(Error::new(format!(
                    "equality left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let right = evaluate_runtime_template(
                program,
                right,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if right.ty != *operand_ty {
                return Err(Error::new(format!(
                    "equality right operand has type id {}, expected {}",
                    right.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let is_equal = left.value == right.value;
            let selected = match operator {
                ArtifactValueEqualityOperator::Equal => is_equal,
                ArtifactValueEqualityOperator::NotEqual => !is_equal,
            };
            program.runtime_payload_value(
                "equality template value",
                *ty,
                RuntimeValue::Atom(bool_atom(selected)),
            )
        }
        LoadedValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(
                program,
                left,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if left.ty != *ty {
                return Err(Error::new(format!(
                    "scalar arithmetic left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            let right = evaluate_runtime_template(
                program,
                right,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if right.ty != *ty {
                return Err(Error::new(format!(
                    "scalar arithmetic right operand has type id {}, expected {}",
                    right.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            let (RuntimeValue::Scalar(left), RuntimeValue::Scalar(right)) =
                (left.value, right.value)
            else {
                return Err(Error::new(
                    "scalar arithmetic operands must produce scalar values",
                ));
            };
            program.runtime_payload_value(
                "scalar arithmetic template value",
                *ty,
                RuntimeValue::Scalar(ArtifactScalarValue::checked_arithmetic(
                    *operator, left, right,
                )?),
            )
        }
        LoadedValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(
                program,
                left,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if left.ty != *operand_ty {
                return Err(Error::new(format!(
                    "scalar ordering left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let right = evaluate_runtime_template(
                program,
                right,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            if right.ty != *operand_ty {
                return Err(Error::new(format!(
                    "scalar ordering right operand has type id {}, expected {}",
                    right.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let (RuntimeValue::Scalar(left), RuntimeValue::Scalar(right)) =
                (left.value, right.value)
            else {
                return Err(Error::new(
                    "scalar ordering operands must produce scalar values",
                ));
            };
            program.runtime_payload_value(
                "scalar ordering template value",
                *ty,
                RuntimeValue::Atom(bool_atom(ArtifactScalarValue::compare(
                    *operator, left, right,
                )?)),
            )
        }
        LoadedValueTemplate::BooleanNot { ty, operand } => {
            let value = evaluate_runtime_template(
                program,
                operand,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            program.runtime_payload_value(
                "boolean predicate template value",
                *ty,
                RuntimeValue::Atom(bool_atom(!runtime_bool_value(&value.value)?)),
            )
        }
        LoadedValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(
                program,
                left,
                received_payload,
                step,
                process_refs,
                loop_elements,
                effect_outcomes,
            )?;
            let left = runtime_bool_value(&left.value)?;
            let selected = match operator {
                ArtifactValueBooleanOperator::And => {
                    left && runtime_bool_value(
                        &evaluate_runtime_template(
                            program,
                            right,
                            received_payload,
                            step,
                            process_refs,
                            loop_elements,
                            effect_outcomes,
                        )?
                        .value,
                    )?
                }
                ArtifactValueBooleanOperator::Or => {
                    left || runtime_bool_value(
                        &evaluate_runtime_template(
                            program,
                            right,
                            received_payload,
                            step,
                            process_refs,
                            loop_elements,
                            effect_outcomes,
                        )?
                        .value,
                    )?
                }
            };
            program.runtime_payload_value(
                "boolean predicate template value",
                *ty,
                RuntimeValue::Atom(bool_atom(selected)),
            )
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

fn runtime_bool_value(value: &RuntimeValue) -> Result<bool> {
    match value {
        RuntimeValue::Atom(label) if label == "True" => Ok(true),
        RuntimeValue::Atom(label) if label == "False" => Ok(false),
        _ => Err(Error::new(format!(
            "boolean predicate operand produced non-Bool value {}",
            value.label()
        ))),
    }
}
