use super::*;

pub(in crate::language::checker::static_validation) fn checked_template_depends_on_received_payload(
    template: &CheckedValueTemplate,
) -> bool {
    match template {
        CheckedValueTemplate::Literal(_) => false,
        CheckedValueTemplate::ReceivedPayload { .. } => true,
        CheckedValueTemplate::CurrentStatePayload { .. } => false,
        CheckedValueTemplate::EnumPayload { value, .. } => {
            checked_template_depends_on_received_payload(value)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            checked_template_depends_on_received_payload(record)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            checked_template_depends_on_received_payload(list)
        }
        CheckedValueTemplate::MapValue { map, .. } => {
            checked_template_depends_on_received_payload(map)
        }
        CheckedValueTemplate::MapRest { map, .. } => {
            checked_template_depends_on_received_payload(map)
        }
        CheckedValueTemplate::ProcessRef { .. } => false,
        CheckedValueTemplate::LoopElement { .. } => false,
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            checked_template_depends_on_received_payload(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_received_payload(field.value())),
        CheckedValueTemplate::List { items, .. } => items
            .iter()
            .any(checked_template_depends_on_received_payload),
        CheckedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            checked_template_depends_on_received_payload(entry.key())
                || checked_template_depends_on_received_payload(entry.value())
        }),
        CheckedValueTemplate::Equality { left, right, .. } => {
            checked_template_depends_on_received_payload(left)
                || checked_template_depends_on_received_payload(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            checked_template_depends_on_received_payload(operand)
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            checked_template_depends_on_received_payload(left)
                || checked_template_depends_on_received_payload(right)
        }
    }
}

pub(in crate::language::checker::static_validation) fn checked_template_depends_on_loop_element(
    template: &CheckedValueTemplate,
) -> bool {
    match template {
        CheckedValueTemplate::LoopElement { .. } => true,
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::ProcessRef { .. } => false,
        CheckedValueTemplate::EnumPayload { value, .. } => {
            checked_template_depends_on_loop_element(value)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            checked_template_depends_on_loop_element(record)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            checked_template_depends_on_loop_element(list)
        }
        CheckedValueTemplate::MapValue { map, .. } => checked_template_depends_on_loop_element(map),
        CheckedValueTemplate::MapRest { map, .. } => checked_template_depends_on_loop_element(map),
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            checked_template_depends_on_loop_element(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_loop_element(field.value())),
        CheckedValueTemplate::List { items, .. } => {
            items.iter().any(checked_template_depends_on_loop_element)
        }
        CheckedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            checked_template_depends_on_loop_element(entry.key())
                || checked_template_depends_on_loop_element(entry.value())
        }),
        CheckedValueTemplate::Equality { left, right, .. } => {
            checked_template_depends_on_loop_element(left)
                || checked_template_depends_on_loop_element(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            checked_template_depends_on_loop_element(operand)
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            checked_template_depends_on_loop_element(left)
                || checked_template_depends_on_loop_element(right)
        }
    }
}

