use super::*;

pub(super) fn check_list_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    element_type: &TypeRef,
    capacity: usize,
    pattern: &ListPattern,
    base_path: &PayloadBindingPath,
) -> Result<Vec<PatternPayloadParam>> {
    let mut bindings = Vec::new();
    for (index, binding) in pattern.elements.iter().enumerate() {
        let element_path =
            base_path.then(list_element_binding_segment(element_type, index, pattern));
        match binding {
            CollectionPatternBinding::Binding(name) => {
                let subject = scope.subject();
                if !scope.seen_bindings.insert(name.to_string()) {
                    return Err(Error::new(format!(
                        "{subject} {} list payload pattern binding {name} is declared more than once",
                        scope.context
                    )));
                }
                validate_pattern_binding_name(scope.binding_context, scope.semantic_index, name)?;
                bindings.push(PatternPayloadParam {
                    name: name.clone(),
                    ty: element_type.clone(),
                    path: element_path,
                });
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let nested_bindings = check_nested_pattern_bindings(
                    scope,
                    element_type,
                    pattern,
                    &element_path,
                    EmptyConstructorPattern::Reject,
                )?;
                if nested_bindings.is_empty() {
                    let subject = scope.subject();
                    return Err(Error::new(format!(
                        "{subject} {} list payload nested pattern must bind at least one value",
                        scope.context
                    )));
                }
                bindings.extend(nested_bindings);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        let subject = scope.subject();
        if !scope.seen_bindings.insert(rest.to_string()) {
            return Err(Error::new(format!(
                "{subject} {} list payload pattern binding {rest} is declared more than once",
                scope.context
            )));
        }
        validate_pattern_binding_name(scope.binding_context, scope.semantic_index, rest)?;
        let rest_ty = list_rest_type(element_type, capacity, pattern.elements.len())?;
        bindings.push(PatternPayloadParam {
            name: rest.clone(),
            ty: rest_ty.clone(),
            path: base_path.then(PayloadProjectionSegment::list_rest(
                rest_ty,
                pattern.elements.len(),
            )),
        });
    }
    if bindings.is_empty() {
        let subject = scope.subject();
        return Err(Error::new(format!(
            "{subject} {} list payload pattern must bind at least one value",
            scope.context
        )));
    }
    Ok(bindings)
}

pub(super) fn validate_list_payload_pattern_capacity(
    subject: &str,
    context: &str,
    payload_type: &TypeRef,
    pattern: &ListPattern,
    capacity: usize,
) -> Result<()> {
    if let Some(pattern_capacity) = pattern.capacity
        && pattern_capacity != capacity
    {
        return Err(Error::new(format!(
            "{subject} {context} list payload pattern has capacity {pattern_capacity}, expected {capacity}"
        )));
    }
    if pattern.elements.len() > capacity {
        return Err(Error::new(format!(
            "{subject} {context} list payload pattern length {} exceeds capacity {capacity} for {payload_type}",
            pattern.elements.len()
        )));
    }
    if pattern.rest.is_some() && pattern.elements.is_empty() {
        return Err(Error::new(format!(
            "{subject} {context} list rest payload pattern must declare at least one prefix element"
        )));
    }
    Ok(())
}

pub(super) fn check_map_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    map_type: MapPatternType<'_>,
    pattern: &MapPattern,
    base_path: &PayloadBindingPath,
) -> Result<Vec<PatternPayloadParam>> {
    let mut seen_keys = BTreeSet::new();
    let mut entry_keys = Vec::with_capacity(pattern.entries.len());
    for entry in &pattern.entries {
        let key = canonical_map_payload_pattern_key(scope, map_type.key, &entry.key)?;
        if !seen_keys.insert(key.clone()) {
            return Err(Error::new(format!(
                "map pattern duplicates key {}",
                key.label()
            )));
        }
        entry_keys.push(key);
    }
    let keys = seen_keys.into_iter().collect::<Vec<_>>();
    let mut bindings = Vec::new();
    for (entry, key) in pattern.entries.iter().zip(entry_keys) {
        let value_path = base_path.then(PayloadProjectionSegment::map_value(
            map_type.value.clone(),
            key,
            keys.clone(),
            map_pattern_projection(pattern),
        ));
        match &entry.binding {
            CollectionPatternBinding::Binding(name) => {
                let subject = scope.subject();
                if !scope.seen_bindings.insert(name.to_string()) {
                    return Err(Error::new(format!(
                        "{subject} {} map payload pattern binding {name} is declared more than once",
                        scope.context
                    )));
                }
                validate_pattern_binding_name(scope.binding_context, scope.semantic_index, name)?;
                bindings.push(PatternPayloadParam {
                    name: name.clone(),
                    ty: map_type.value.clone(),
                    path: value_path,
                });
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let nested_bindings = check_nested_pattern_bindings(
                    scope,
                    map_type.value,
                    pattern,
                    &value_path,
                    EmptyConstructorPattern::Reject,
                )?;
                if nested_bindings.is_empty() {
                    let subject = scope.subject();
                    return Err(Error::new(format!(
                        "{subject} {} map payload nested pattern must bind at least one value",
                        scope.context
                    )));
                }
                bindings.extend(nested_bindings);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        let subject = scope.subject();
        if !scope.seen_bindings.insert(rest.to_string()) {
            return Err(Error::new(format!(
                "{subject} {} map payload pattern binding {rest} is declared more than once",
                scope.context
            )));
        }
        validate_pattern_binding_name(scope.binding_context, scope.semantic_index, rest)?;
        let rest_ty = map_rest_type(map_type.key, map_type.value, map_type.capacity, keys.len())?;
        bindings.push(PatternPayloadParam {
            name: rest.clone(),
            ty: rest_ty.clone(),
            path: base_path.then(PayloadProjectionSegment::map_rest(rest_ty, keys)),
        });
    }
    if bindings.is_empty() {
        let subject = scope.subject();
        return Err(Error::new(format!(
            "{subject} {} map payload pattern must bind at least one value",
            scope.context
        )));
    }
    Ok(bindings)
}

fn canonical_map_payload_pattern_key(
    scope: &NestedPatternBindingScope<'_, '_>,
    key_type: &TypeRef,
    key: &ValueExpr,
) -> Result<ArtifactValue> {
    canonical_source_value_with_bindings(scope.module, scope.semantic_index, key_type, key, &[])
        .map_err(|_| {
            let subject = scope.subject();
            Error::new(format!(
                "{subject} {} map payload pattern keys must be static source values of type {key_type}",
                scope.context
            ))
        })
}

pub(super) fn validate_map_payload_pattern_capacity(
    subject: &str,
    context: &str,
    payload_type: &TypeRef,
    pattern: &MapPattern,
    capacity: usize,
) -> Result<()> {
    if let Some(pattern_capacity) = pattern.capacity
        && pattern_capacity != capacity
    {
        return Err(Error::new(format!(
            "{subject} {context} map payload pattern has capacity {pattern_capacity}, expected {capacity}"
        )));
    }
    if pattern.entries.len() > capacity {
        return Err(Error::new(format!(
            "{subject} {context} map payload pattern entry count {} exceeds capacity {capacity} for {payload_type}",
            pattern.entries.len()
        )));
    }
    if pattern.rest.is_some() && pattern.completeness != MapPatternCompleteness::Subset {
        return Err(Error::new(format!(
            "{subject} {context} map rest binding requires a subset map payload pattern"
        )));
    }
    if pattern.rest.is_some() && pattern.entries.is_empty() {
        return Err(Error::new(format!(
            "{subject} {context} map rest payload pattern must declare at least one key"
        )));
    }
    if pattern.completeness == MapPatternCompleteness::Subset && pattern.entries.is_empty() {
        return Err(Error::new(format!(
            "{subject} {context} subset map payload pattern must declare at least one key"
        )));
    }
    Ok(())
}

fn map_pattern_projection(pattern: &MapPattern) -> MapProjectionMode {
    match pattern.completeness {
        MapPatternCompleteness::Exact => MapProjectionMode::Exact,
        MapPatternCompleteness::Subset => MapProjectionMode::Subset,
    }
}

pub(in crate::language::checker) fn list_element_binding_segment(
    element_type: &TypeRef,
    index: usize,
    pattern: &ListPattern,
) -> PayloadProjectionSegment {
    if pattern.rest.is_some() {
        PayloadProjectionSegment::list_prefix_index(
            element_type.clone(),
            index,
            pattern.elements.len(),
        )
    } else {
        PayloadProjectionSegment::list_index(element_type.clone(), index, pattern.elements.len())
    }
}
