use super::*;

pub(super) fn check_list_pattern_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    element_type: &TypeRef,
    capacity: usize,
    pattern: &ListPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let mut seen_bindings = BTreeSet::new();
    let mut bindings = Vec::new();
    for (index, binding) in pattern.elements.iter().enumerate() {
        let element_path = PayloadBindingPath::whole().then(list_element_binding_segment(
            element_type,
            index,
            pattern,
        ));
        match binding {
            CollectionPatternBinding::Binding(name) => {
                if !seen_bindings.insert(name.as_str()) {
                    return Err(Error::new(format!(
                        "{subject} list pattern binding {name} is declared more than once"
                    )));
                }
                validate_source_pattern_binding_name(subject, semantic_index, name)?;
                bindings.push(PatternPayloadParam {
                    name: name.clone(),
                    ty: element_type.clone(),
                    path: element_path,
                });
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let mut nested_scope = NestedPatternBindingScope {
                    module,
                    semantic_index,
                    binding_context: PatternBindingContext::Source { owner: subject },
                    context: "pattern",
                    seen_bindings: &mut seen_bindings,
                };
                let nested_bindings = check_nested_pattern_bindings(
                    &mut nested_scope,
                    element_type,
                    pattern,
                    &element_path,
                    EmptyConstructorPattern::Reject,
                )?;
                if nested_bindings.is_empty() {
                    return Err(Error::new(format!(
                        "{subject} list nested pattern must bind at least one value"
                    )));
                }
                bindings.extend(nested_bindings);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        if !seen_bindings.insert(rest.as_str()) {
            return Err(Error::new(format!(
                "{subject} list pattern binding {rest} is declared more than once"
            )));
        }
        validate_source_pattern_binding_name(subject, semantic_index, rest)?;
        let rest_ty = list_rest_type(element_type, capacity, pattern.elements.len())?;
        bindings.push(PatternPayloadParam {
            name: rest.clone(),
            ty: rest_ty.clone(),
            path: PayloadBindingPath::whole().then(PayloadProjectionSegment::list_rest(
                rest_ty,
                pattern.elements.len(),
            )),
        });
    }
    Ok(bindings)
}

pub(super) fn check_map_pattern_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
    pattern: &MapPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let keys = canonical_map_pattern_keys(module, semantic_index, subject, key_type, pattern)?;
    let keys = std::sync::Arc::<[ArtifactValue]>::from(keys);
    let mut seen_bindings = BTreeSet::new();
    let mut bindings = Vec::new();
    for entry in &pattern.entries {
        let key = canonical_map_pattern_key(module, semantic_index, subject, key_type, &entry.key)?;
        let value_path = PayloadBindingPath::whole().then(PayloadProjectionSegment::map_value(
            value_type.clone(),
            key,
            keys.clone(),
            map_pattern_projection(pattern),
        ));
        match &entry.binding {
            CollectionPatternBinding::Binding(name) => {
                if !seen_bindings.insert(name.as_str()) {
                    return Err(Error::new(format!(
                        "{subject} map pattern binding {name} is declared more than once"
                    )));
                }
                validate_source_pattern_binding_name(subject, semantic_index, name)?;
                bindings.push(PatternPayloadParam {
                    name: name.clone(),
                    ty: value_type.clone(),
                    path: value_path,
                });
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let mut nested_scope = NestedPatternBindingScope {
                    module,
                    semantic_index,
                    binding_context: PatternBindingContext::Source { owner: subject },
                    context: "pattern",
                    seen_bindings: &mut seen_bindings,
                };
                let nested_bindings = check_nested_pattern_bindings(
                    &mut nested_scope,
                    value_type,
                    pattern,
                    &value_path,
                    EmptyConstructorPattern::Reject,
                )?;
                if nested_bindings.is_empty() {
                    return Err(Error::new(format!(
                        "{subject} map nested pattern must bind at least one value"
                    )));
                }
                bindings.extend(nested_bindings);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        if !seen_bindings.insert(rest.as_str()) {
            return Err(Error::new(format!(
                "{subject} map pattern binding {rest} is declared more than once"
            )));
        }
        validate_source_pattern_binding_name(subject, semantic_index, rest)?;
        let rest_ty = map_rest_type(key_type, value_type, capacity, keys.len())?;
        bindings.push(PatternPayloadParam {
            name: rest.clone(),
            ty: rest_ty.clone(),
            path: PayloadBindingPath::whole()
                .then(PayloadProjectionSegment::map_rest(rest_ty, keys)),
        });
    }
    Ok(bindings)
}

pub(super) fn canonical_map_pattern_keys(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    key_type: &TypeRef,
    pattern: &MapPattern,
) -> Result<Vec<ArtifactValue>> {
    let mut keys = BTreeSet::new();
    for entry in &pattern.entries {
        let key = canonical_map_pattern_key(module, semantic_index, subject, key_type, &entry.key)?;
        if !keys.insert(key.clone()) {
            return Err(Error::new(format!(
                "map pattern duplicates key {}",
                key.label()
            )));
        }
    }
    Ok(keys.into_iter().collect())
}

fn canonical_map_pattern_key(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    key_type: &TypeRef,
    key: &ValueExpr,
) -> Result<ArtifactValue> {
    canonical_source_value_with_bindings(module, semantic_index, key_type, key, &[]).map_err(|_| {
        Error::new(format!(
            "{subject} map pattern keys must be static source values of type {key_type}"
        ))
    })
}

pub(super) fn list_pattern_substitutions(
    module: &Module,
    semantic_index: &SemanticIndex,
    element_type: &TypeRef,
    capacity: usize,
    pattern: &ListPattern,
    value: &ListValue,
) -> Result<Option<Vec<SourceSubstitution>>> {
    let mut substitutions = Vec::new();
    for (binding, value) in pattern.elements.iter().zip(&value.items) {
        match binding {
            CollectionPatternBinding::Binding(name) => {
                substitutions.push(SourceSubstitution::new(name.clone(), value.clone()));
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let Some(mut nested) = source_nested_pattern_substitutions(
                    module,
                    semantic_index,
                    element_type,
                    pattern,
                    value,
                )?
                else {
                    return Ok(None);
                };
                substitutions.append(&mut nested);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        let rest_capacity = capacity
            .checked_sub(pattern.elements.len())
            .ok_or_else(|| {
                Error::new(format!(
                    "list rest binding removes {} prefix elements from capacity {capacity}",
                    pattern.elements.len()
                ))
            })?;
        substitutions.push(SourceSubstitution::new(
            rest.clone(),
            ValueExpr::List(ListValue {
                element_type: Some(element_type.clone()),
                capacity: Some(rest_capacity),
                items: value
                    .items
                    .iter()
                    .skip(pattern.elements.len())
                    .cloned()
                    .collect(),
            }),
        ));
    }
    Ok(Some(substitutions))
}

pub(super) fn map_pattern_substitutions(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    map_type: MapPatternType<'_>,
    pattern: &MapPattern,
    value: &MapValue,
) -> Result<Option<Vec<SourceSubstitution>>> {
    let mut value_entries = BTreeMap::new();
    for entry in &value.entries {
        let key = canonical_source_value_with_bindings(
            module,
            semantic_index,
            map_type.key,
            &entry.key,
            &[],
        )?;
        if value_entries.insert(key.clone(), &entry.value).is_some() {
            return Err(Error::new(format!(
                "map value duplicates key {}",
                key.label()
            )));
        }
    }

    let mut substitutions = Vec::new();
    let mut pattern_keys = BTreeSet::new();
    for entry in &pattern.entries {
        let key =
            canonical_map_pattern_key(module, semantic_index, subject, map_type.key, &entry.key)?;
        if !pattern_keys.insert(key.clone()) {
            return Err(Error::new(format!(
                "map pattern duplicates key {}",
                key.label()
            )));
        }
        let Some(value) = value_entries.get(&key).copied() else {
            return Ok(None);
        };
        match &entry.binding {
            CollectionPatternBinding::Binding(binding) => {
                substitutions.push(SourceSubstitution::new(binding.clone(), value.clone()));
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let Some(mut nested) = source_nested_pattern_substitutions(
                    module,
                    semantic_index,
                    map_type.value,
                    pattern,
                    value,
                )?
                else {
                    return Ok(None);
                };
                substitutions.append(&mut nested);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if pattern.completeness == MapPatternCompleteness::Exact
        && pattern_keys.len() != value_entries.len()
    {
        return Ok(None);
    }
    if let Some(rest) = &pattern.rest {
        let rest_capacity = map_type
            .capacity
            .checked_sub(pattern_keys.len())
            .ok_or_else(|| {
                Error::new(format!(
                    "map rest binding removes {} key(s) from capacity {}",
                    pattern_keys.len(),
                    map_type.capacity
                ))
            })?;
        let rest_entries = value
            .entries
            .iter()
            .map(|entry| {
                let key = canonical_source_value_with_bindings(
                    module,
                    semantic_index,
                    map_type.key,
                    &entry.key,
                    &[],
                )?;
                Ok((key, entry))
            })
            .filter_map(|result| match result {
                Ok((key, entry)) if !pattern_keys.contains(&key) => Some(Ok(entry.clone())),
                Ok(_) => None,
                Err(err) => Some(Err(err)),
            })
            .collect::<Result<Vec<_>>>()?;
        substitutions.push(SourceSubstitution::new(
            rest.clone(),
            ValueExpr::Map(MapValue {
                key_type: Some(map_type.key.clone()),
                value_type: Some(map_type.value.clone()),
                capacity: Some(rest_capacity),
                entries: rest_entries,
            }),
        ));
    }
    Ok(Some(substitutions))
}

pub(in crate::language::checker::source_functions) fn source_nested_pattern_substitutions(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    pattern: &Pattern,
    value: &ValueExpr,
) -> Result<Option<Vec<SourceSubstitution>>> {
    match pattern {
        Pattern::Record { name, fields } => {
            let record = semantic_index.record_decl(module, expected_type)?;
            if record.name != *name {
                return Ok(None);
            }
            let ValueExpr::Record(record_value) = value else {
                return Ok(None);
            };
            let mut substitutions = Vec::with_capacity(fields.len());
            for field in fields {
                let Some(value_field) = record_value
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field.field)
                else {
                    return Err(Error::new(format!(
                        "record pattern {} could not resolve field {}",
                        record.name, field.field
                    )));
                };
                substitutions.push(SourceSubstitution::new(
                    field.binding.clone(),
                    value_field.value.clone(),
                ));
            }
            Ok(Some(substitutions))
        }
        Pattern::List(pattern) => {
            let Some(CollectionType::List { element, capacity }) =
                semantic_index.collection_type(expected_type)?
            else {
                return Ok(None);
            };
            let ValueExpr::List(list) = value else {
                return Ok(None);
            };
            if !list_pattern_matches(pattern, list) {
                return Ok(None);
            }
            list_pattern_substitutions(module, semantic_index, element, capacity, pattern, list)
        }
        Pattern::Map(pattern) => {
            let Some(CollectionType::Map {
                key,
                value: item,
                capacity,
            }) = semantic_index.collection_type(expected_type)?
            else {
                return Ok(None);
            };
            let ValueExpr::Map(map) = value else {
                return Ok(None);
            };
            map_pattern_substitutions(
                module,
                semantic_index,
                "nested map pattern",
                MapPatternType {
                    key,
                    value: item,
                    capacity,
                },
                pattern,
                map,
            )
        }
        Pattern::Constructor { name, payload } => {
            let enum_decl = semantic_index.enum_decl(module, expected_type)?;
            let variant_index = semantic_index.enum_variant_index(module, expected_type, name)?;
            let variant = &enum_decl.variants[variant_index];
            match (&variant.payload_type, payload) {
                (None, None) => match value {
                    ValueExpr::Identifier(value_name) if value_name == &variant.name => {
                        Ok(Some(Vec::new()))
                    }
                    _ => Ok(None),
                },
                (Some(_payload_type), Some(ConstructorPayloadPattern::Binding(binding))) => {
                    let ValueExpr::EnumVariant {
                        name: value_name,
                        payload: value_payload,
                    } = value
                    else {
                        return Ok(None);
                    };
                    if value_name != &variant.name {
                        return Ok(None);
                    }
                    Ok(Some(vec![SourceSubstitution::new(
                        binding.name.clone(),
                        (**value_payload).clone(),
                    )]))
                }
                (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
                    let ValueExpr::EnumVariant {
                        name: value_name,
                        payload: value_payload,
                    } = value
                    else {
                        return Ok(None);
                    };
                    if value_name != &variant.name {
                        return Ok(None);
                    }
                    source_nested_pattern_substitutions(
                        module,
                        semantic_index,
                        payload_type,
                        pattern,
                        value_payload,
                    )
                }
                _ => Ok(None),
            }
        }
        Pattern::Wildcard => Ok(Some(Vec::new())),
    }
}

pub(super) fn map_pattern_projection(pattern: &MapPattern) -> MapProjectionMode {
    match pattern.completeness {
        MapPatternCompleteness::Exact => MapProjectionMode::Exact,
        MapPatternCompleteness::Subset => MapProjectionMode::Subset,
    }
}
