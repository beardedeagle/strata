use super::admission::LoadedTemplateAdmission;
use super::dependencies::{
    loaded_template_depends_on_effect_outcome, loaded_template_depends_on_loop_element,
    loaded_template_depends_on_received_payload,
};
use super::support::*;

#[derive(Clone, Copy)]
pub(in crate::program) struct LoadedBoolConditionAdmission<'a> {
    pub(in crate::program) program: &'a LoadedProgram,
    pub(in crate::program) process: &'a LoadedProcess,
    pub(in crate::program) field: &'a str,
    pub(in crate::program) received_payload_type: Option<TypeId>,
    pub(in crate::program) current_state_payload: Option<&'a RuntimePayload>,
    pub(in crate::program) loop_elements: &'a [LoadedLoopElement],
    pub(in crate::program) effect_outcomes: &'a [(EffectOutcomeId, TypeId)],
}

pub(in crate::program) fn evaluate_loaded_state_value(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
    received_payload: Option<&RuntimePayload>,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<LoadedStateValue> {
    let payload =
        evaluate_loaded_payload_value(program, template, received_payload, current_state_payload)?;
    Ok(LoadedStateValue::from_payload(payload))
}

pub(in crate::program) fn validate_loaded_bool_condition(
    program: &LoadedProgram,
    process: &LoadedProcess,
    field: &str,
    condition: &LoadedValueTemplate,
    received_payload_type: Option<TypeId>,
    current_state_payload: Option<&RuntimePayload>,
    effect_outcomes: &[(EffectOutcomeId, TypeId)],
) -> Result<()> {
    validate_loaded_bool_condition_with_loop_elements(
        LoadedBoolConditionAdmission {
            program,
            process,
            field,
            received_payload_type,
            current_state_payload,
            loop_elements: &[],
            effect_outcomes,
        },
        condition,
    )
}

pub(in crate::program) fn validate_loaded_bool_condition_with_loop_elements(
    admission: LoadedBoolConditionAdmission<'_>,
    condition: &LoadedValueTemplate,
) -> Result<()> {
    let bool_type = condition.result_type();
    let ty = admission.program.type_entry(bool_type)?;
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
            "{} must have type enum Bool {{ False, True }}",
            admission.field
        )));
    }
    validate_loaded_bool_condition_shape(admission.field, condition)?;
    LoadedTemplateAdmission {
        expected_type: Some(bool_type),
        received_payload_type: admission.received_payload_type,
        current_state_payload_type: admission.current_state_payload.map(|payload| payload.ty),
        allow_direct_process_ref: false,
        allow_process_ref_effect_outcome: false,
        loop_elements: admission.loop_elements,
        effect_outcomes: admission.effect_outcomes,
        program: admission.program,
        process: admission.process,
        spawned_refs: &[],
    }
    .validate(admission.field, condition)?;
    validate_loaded_static_bool_condition_value(
        admission.program,
        admission.field,
        condition,
        admission.current_state_payload,
    )
}

