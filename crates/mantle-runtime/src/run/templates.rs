use mantle_artifact::{ArtifactMapEntry, ArtifactScalarValue, Error, Result};

use super::RuntimeEffectOutcome;
use super::model::{ActiveStep, RuntimeLoopElement};
use super::process_refs::LocalProcessRefs;
use crate::executable::{
    ExecutableTemplateProgram, ExecutableValueTemplate, ExecutableValueTemplateRef,
};
use crate::program::{LoadedProgram, RuntimePayload, RuntimeValue};

#[derive(Clone, Copy)]
pub(super) struct RuntimeTemplateContext<'a, 'program, 'template> {
    pub(super) program: &'program LoadedProgram,
    pub(super) templates: &'a ExecutableTemplateProgram<'template>,
    pub(super) received_payload: Option<&'a RuntimePayload>,
    pub(super) step: &'a ActiveStep,
    pub(super) process_refs: &'a LocalProcessRefs,
    pub(super) loop_elements: &'a [RuntimeLoopElement<'a>],
    pub(super) effect_outcomes: &'a [RuntimeEffectOutcome],
}

pub(super) fn evaluate_runtime_template(
    context: RuntimeTemplateContext<'_, '_, '_>,
    template: ExecutableValueTemplateRef,
) -> Result<RuntimePayload> {
    let template = context.templates.get(template)?;
    match template {
        ExecutableValueTemplate::Literal { ty, value } => {
            context
                .program
                .runtime_payload_value("literal value template", *ty, (*value).clone())
        }
        ExecutableValueTemplate::ReceivedPayload { ty } => {
            let payload = context.received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            context.program.validate_runtime_payload_matches_type(
                "received payload",
                *ty,
                payload,
            )?;
            Ok(payload.clone())
        }
        ExecutableValueTemplate::CurrentStatePayload { ty } => {
            let payload = context
                .step
                .current_state_payload(context.program)?
                .ok_or_else(|| {
                    Error::new("current state payload template requires a payload-bearing state")
                })?;
            context.program.validate_runtime_payload_matches_type(
                "current state payload",
                *ty,
                payload,
            )?;
            Ok(payload.clone())
        }
        ExecutableValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_runtime_template(context, *value)?;
            context.program.runtime_payload_value(
                "enum payload projection value",
                *ty,
                context.program.project_enum_payload_by_id(
                    "enum payload projection value",
                    value.ty,
                    *variant,
                    &value.value,
                )?,
            )
        }
        ExecutableValueTemplate::RecordField { ty, record, field } => {
            let record = evaluate_runtime_template(context, *record)?;
            context.program.runtime_payload_value(
                "record field projection value",
                *ty,
                context.program.project_record_field_by_id(
                    "record field projection value",
                    record.ty,
                    *field,
                    &record.value,
                )?,
            )
        }
        ExecutableValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            let list = evaluate_runtime_template(context, *list)?;
            context.program.runtime_payload_value(
                "list element projection value",
                *ty,
                list.value.project_list_element(*index, *len)?,
            )
        }
        ExecutableValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            let list = evaluate_runtime_template(context, *list)?;
            context.program.runtime_payload_value(
                "list prefix projection value",
                *ty,
                list.value
                    .project_list_prefix_element(*index, *prefix_len)?,
            )
        }
        ExecutableValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            let list = evaluate_runtime_template(context, *list)?;
            context.program.runtime_payload_value(
                "list rest projection value",
                *ty,
                list.value.project_list_rest(*prefix_len)?,
            )
        }
        ExecutableValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let map = evaluate_runtime_template(context, *map)?;
            context.program.runtime_payload_value(
                "map value projection value",
                *ty,
                map.value.project_map_value(key, keys, *projection)?,
            )
        }
        ExecutableValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            let map = evaluate_runtime_template(context, *map)?;
            context.program.runtime_payload_value(
                "map rest projection value",
                *ty,
                map.value.project_map_rest(excluded_keys)?,
            )
        }
        ExecutableValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            let pid = context.process_refs.get(*process_ref).ok_or_else(|| {
                Error::new(format!(
                    "process {} sends unbound process reference id {} as payload",
                    context.step.process_name,
                    process_ref.as_u32()
                ))
            })?;
            RuntimePayload::from_process_ref(*ty, *target_process, pid)
        }
        ExecutableValueTemplate::LoopElement { ty, element } => {
            let payload = context
                .loop_elements
                .iter()
                .find(|binding| binding.id == *element)
                .map(|binding| binding.payload)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} references inactive loop element id {}",
                        context.step.process_name,
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
        ExecutableValueTemplate::EffectOutcome { ty, outcome } => {
            let payload = context
                .effect_outcomes
                .iter()
                .find(|binding| binding.id == *outcome)
                .map(|binding| &binding.payload)
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} references unbound effect outcome id {}",
                        context.step.process_name,
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
        ExecutableValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload = evaluate_runtime_template(context, *payload)?;
            context.program.runtime_enum_variant_payload(
                "enum variant template value",
                *ty,
                *variant,
                payload.value,
            )
        }
        ExecutableValueTemplate::Record { ty, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            for (field_index, field) in fields.iter().enumerate() {
                let value = evaluate_runtime_template(context, field.value)?;
                if fields[..field_index]
                    .iter()
                    .any(|previous| previous.field == field.field)
                {
                    return Err(Error::new(format!(
                        "record template duplicates field id {}",
                        field.field.as_u32()
                    )));
                }
                values.push((field.field, value.value));
            }
            context
                .program
                .runtime_record_payload("record template value", *ty, values)
        }
        ExecutableValueTemplate::List { ty, items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value = evaluate_runtime_template(context, *item)?;
                values.push(value.value);
            }
            context.program.runtime_payload_value(
                "list template value",
                *ty,
                RuntimeValue::List(values),
            )
        }
        ExecutableValueTemplate::Map { ty, entries } => {
            let mut values: Vec<ArtifactMapEntry> = Vec::with_capacity(entries.len());
            for entry in entries {
                let key = evaluate_runtime_template(context, entry.key)?;
                let value = evaluate_runtime_template(context, entry.value)?;
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
            context.program.runtime_payload_value(
                "map template value",
                *ty,
                RuntimeValue::Map(values),
            )
        }
        ExecutableValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => {
            let condition = evaluate_runtime_template(context, *condition)?;
            let selected = if runtime_bool_value(&condition.value)? {
                then_value
            } else {
                else_value
            };
            let value = evaluate_runtime_template(context, *selected)?;
            if value.ty != *ty {
                return Err(Error::new(format!(
                    "if_else value branch has type id {}, expected {}",
                    value.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(value)
        }
        ExecutableValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(context, *left)?;
            if left.ty != *operand_ty {
                return Err(Error::new(format!(
                    "equality left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let right = evaluate_runtime_template(context, *right)?;
            if right.ty != *operand_ty {
                return Err(Error::new(format!(
                    "equality right operand has type id {}, expected {}",
                    right.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let is_equal = left.value == right.value;
            let selected = match operator {
                mantle_artifact::ArtifactValueEqualityOperator::Equal => is_equal,
                mantle_artifact::ArtifactValueEqualityOperator::NotEqual => !is_equal,
            };
            context.program.runtime_payload_value(
                "equality template value",
                *ty,
                RuntimeValue::Atom(bool_atom(selected)),
            )
        }
        ExecutableValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(context, *left)?;
            if left.ty != *ty {
                return Err(Error::new(format!(
                    "scalar arithmetic left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            let right = evaluate_runtime_template(context, *right)?;
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
            context.program.runtime_payload_value(
                "scalar arithmetic template value",
                *ty,
                RuntimeValue::Scalar(ArtifactScalarValue::checked_arithmetic(
                    *operator, left, right,
                )?),
            )
        }
        ExecutableValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(context, *left)?;
            if left.ty != *operand_ty {
                return Err(Error::new(format!(
                    "scalar ordering left operand has type id {}, expected {}",
                    left.ty.as_u32(),
                    operand_ty.as_u32()
                )));
            }
            let right = evaluate_runtime_template(context, *right)?;
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
            context.program.runtime_payload_value(
                "scalar ordering template value",
                *ty,
                RuntimeValue::Atom(bool_atom(ArtifactScalarValue::compare(
                    *operator, left, right,
                )?)),
            )
        }
        ExecutableValueTemplate::BooleanNot { ty, operand } => {
            let value = evaluate_runtime_template(context, *operand)?;
            context.program.runtime_payload_value(
                "boolean predicate template value",
                *ty,
                RuntimeValue::Atom(bool_atom(!runtime_bool_value(&value.value)?)),
            )
        }
        ExecutableValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_runtime_template(context, *left)?;
            let left = runtime_bool_value(&left.value)?;
            let selected = match operator {
                mantle_artifact::ArtifactValueBooleanOperator::And => {
                    left && runtime_bool_value(&evaluate_runtime_template(context, *right)?.value)?
                }
                mantle_artifact::ArtifactValueBooleanOperator::Or => {
                    left || runtime_bool_value(&evaluate_runtime_template(context, *right)?.value)?
                }
            };
            context.program.runtime_payload_value(
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
