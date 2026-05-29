use super::*;
use crate::ArtifactScalarValue;

impl ArtifactValueTemplate {
    pub fn evaluate_state_value<'types>(
        &self,
        received_payload: Option<&ArtifactPayload>,
        current_state_payload: Option<&ArtifactPayload>,
        type_entry: &dyn Fn(TypeId) -> Result<&'types ArtifactType>,
    ) -> Result<ArtifactStateValue> {
        match self {
            Self::Literal { ty, value } => ArtifactStateValue::from_value(*ty, value.clone()),
            Self::ReceivedPayload { ty } => {
                let payload = received_payload.ok_or_else(|| {
                    Error::new("received payload template requires a payload-bearing message")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "received payload has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                ArtifactStateValue::from_value(payload.ty, payload.value.clone())
            }
            Self::CurrentStatePayload { ty } => {
                let payload = current_state_payload.ok_or_else(|| {
                    Error::new("current state payload template requires a payload-bearing state")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "current state payload has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                ArtifactStateValue::from_value(payload.ty, payload.value.clone())
            }
            Self::EffectOutcome { .. } => Err(Error::new(
                "effect outcome templates require runtime effect outcome bindings",
            )),
            Self::EnumPayload { ty, value, variant } => {
                let value = value.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value_type = type_entry(value.ty)?;
                let variant = enum_variant_label_for_template(value.ty, value_type, *variant)?;
                let payload = value.value.project_enum_payload(variant)?;
                payload.validate_generated_label_len("enum payload projection value")?;
                ArtifactStateValue::from_value(*ty, payload)
            }
            Self::RecordField { ty, record, field } => {
                let record = record.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let field_name = record_field_name_for_template(record.ty, type_entry, *field)?;
                let value = record.value.project_record_field(field_name)?;
                value.validate_generated_label_len("record field projection value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ListElement {
                ty,
                list,
                index,
                len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = list.value.project_list_element(*index, *len)?;
                value.validate_generated_label_len("list element projection value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = list
                    .value
                    .project_list_prefix_element(*index, *prefix_len)?;
                value.validate_generated_label_len("list prefix projection value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ListRest {
                ty,
                list,
                prefix_len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = list.value.project_list_rest(*prefix_len)?;
                value.validate_generated_label_len("list rest projection value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::MapValue {
                ty,
                map,
                key,
                keys,
                projection,
            } => {
                let map =
                    map.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = map.value.project_map_value(key, keys, *projection)?;
                value.validate_generated_label_len("map value projection value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::MapRest {
                ty,
                map,
                excluded_keys,
            } => {
                let map =
                    map.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = map.value.project_map_rest(excluded_keys)?;
                value.validate_generated_label_len("map rest projection value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ProcessRef { .. } => Err(Error::new(
                "process reference template requires runtime process reference bindings",
            )),
            Self::LoopElement { .. } => Err(Error::new(
                "loop element template requires runtime loop element bindings",
            )),
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                let payload = payload.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value_type = type_entry(*ty)?;
                let variant = enum_variant_label_for_template(*ty, value_type, *variant)?;
                let value = ArtifactValue::EnumVariant {
                    variant: variant.to_string(),
                    payload: Box::new(payload.value),
                };
                value.validate_generated_label_len("enum variant template value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Record { ty, fields } => {
                let record_type = type_entry(*ty)?;
                let ty_label = record_type.label.clone();
                let ArtifactValueShape::Record {
                    fields: expected_fields,
                } = record_type.value_shape()?
                else {
                    return Err(Error::new(format!(
                        "record template type id {} must be a record type",
                        ty.as_u32()
                    )));
                };
                let mut values = Vec::with_capacity(fields.len());
                for (index, field) in fields.iter().enumerate() {
                    let value = field.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
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
                    let Some(expected) = expected_fields.get(field.field.index()) else {
                        return Err(Error::new(format!(
                            "record template field id {} is not declared by type id {}",
                            field.field.as_u32(),
                            ty.as_u32()
                        )));
                    };
                    values.push(ArtifactRecordField {
                        name: expected.name.clone(),
                        value: value.value,
                    });
                }
                let value = ArtifactValue::Record {
                    constructor: ty_label,
                    fields: values,
                };
                value.validate_generated_label_len("record template value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::List { ty, items } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    let item_value = item.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    values.push(item_value.value);
                }
                let value = ArtifactValue::List(values);
                value.validate_generated_label_len("list template value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Map { ty, entries } => {
                let mut values: Vec<ArtifactMapEntry> = Vec::with_capacity(entries.len());
                for entry in entries {
                    let key = entry.key.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    let value = entry.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
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
                let value = ArtifactValue::Map(values);
                value.validate_generated_label_len("map template value")?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::IfElse {
                ty,
                condition,
                then_value,
                else_value,
            } => {
                let condition = condition.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let selected = if artifact_bool_value(&condition.value)? {
                    then_value
                } else {
                    else_value
                };
                let value = selected.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
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
            Self::Equality {
                ty,
                operand_ty,
                operator,
                left,
                right,
            } => {
                let left =
                    left.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                if left.ty != *operand_ty {
                    return Err(Error::new(format!(
                        "equality left operand has type id {}, expected {}",
                        left.ty.as_u32(),
                        operand_ty.as_u32()
                    )));
                }
                let right = right.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
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
                ArtifactStateValue::from_value(*ty, ArtifactValue::Atom(bool_atom(selected)))
            }
            Self::ScalarArithmetic {
                ty,
                operator,
                left,
                right,
            } => {
                let left =
                    left.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                if left.ty != *ty {
                    return Err(Error::new(format!(
                        "scalar arithmetic left operand has type id {}, expected {}",
                        left.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                let right = right.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                if right.ty != *ty {
                    return Err(Error::new(format!(
                        "scalar arithmetic right operand has type id {}, expected {}",
                        right.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                let (ArtifactValue::Scalar(left), ArtifactValue::Scalar(right)) =
                    (left.value, right.value)
                else {
                    return Err(Error::new(
                        "scalar arithmetic operands must produce scalar values",
                    ));
                };
                ArtifactStateValue::from_value(
                    *ty,
                    ArtifactValue::Scalar(ArtifactScalarValue::checked_arithmetic(
                        *operator, left, right,
                    )?),
                )
            }
            Self::ScalarOrdering {
                ty,
                operand_ty,
                operator,
                left,
                right,
            } => {
                let left =
                    left.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                if left.ty != *operand_ty {
                    return Err(Error::new(format!(
                        "scalar ordering left operand has type id {}, expected {}",
                        left.ty.as_u32(),
                        operand_ty.as_u32()
                    )));
                }
                let right = right.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                if right.ty != *operand_ty {
                    return Err(Error::new(format!(
                        "scalar ordering right operand has type id {}, expected {}",
                        right.ty.as_u32(),
                        operand_ty.as_u32()
                    )));
                }
                let (ArtifactValue::Scalar(left), ArtifactValue::Scalar(right)) =
                    (left.value, right.value)
                else {
                    return Err(Error::new(
                        "scalar ordering operands must produce scalar values",
                    ));
                };
                ArtifactStateValue::from_value(
                    *ty,
                    ArtifactValue::Atom(bool_atom(ArtifactScalarValue::compare(
                        *operator, left, right,
                    )?)),
                )
            }
            Self::BooleanNot { ty, operand } => {
                let value = operand.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                ArtifactStateValue::from_value(
                    *ty,
                    ArtifactValue::Atom(bool_atom(!artifact_bool_value(&value.value)?)),
                )
            }
            Self::BooleanBinary {
                ty,
                operator,
                left,
                right,
            } => {
                let left =
                    left.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let left = artifact_bool_value(&left.value)?;
                let selected = match operator {
                    ArtifactValueBooleanOperator::And => {
                        left && artifact_bool_value(
                            &right
                                .evaluate_state_value(
                                    received_payload,
                                    current_state_payload,
                                    type_entry,
                                )?
                                .value,
                        )?
                    }
                    ArtifactValueBooleanOperator::Or => {
                        left || artifact_bool_value(
                            &right
                                .evaluate_state_value(
                                    received_payload,
                                    current_state_payload,
                                    type_entry,
                                )?
                                .value,
                        )?
                    }
                };
                ArtifactStateValue::from_value(*ty, ArtifactValue::Atom(bool_atom(selected)))
            }
        }
    }
}

fn record_field_name_for_template<'types>(
    record_ty: TypeId,
    type_entry: &dyn Fn(TypeId) -> Result<&'types ArtifactType>,
    field: RecordFieldId,
) -> Result<&'types str> {
    let record_type = type_entry(record_ty)?;
    let ArtifactValueShape::Record { fields } = record_type.value_shape()? else {
        return Err(Error::new(format!(
            "record field projection type id {} must be a record type",
            record_ty.as_u32()
        )));
    };
    fields
        .get(field.index())
        .map(|field| field.name.as_str())
        .ok_or_else(|| {
            Error::new(format!(
                "record field projection id {} is not declared by type id {}",
                field.as_u32(),
                record_ty.as_u32()
            ))
        })
}

fn bool_atom(value: bool) -> String {
    if value {
        "True".to_string()
    } else {
        "False".to_string()
    }
}

fn artifact_bool_value(value: &ArtifactValue) -> Result<bool> {
    match value {
        ArtifactValue::Atom(label) if label == "True" => Ok(true),
        ArtifactValue::Atom(label) if label == "False" => Ok(false),
        _ => Err(Error::new(format!(
            "boolean predicate operand produced non-Bool value {}",
            value.label()
        ))),
    }
}

fn enum_variant_label_for_template(
    ty: TypeId,
    type_entry: &ArtifactType,
    variant: crate::EnumVariantId,
) -> Result<&str> {
    let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "type id {} is not an enum type",
            ty.as_u32()
        )));
    };
    variants
        .get(variant.index())
        .map(|variant| variant.label.as_str())
        .ok_or_else(|| {
            Error::new(format!(
                "type id {} has no enum variant id {}",
                ty.as_u32(),
                variant.as_u32()
            ))
        })
}