pub(in crate::language::checker::static_validation) fn evaluate_checked_template(
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
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
            if payload.process_ref_payload().is_some() {
                return Err(Error::new(
                    "process reference payloads are not valid state values",
                ));
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
        CheckedValueTemplate::EnumPayload { ty, value, variant } => {
            let value = evaluate_checked_template(value, received_payload, current_state_payload)?;
            let variant = value.ty().enum_variant_label(*variant)?;
            let payload = checked_payload_value(&value)?
                .project_enum_payload(variant.as_str())
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), payload))
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            let record =
                evaluate_checked_template(record, received_payload, current_state_payload)?;
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
            let list = evaluate_checked_template(list, received_payload, current_state_payload)?;
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
            let list = evaluate_checked_template(list, received_payload, current_state_payload)?;
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
            let list = evaluate_checked_template(list, received_payload, current_state_payload)?;
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
            let map = evaluate_checked_template(map, received_payload, current_state_payload)?;
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
            let map = evaluate_checked_template(map, received_payload, current_state_payload)?;
            let value = checked_payload_value(&map)?
                .project_map_rest(excluded_keys)
                .map_err(|err| Error::new(err.to_string()))?;
            Ok(CheckedPayloadValue::new(ty.clone(), value))
        }
        CheckedValueTemplate::ProcessRef { .. } => Err(Error::new(
            "process reference template requires static runtime process reference bindings",
        )),
        CheckedValueTemplate::LoopElement { .. } => Err(Error::new(
            "loop element template requires static runtime loop element bindings",
        )),
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload =
                evaluate_checked_template(payload, received_payload, current_state_payload)?;
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
            let mut seen = BTreeSet::new();
            for field in fields {
                let value = evaluate_checked_template(
                    field.value(),
                    received_payload,
                    current_state_payload,
                )?;
                if !seen.insert(field.name()) {
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
                let value =
                    evaluate_checked_template(item, received_payload, current_state_payload)?;
                values.push(checked_payload_value(&value)?);
            }
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::List(values),
            ))
        }
        CheckedValueTemplate::Map { ty, entries } => {
            let mut values = Vec::with_capacity(entries.len());
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = evaluate_checked_template(
                    entry.key(),
                    received_payload,
                    current_state_payload,
                )?;
                let value = evaluate_checked_template(
                    entry.value(),
                    received_payload,
                    current_state_payload,
                )?;
                let key_value = checked_payload_value(&key)?;
                let item_value = checked_payload_value(&value)?;
                if !seen.insert(key_value.clone()) {
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
        CheckedValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_template(left, received_payload, current_state_payload)?;
            if left.ty() != operand_ty {
                return Err(Error::new(format!(
                    "equality left operand has type {}, expected {}",
                    left.ty(),
                    operand_ty
                )));
            }
            let left_value = checked_payload_value_ref(&left)?;
            let right = evaluate_checked_template(right, received_payload, current_state_payload)?;
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
        CheckedValueTemplate::BooleanNot { ty, operand } => {
            let value =
                evaluate_checked_template(operand, received_payload, current_state_payload)?;
            Ok(CheckedPayloadValue::new(
                ty.clone(),
                ArtifactValue::Atom(bool_atom(!checked_template_bool_value(&value)?)),
            ))
        }
        CheckedValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            let left = evaluate_checked_template(left, received_payload, current_state_payload)?;
            let left = checked_template_bool_value(&left)?;
            let selected = match operator {
                CheckedValueBooleanOperator::And => {
                    left && checked_template_bool_value(&evaluate_checked_template(
                        right,
                        received_payload,
                        current_state_payload,
                    )?)?
                }
                CheckedValueBooleanOperator::Or => {
                    left || checked_template_bool_value(&evaluate_checked_template(
                        right,
                        received_payload,
                        current_state_payload,
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

fn checked_template_bool_value(payload: &CheckedPayloadValue) -> Result<bool> {
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

fn checked_payload_value(payload: &CheckedPayloadValue) -> Result<ArtifactValue> {
    payload
        .value()
        .cloned()
        .ok_or_else(|| Error::new("process reference payloads are not valid state values"))
}

pub(in crate::language::checker::static_validation) fn resolve_checked_template_state(
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedStateId> {
    let value = evaluate_checked_template(template, received_payload, current_state_payload)?;
    let state_index = process
        .state_values()
        .iter()
        .position(|state| state.has_same_identity_as_payload(&value))
        .ok_or_else(|| {
            Error::new(format!(
                "process {} next_state template produced value {} not admitted by state table",
                process.debug_name(),
                value.label()
            ))
        })?;
    CheckedStateId::from_index(state_index)
}

pub(in crate::language::checker::static_validation) fn resolve_checked_next_state(
    process: &CheckedProcess,
    current_state: CheckedStateId,
    next_state: &CheckedNextState,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<CheckedStateId> {
    let current_state_payload = process
        .state_values()
        .get(current_state.index())
        .and_then(|state| state.payload());
    match next_state {
        CheckedNextState::Current => Ok(current_state),
        CheckedNextState::Value(state) => Ok(*state),
        CheckedNextState::Template(template) => resolve_checked_template_state(
            process,
            template,
            received_payload,
            current_state_payload,
        ),
        CheckedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            let selected_state = match checked_bool_condition_value(
                process,
                condition,
                received_payload,
                current_state_payload,
            )? {
                true => then_state,
                false => else_state,
            };
            resolve_checked_next_state(process, current_state, selected_state, received_payload)
        }
    }
}

fn checked_bool_condition_value(
    process: &CheckedProcess,
    condition: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
) -> Result<bool> {
    let value = evaluate_checked_template(condition, received_payload, current_state_payload)?;
    let value = value.value().ok_or_else(|| {
        Error::new(format!(
            "process {} if condition produced a process reference payload",
            process.debug_name()
        ))
    })?;
    let ArtifactValue::Atom(label) = value else {
        return Err(Error::new(format!(
            "process {} if condition produced non-Bool value {}",
            process.debug_name(),
            value.label()
        )));
    };
    match label.as_str() {
        "True" => Ok(true),
        "False" => Ok(false),
        _ => Err(Error::new(format!(
            "process {} if condition produced invalid Bool value {}",
            process.debug_name(),
            label
        ))),
    }
}