fn validate_loaded_bool_condition_shape(
    field: &str,
    condition: &LoadedValueTemplate,
) -> Result<()> {
    match condition {
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::EnumPayload { .. }
        | LoadedValueTemplate::RecordField { .. }
        | LoadedValueTemplate::ListElement { .. }
        | LoadedValueTemplate::ListPrefixElement { .. }
        | LoadedValueTemplate::MapValue { .. }
        | LoadedValueTemplate::LoopElement { .. }
        | LoadedValueTemplate::EffectOutcome { .. }
        | LoadedValueTemplate::Equality { .. }
        | LoadedValueTemplate::ScalarOrdering { .. }
        | LoadedValueTemplate::IfElse { .. }
        | LoadedValueTemplate::BooleanNot { .. }
        | LoadedValueTemplate::BooleanBinary { .. } => Ok(()),
        LoadedValueTemplate::ListRest { .. }
        | LoadedValueTemplate::MapRest { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::EnumVariant { .. }
        | LoadedValueTemplate::Record { .. }
        | LoadedValueTemplate::List { .. }
        | LoadedValueTemplate::Map { .. }
        | LoadedValueTemplate::ScalarArithmetic { .. } => Err(Error::new(format!(
            "{field} must evaluate to unit Bool value False or True"
        ))),
    }
}

fn validate_loaded_static_bool_condition_value(
    program: &LoadedProgram,
    field: &str,
    condition: &LoadedValueTemplate,
    current_state_payload: Option<&RuntimePayload>,
) -> Result<()> {
    if loaded_template_depends_on_received_payload(condition)
        || loaded_template_depends_on_loop_element(condition)
        || loaded_template_depends_on_effect_outcome(condition)
    {
        return Ok(());
    }

    let value = evaluate_loaded_payload_value(program, condition, None, current_state_payload)?;
    validate_loaded_bool_atom_value(field, &value.value)
}

fn validate_loaded_bool_atom_value(field: &str, value: &RuntimeValue) -> Result<()> {
    match value {
        RuntimeValue::Atom(label) if label == "False" || label == "True" => Ok(()),
        _ => Err(Error::new(format!(
            "{field} must evaluate to unit Bool value False or True"
        ))),
    }
}

fn evaluate_loaded_payload_value(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
    received_payload: Option<&RuntimePayload>,
    current_state_payload: Option<&RuntimePayload>,
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
            let payload = current_state_payload.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            program.validate_runtime_payload_matches_type("current state payload", *ty, payload)?;
            Ok(payload.clone())
        }
        LoadedValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_loaded_payload_value(
                program,
                value,
                received_payload,
                current_state_payload,
            )?;
            let variant = program.enum_variant_label(value.ty, *variant)?;
            program.runtime_payload_value(
                "enum payload projection value",
                *ty,
                value.value.project_enum_payload(variant)?,
            )
        }
        LoadedValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_loaded_payload_value(
                program,
                record,
                received_payload,
                current_state_payload,
            )?;
            let field_name = program.record_field_name(record.ty, *field)?;
            program.runtime_payload_value(
                "record field projection value",
                *ty,
                record.value.project_record_field(field_name)?,
            )
        }
        LoadedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_loaded_payload_value(
                program,
                list,
                received_payload,
                current_state_payload,
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
            let list = evaluate_loaded_payload_value(
                program,
                list,
                received_payload,
                current_state_payload,
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
            let list = evaluate_loaded_payload_value(
                program,
                list,
                received_payload,
                current_state_payload,
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
            let map = evaluate_loaded_payload_value(
                program,
                map,
                received_payload,
                current_state_payload,
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
            let map = evaluate_loaded_payload_value(
                program,
                map,
                received_payload,
                current_state_payload,
            )?;
            program.runtime_payload_value(
                "map rest projection value",
                *ty,
                map.value.project_map_rest(excluded_keys)?,
            )
        }
        LoadedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference template requires runtime process reference bindings",
        )),
        LoadedValueTemplate::LoopElement { .. } => Err(Error::new(
            "loop element template requires runtime loop element bindings",
        )),
        LoadedValueTemplate::EffectOutcome { .. } => Err(Error::new(
            "effect outcome template requires runtime effect outcome bindings",
        )),
        LoadedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_loaded_payload_value(
                program,
                payload,
                received_payload,
                current_state_payload,
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
            let mut values = Vec::with_capacity(fields.len());
            for (index, field) in fields.iter().enumerate() {
                let value = evaluate_loaded_payload_value(
                    program,
                    &field.value,
                    received_payload,
                    current_state_payload,
                )?;
                if fields[..index]
                    .iter()
                    .any(|previous| previous.field == field.field)
                {
                    return Err(Error::new(format!(
                        "record template duplicates field id {}",
                        field.field.as_u32()
                    )));
                }
                let field_name = program.record_field_name(*ty, field.field)?;
                values.push(ArtifactRecordField {
                    name: field_name.to_string(),
                    value: value.value,
                });
            }
            program.runtime_payload_value(
                "record template value",
                *ty,
                RuntimeValue::Record {
                    constructor: program.type_label(*ty)?.to_string(),
                    fields: values,
                },
            )
        }
        LoadedValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(
                    evaluate_loaded_payload_value(
                        program,
                        item,
                        received_payload,
                        current_state_payload,
                    )?
                    .value,
                );
            }
            program.runtime_payload_value("list template value", *ty, RuntimeValue::List(values))
        }
        LoadedValueTemplate::Map { ty, entries } => {
            let mut values: Vec<ArtifactMapEntry> = Vec::with_capacity(entries.len());
            for entry in entries {
                let key = evaluate_loaded_payload_value(
                    program,
                    &entry.key,
                    received_payload,
                    current_state_payload,
                )?;
                let value = evaluate_loaded_payload_value(
                    program,
                    &entry.value,
                    received_payload,
                    current_state_payload,
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
            let condition = evaluate_loaded_payload_value(
                program,
                condition,
                received_payload,
                current_state_payload,
            )?;
            let selected = if runtime_bool_value(&condition.value)? {
                then_value
            } else {
                else_value
            };
            let value = evaluate_loaded_payload_value(
                program,
                selected,
                received_payload,
                current_state_payload,
            )?;
            if value.ty != *ty {
                return Err(Error::new(format!(
                    "if expression branch produced type id {}, expected {}",
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
            let left = evaluate_loaded_payload_value(
                program,
                left,
                received_payload,
                current_state_payload,
            )?;
            if left.ty != *operand_ty {
                return Err(Error::new(format!(
                    "equality left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let right = evaluate_loaded_payload_value(
                program,
                right,
                received_payload,
                current_state_payload,
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
            let left = evaluate_loaded_payload_value(
                program,
                left,
                received_payload,
                current_state_payload,
            )?;
            if left.ty != *ty {
                return Err(Error::new(format!(
                    "scalar arithmetic left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            let right = evaluate_loaded_payload_value(
                program,
                right,
                received_payload,
                current_state_payload,
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
            let left = evaluate_loaded_payload_value(
                program,
                left,
                received_payload,
                current_state_payload,
            )?;
            if left.ty != *operand_ty {
                return Err(Error::new(format!(
                    "scalar ordering left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let right = evaluate_loaded_payload_value(
                program,
                right,
                received_payload,
                current_state_payload,
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
            let value = evaluate_loaded_payload_value(
                program,
                operand,
                received_payload,
                current_state_payload,
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
            let left = evaluate_loaded_payload_value(
                program,
                left,
                received_payload,
                current_state_payload,
            )?;
            let left = runtime_bool_value(&left.value)?;
            let selected = match operator {
                ArtifactValueBooleanOperator::And => {
                    left && runtime_bool_value(
                        &evaluate_loaded_payload_value(
                            program,
                            right,
                            received_payload,
                            current_state_payload,
                        )?
                        .value,
                    )?
                }
                ArtifactValueBooleanOperator::Or => {
                    left || runtime_bool_value(
                        &evaluate_loaded_payload_value(
                            program,
                            right,
                            received_payload,
                            current_state_payload,
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
