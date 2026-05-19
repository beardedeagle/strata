use super::*;

impl ArtifactValueTemplate {
    pub fn evaluate_state_value(
        &self,
        received_payload: Option<&ArtifactPayload>,
        current_state_payload: Option<&ArtifactPayload>,
        type_entry: &dyn Fn(TypeId) -> Result<ArtifactType>,
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
            Self::EnumPayload { ty, value, variant } => {
                let value = value.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value_type = type_entry(value.ty)?;
                let variant = enum_variant_label_for_template(value.ty, &value_type, *variant)?;
                let payload = value.value.project_enum_payload(variant)?;
                validate_value_label("enum payload projection value", &payload.label())?;
                ArtifactStateValue::from_value(*ty, payload)
            }
            Self::RecordField { ty, record, field } => {
                let record = record.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value = record.value.project_record_field(field)?;
                validate_value_label("record field projection value", &value.label())?;
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
                validate_value_label("list element projection value", &value.label())?;
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
                validate_value_label("list prefix projection value", &value.label())?;
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
                validate_value_label("list rest projection value", &value.label())?;
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
                validate_value_label("map value projection value", &value.label())?;
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
                validate_value_label("map rest projection value", &value.label())?;
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
                let variant = enum_variant_label_for_template(*ty, &value_type, *variant)?;
                let value = ArtifactValue::EnumVariant {
                    variant: variant.to_string(),
                    payload: Box::new(payload.value),
                };
                validate_value_label("enum variant template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Record { ty, fields } => {
                let ty_label = type_entry(*ty)?.label;
                let mut values = Vec::with_capacity(fields.len());
                let mut seen = BTreeSet::new();
                for field in fields {
                    let value = field.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    if !seen.insert(field.name.as_str()) {
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
                let value = ArtifactValue::Record {
                    constructor: ty_label,
                    fields: values,
                };
                validate_value_label("record template value", &value.label())?;
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
                validate_value_label("list template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Map { ty, entries } => {
                let mut values = Vec::with_capacity(entries.len());
                let mut seen = BTreeSet::new();
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
                    if !seen.insert(key.value.clone()) {
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
                validate_value_label("map template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
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
