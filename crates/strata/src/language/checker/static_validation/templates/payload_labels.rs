use super::*;

pub(in crate::language::checker::static_validation) fn validate_value_template_payload_labels(
    template: &CheckedValueTemplate,
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(value) => validate_checked_payload_value(value),
        CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::LoopElement { .. } => Ok(()),
        CheckedValueTemplate::EnumPayload { value, .. } => {
            validate_value_template_payload_labels(value)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            validate_value_template_payload_labels(record)
        }
        CheckedValueTemplate::ListElement { list, .. } => {
            validate_value_template_payload_labels(list)
        }
        CheckedValueTemplate::ListPrefixElement {
            list,
            index,
            prefix_len,
            ..
        } => {
            validate_list_prefix_projection(*index, *prefix_len)?;
            validate_value_template_payload_labels(list)
        }
        CheckedValueTemplate::ListRest {
            list, prefix_len, ..
        } => {
            validate_list_rest_projection_prefix(*prefix_len)?;
            validate_value_template_payload_labels(list)
        }
        CheckedValueTemplate::MapValue { map, key, keys, .. } => {
            validate_map_projection_keys(key, keys)?;
            validate_value_template_payload_labels(map)
        }
        CheckedValueTemplate::MapRest {
            map, excluded_keys, ..
        } => {
            validate_map_rest_projection_keys(excluded_keys)?;
            validate_value_template_payload_labels(map)
        }
        CheckedValueTemplate::Equality { left, right, .. } => {
            validate_value_template_payload_labels(left)?;
            validate_value_template_payload_labels(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            validate_value_template_payload_labels(operand)
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            validate_value_template_payload_labels(left)?;
            validate_value_template_payload_labels(right)
        }
        CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            validate_value_template_payload_labels(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => {
            validate_record_template_shape(fields)?;
            for field in fields {
                validate_value_template_payload_labels(field.value())?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            validate_list_template_shape(items)?;
            for item in items {
                validate_value_template_payload_labels(item)?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            validate_map_template_shape(entries)?;
            Ok(())
        }
    }
}

fn validate_record_template_shape(fields: &[CheckedValueTemplateField]) -> Result<()> {
    if fields.is_empty() {
        return Err(Error::new(
            "record template field_count must be greater than zero",
        ));
    }
    if fields.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "record template field_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    let mut names = BTreeSet::new();
    for field in fields {
        if !names.insert(field.name().as_str()) {
            return Err(Error::new(format!(
                "record template duplicates field {}",
                field.name()
            )));
        }
    }
    Ok(())
}

fn validate_list_template_shape(items: &[CheckedValueTemplate]) -> Result<()> {
    if items.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "list template item_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    Ok(())
}

fn validate_map_template_shape(entries: &[CheckedValueTemplateMapEntry]) -> Result<()> {
    if entries.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "map template entry_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    let mut keys = BTreeSet::new();
    for entry in entries {
        validate_value_template_payload_labels(entry.key())?;
        let key = checked_static_template_value(entry.key()).ok_or_else(|| {
            Error::new("map template keys must be static source values in this source slice")
        })?;
        validate_artifact_value("map template key", &key)?;
        if !keys.insert(key.clone()) {
            return Err(Error::new(format!(
                "map template duplicates key {}",
                key.label()
            )));
        }
        validate_value_template_payload_labels(entry.value())?;
    }
    Ok(())
}

fn validate_checked_payload_value(value: &CheckedPayloadValue) -> Result<()> {
    let Some(value) = value.value() else {
        return Err(Error::new(
            "literal process reference template must be explicit",
        ));
    };
    validate_artifact_value("payload value", value)
}

fn validate_artifact_value(field: &str, value: &ArtifactValue) -> Result<()> {
    value
        .validate(field)
        .map_err(|err| Error::new(err.to_string()))?;
    if value.contains_process_ref() {
        return Err(Error::new(format!(
            "{field} must not contain a process reference value"
        )));
    }
    Ok(())
}

fn checked_static_template_value(template: &CheckedValueTemplate) -> Option<ArtifactValue> {
    match template {
        CheckedValueTemplate::Literal(value) => value.value().cloned(),
        CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::EnumPayload { .. }
        | CheckedValueTemplate::RecordField { .. }
        | CheckedValueTemplate::ListElement { .. }
        | CheckedValueTemplate::ListPrefixElement { .. }
        | CheckedValueTemplate::ListRest { .. }
        | CheckedValueTemplate::MapValue { .. }
        | CheckedValueTemplate::MapRest { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::LoopElement { .. }
        | CheckedValueTemplate::Equality { .. }
        | CheckedValueTemplate::BooleanNot { .. }
        | CheckedValueTemplate::BooleanBinary { .. } => None,
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => Some(ArtifactValue::EnumVariant {
            variant: ty.enum_variant_label(*variant).ok()?.to_string(),
            payload: Box::new(checked_static_template_value(payload)?),
        }),
        CheckedValueTemplate::Record { ty, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            let mut seen = BTreeSet::new();
            for field in fields {
                if !seen.insert(field.name()) {
                    return None;
                }
                values.push(ArtifactRecordField {
                    name: field.name().to_string(),
                    value: checked_static_template_value(field.value())?,
                });
            }
            Some(ArtifactValue::Record {
                constructor: ty.label().to_string(),
                fields: values,
            })
        }
        CheckedValueTemplate::List { items, .. } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(checked_static_template_value(item)?);
            }
            Some(ArtifactValue::List(values))
        }
        CheckedValueTemplate::Map { entries, .. } => {
            let mut values = Vec::with_capacity(entries.len());
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = checked_static_template_value(entry.key())?;
                let value = checked_static_template_value(entry.value())?;
                if !seen.insert(key.clone()) {
                    return None;
                }
                values.push(ArtifactMapEntry { key, value });
            }
            Some(ArtifactValue::Map(values))
        }
    }
}

fn validate_map_projection_keys(key: &ArtifactValue, keys: &[ArtifactValue]) -> Result<()> {
    validate_map_key_set("map projection", keys, MapKeySetKind::Expected)?;
    validate_artifact_value("map projection key", key)?;
    if keys.binary_search(key).is_err() {
        return Err(Error::new(format!(
            "map projection key {} is not one of the expected map keys",
            key.label()
        )));
    }
    Ok(())
}

fn validate_map_rest_projection_keys(keys: &[ArtifactValue]) -> Result<()> {
    validate_map_key_set("map rest projection", keys, MapKeySetKind::Excluded)
}

fn validate_list_rest_projection_prefix(prefix_len: usize) -> Result<()> {
    if prefix_len == 0 || prefix_len > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "list rest projection prefix length must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    Ok(())
}

fn validate_list_prefix_projection(index: usize, prefix_len: usize) -> Result<()> {
    if prefix_len == 0 || prefix_len > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "list prefix projection prefix length must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    if index >= prefix_len {
        return Err(Error::new(format!(
            "list prefix projection index {index} is outside prefix length {prefix_len}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum MapKeySetKind {
    Expected,
    Excluded,
}

impl MapKeySetKind {
    fn field_label(self) -> &'static str {
        match self {
            Self::Expected => "expected key",
            Self::Excluded => "excluded key",
        }
    }

    fn singular(self) -> &'static str {
        match self {
            Self::Expected => "expected map key",
            Self::Excluded => "excluded map key",
        }
    }

    fn plural(self) -> &'static str {
        match self {
            Self::Expected => "expected keys",
            Self::Excluded => "excluded keys",
        }
    }
}

fn validate_map_key_set(field: &str, keys: &[ArtifactValue], kind: MapKeySetKind) -> Result<()> {
    if keys.is_empty() {
        return Err(Error::new(format!(
            "{field} key_count must be greater than zero"
        )));
    }
    if keys.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "{field} key_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    let mut seen = BTreeSet::new();
    for map_key in keys {
        validate_artifact_value(&format!("{field} {}", kind.field_label()), map_key)?;
        if !seen.insert(map_key.clone()) {
            return Err(Error::new(format!(
                "{field} duplicates {} {}",
                kind.singular(),
                map_key.label()
            )));
        }
    }
    if seen.into_iter().collect::<Vec<_>>() != keys {
        return Err(Error::new(format!(
            "{field} {} must be sorted canonically",
            kind.plural()
        )));
    }
    Ok(())
}
