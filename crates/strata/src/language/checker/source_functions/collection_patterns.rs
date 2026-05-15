use super::*;
use crate::language::{LIST_TYPE, MAP_TYPE};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::language::checker::source_functions) enum CollectionPatternShape {
    List {
        prefix_len: usize,
        completeness: ListPatternCompleteness,
    },
    Map {
        keys: Vec<ArtifactValue>,
        completeness: MapPatternCompleteness,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::language::checker::source_functions) enum ListPatternCompleteness {
    Exact,
    Rest,
}

pub(in crate::language::checker::source_functions) struct CollectionPatternResolution {
    pub(in crate::language::checker::source_functions) substitutions: Vec<SourceSubstitution>,
    pub(in crate::language::checker::source_functions) bindings: Vec<PatternPayloadParam>,
}

#[derive(Clone, Copy)]
struct MapPatternType<'a> {
    key: &'a TypeRef,
    value: &'a TypeRef,
    capacity: usize,
}

pub(in crate::language::checker::source_functions) fn validate_list_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[&Function],
) -> Result<()> {
    validate_collection_pattern_source_function_group(
        module,
        semantic_index,
        owner,
        process,
        functions,
        CollectionPatternKind::List,
    )
}

pub(in crate::language::checker::source_functions) fn validate_map_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[&Function],
) -> Result<()> {
    validate_collection_pattern_source_function_group(
        module,
        semantic_index,
        owner,
        process,
        functions,
        CollectionPatternKind::Map,
    )
}

pub(in crate::language::checker::source_functions) fn collection_pattern_type(
    function: &Function,
) -> Result<TypeRef> {
    match &function.params[0] {
        FunctionParam::Pattern(Pattern::List(pattern)) => {
            let (Some(element), Some(capacity)) = (&pattern.element_type, pattern.capacity) else {
                return Err(Error::new(format!(
                    "function {} list pattern signature must declare List<T,N>",
                    function.name
                )));
            };
            Ok(TypeRef::Applied {
                constructor: Identifier::new(LIST_TYPE)?,
                args: vec![element.clone()],
                const_args: vec![capacity],
            })
        }
        FunctionParam::Pattern(Pattern::Map(pattern)) => {
            let (Some(key), Some(value), Some(capacity)) =
                (&pattern.key_type, &pattern.value_type, pattern.capacity)
            else {
                return Err(Error::new(format!(
                    "function {} map pattern signature must declare Map<K,V,N>",
                    function.name
                )));
            };
            Ok(TypeRef::Applied {
                constructor: Identifier::new(MAP_TYPE)?,
                args: vec![key.clone(), value.clone()],
                const_args: vec![capacity],
            })
        }
        _ => Err(Error::new(format!(
            "function {} must declare a collection pattern parameter",
            function.name
        ))),
    }
}

pub(in crate::language::checker::source_functions) fn check_collection_pattern_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    expected_type: &TypeRef,
    pattern: &Pattern,
) -> Result<Vec<PatternPayloadParam>> {
    match semantic_index.collection_type(expected_type)? {
        Some(CollectionType::List { element, capacity }) => {
            let Pattern::List(pattern) = pattern else {
                return Err(collection_pattern_error(subject, pattern, expected_type));
            };
            validate_list_pattern_type(
                semantic_index,
                subject,
                expected_type,
                pattern,
                element,
                capacity,
            )?;
            check_list_pattern_bindings(module, semantic_index, subject, element, capacity, pattern)
        }
        Some(CollectionType::Map {
            key,
            value,
            capacity,
        }) => {
            let Pattern::Map(pattern) = pattern else {
                return Err(collection_pattern_error(subject, pattern, expected_type));
            };
            validate_map_pattern_type(
                semantic_index,
                subject,
                expected_type,
                pattern,
                key,
                value,
                capacity,
            )?;
            check_map_pattern_bindings(
                module,
                semantic_index,
                subject,
                key,
                value,
                capacity,
                pattern,
            )
        }
        None => Err(Error::new(format!(
            "{subject} collection pattern expected List<T,N> or Map<K,V,N>, found {expected_type}"
        ))),
    }
}

pub(in crate::language::checker::source_functions) fn collection_pattern_shape(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    pattern: &Pattern,
) -> Result<CollectionPatternShape> {
    match semantic_index.collection_type(expected_type)? {
        Some(CollectionType::List { .. }) => {
            let Pattern::List(pattern) = pattern else {
                return Err(collection_pattern_error(
                    "collection pattern",
                    pattern,
                    expected_type,
                ));
            };
            Ok(CollectionPatternShape::List {
                prefix_len: pattern.elements.len(),
                completeness: list_pattern_completeness(pattern),
            })
        }
        Some(CollectionType::Map { key, .. }) => {
            let Pattern::Map(pattern) = pattern else {
                return Err(collection_pattern_error(
                    "collection pattern",
                    pattern,
                    expected_type,
                ));
            };
            let keys = canonical_map_pattern_keys(
                module,
                semantic_index,
                "collection pattern",
                key,
                pattern,
            )?;
            Ok(CollectionPatternShape::Map {
                keys,
                completeness: pattern.completeness,
            })
        }
        None => Err(Error::new(format!(
            "collection pattern expected List<T,N> or Map<K,V,N>, found {expected_type}"
        ))),
    }
}

pub(in crate::language::checker::source_functions) fn resolve_collection_pattern_value_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    function_name: &str,
    usage: &str,
    expected_type: &TypeRef,
    pattern: &Pattern,
    value: &ValueExpr,
) -> Result<Option<CollectionPatternResolution>> {
    match semantic_index.collection_type(expected_type)? {
        Some(CollectionType::List { element, capacity }) => {
            let ValueExpr::List(list) = value else {
                return Err(Error::new(format!(
                    "function {function_name} {usage} requires a concrete list value argument"
                )));
            };
            match pattern {
                Pattern::Wildcard => Ok(Some(CollectionPatternResolution {
                    substitutions: Vec::new(),
                    bindings: Vec::new(),
                })),
                Pattern::List(pattern) => {
                    if !list_pattern_matches(pattern, list) {
                        return Ok(None);
                    }
                    let subject = format!("function {function_name} {usage}");
                    validate_list_pattern_type(
                        semantic_index,
                        &subject,
                        expected_type,
                        pattern,
                        element,
                        capacity,
                    )?;
                    let bindings = check_list_pattern_bindings(
                        module,
                        semantic_index,
                        &subject,
                        element,
                        capacity,
                        pattern,
                    )?;
                    let Some(substitutions) = list_pattern_substitutions(
                        module,
                        semantic_index,
                        element,
                        capacity,
                        pattern,
                        list,
                    )?
                    else {
                        return Ok(None);
                    };
                    Ok(Some(CollectionPatternResolution {
                        substitutions,
                        bindings,
                    }))
                }
                _ => Err(collection_pattern_error(
                    &format!("function {function_name} {usage}"),
                    pattern,
                    expected_type,
                )),
            }
        }
        Some(CollectionType::Map {
            key,
            value: item,
            capacity,
        }) => {
            let ValueExpr::Map(map) = value else {
                return Err(Error::new(format!(
                    "function {function_name} {usage} requires a concrete map value argument"
                )));
            };
            match pattern {
                Pattern::Wildcard => Ok(Some(CollectionPatternResolution {
                    substitutions: Vec::new(),
                    bindings: Vec::new(),
                })),
                Pattern::Map(pattern) => {
                    let subject = format!("function {function_name} {usage}");
                    validate_map_pattern_type(
                        semantic_index,
                        &subject,
                        expected_type,
                        pattern,
                        key,
                        item,
                        capacity,
                    )?;
                    let Some(substitutions) = map_pattern_substitutions(
                        module,
                        semantic_index,
                        &subject,
                        MapPatternType {
                            key,
                            value: item,
                            capacity,
                        },
                        pattern,
                        map,
                    )?
                    else {
                        return Ok(None);
                    };
                    let bindings = check_map_pattern_bindings(
                        module,
                        semantic_index,
                        &subject,
                        key,
                        item,
                        capacity,
                        pattern,
                    )?;
                    Ok(Some(CollectionPatternResolution {
                        substitutions,
                        bindings,
                    }))
                }
                _ => Err(collection_pattern_error(
                    &format!("function {function_name} {usage}"),
                    pattern,
                    expected_type,
                )),
            }
        }
        None => Err(Error::new(format!(
            "function {function_name} {usage} expected a collection type, found {expected_type}"
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionPatternKind {
    List,
    Map,
}

fn validate_collection_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[&Function],
    kind: CollectionPatternKind,
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let collection_type = collection_pattern_type(first)?;
    validate_collection_kind(first, &collection_type, kind)?;

    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
        semantic_index,
    };
    let collection_capacity = collection_pattern_capacity(semantic_index, &collection_type)?;
    let mut seen_shapes = Vec::new();
    for function in functions {
        validate_pure_source_function_block(owner, function, source_function_block(function)?)?;
        let next_type = collection_pattern_type(function)?;
        validate_collection_kind(function, &next_type, kind)?;
        if !semantic_index.same_type(&collection_type, &next_type) {
            return Err(Error::new(format!(
                "{owner} function {} collection pattern has type {}, expected {}",
                function.name, next_type, collection_type
            )));
        }
        let FunctionParam::Pattern(pattern) = &function.params[0] else {
            return Err(Error::new(format!(
                "{owner} function {} cannot mix binding and collection pattern clauses",
                function.name
            )));
        };
        let subject = format!("{owner} function {}", function.name);
        let pattern_bindings = check_collection_pattern_bindings(
            module,
            semantic_index,
            &subject,
            &collection_type,
            pattern,
        )?;
        let shape = collection_pattern_shape(module, semantic_index, &collection_type, pattern)?;
        if let Some(overlap) =
            first_overlapping_collection_pattern(&seen_shapes, &shape, collection_capacity)
        {
            return Err(Error::new(format!(
                "{owner} function {} declares overlapping collection patterns {} and {}",
                function.name,
                collection_shape_label(overlap),
                collection_shape_label(&shape)
            )));
        }
        seen_shapes.push(shape);
        let body_bindings = pattern_bindings
            .iter()
            .map(|binding| SourceValueBinding {
                name: &binding.name,
                ty: &binding.ty,
            })
            .collect::<Vec<_>>();
        validate_source_function_body_values(&scope, function, &body_bindings)?;
    }
    Ok(())
}

fn validate_collection_kind(
    function: &Function,
    ty: &TypeRef,
    kind: CollectionPatternKind,
) -> Result<()> {
    match (kind, ty) {
        (
            CollectionPatternKind::List,
            TypeRef::Applied {
                constructor,
                args,
                const_args,
            },
        ) if constructor.as_str() == LIST_TYPE && args.len() == 1 && const_args.len() == 1 => {
            Ok(())
        }
        (
            CollectionPatternKind::Map,
            TypeRef::Applied {
                constructor,
                args,
                const_args,
            },
        ) if constructor.as_str() == MAP_TYPE && args.len() == 2 && const_args.len() == 1 => Ok(()),
        (CollectionPatternKind::List, _) => Err(Error::new(format!(
            "function {} must declare list pattern clauses",
            function.name
        ))),
        (CollectionPatternKind::Map, _) => Err(Error::new(format!(
            "function {} must declare map pattern clauses",
            function.name
        ))),
    }
}

fn list_pattern_completeness(pattern: &ListPattern) -> ListPatternCompleteness {
    if pattern.rest.is_some() {
        ListPatternCompleteness::Rest
    } else {
        ListPatternCompleteness::Exact
    }
}

fn list_pattern_matches(pattern: &ListPattern, value: &ListValue) -> bool {
    match list_pattern_completeness(pattern) {
        ListPatternCompleteness::Exact => pattern.elements.len() == value.items.len(),
        ListPatternCompleteness::Rest => value.items.len() >= pattern.elements.len(),
    }
}

fn validate_list_pattern_type(
    semantic_index: &SemanticIndex,
    subject: &str,
    expected_type: &TypeRef,
    pattern: &ListPattern,
    element_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(pattern_type) = &pattern.element_type
        && !semantic_index.same_type(pattern_type, element_type)
    {
        return Err(Error::new(format!(
            "{subject} list pattern has element type {pattern_type}, expected {element_type} for {expected_type}"
        )));
    }
    if let Some(pattern_capacity) = pattern.capacity
        && pattern_capacity != capacity
    {
        return Err(Error::new(format!(
            "{subject} list pattern has capacity {pattern_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if pattern.elements.len() > capacity {
        return Err(Error::new(format!(
            "{subject} list pattern length {} exceeds capacity {capacity} for {expected_type}",
            pattern.elements.len()
        )));
    }
    if pattern.rest.is_some() && pattern.elements.is_empty() {
        return Err(Error::new(format!(
            "{subject} list rest pattern must declare at least one prefix element"
        )));
    }
    Ok(())
}

fn validate_map_pattern_type(
    semantic_index: &SemanticIndex,
    subject: &str,
    expected_type: &TypeRef,
    pattern: &MapPattern,
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(pattern_key_type) = &pattern.key_type
        && !semantic_index.same_type(pattern_key_type, key_type)
    {
        return Err(Error::new(format!(
            "{subject} map pattern has key type {pattern_key_type}, expected {key_type} for {expected_type}"
        )));
    }
    if let Some(pattern_value_type) = &pattern.value_type
        && !semantic_index.same_type(pattern_value_type, value_type)
    {
        return Err(Error::new(format!(
            "{subject} map pattern has value type {pattern_value_type}, expected {value_type} for {expected_type}"
        )));
    }
    if let Some(pattern_capacity) = pattern.capacity
        && pattern_capacity != capacity
    {
        return Err(Error::new(format!(
            "{subject} map pattern has capacity {pattern_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if pattern.entries.len() > capacity {
        return Err(Error::new(format!(
            "{subject} map pattern entry count {} exceeds capacity {capacity} for {expected_type}",
            pattern.entries.len()
        )));
    }
    if pattern.rest.is_some() && pattern.completeness != MapPatternCompleteness::Subset {
        return Err(Error::new(format!(
            "{subject} map rest binding requires a subset map pattern"
        )));
    }
    if pattern.rest.is_some() && pattern.entries.is_empty() {
        return Err(Error::new(format!(
            "{subject} map rest pattern must declare at least one key"
        )));
    }
    if pattern.completeness == MapPatternCompleteness::Subset && pattern.entries.is_empty() {
        return Err(Error::new(format!(
            "{subject} subset map pattern must declare at least one key"
        )));
    }
    Ok(())
}

fn check_list_pattern_bindings(
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
                if !seen_bindings.insert(name.to_string()) {
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
        if !seen_bindings.insert(rest.to_string()) {
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

fn check_map_pattern_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    subject: &str,
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
    pattern: &MapPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let keys = canonical_map_pattern_keys(module, semantic_index, subject, key_type, pattern)?;
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
                if !seen_bindings.insert(name.to_string()) {
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
        if !seen_bindings.insert(rest.to_string()) {
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

fn canonical_map_pattern_keys(
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
            "{subject} map pattern keys must be static source values of type {key_type} in this source slice"
        ))
    })
}

fn list_pattern_substitutions(
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

fn map_pattern_substitutions(
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
                capacity: Some(map_type.capacity - pattern_keys.len()),
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

pub(in crate::language::checker::source_functions) fn collection_shape_label(
    shape: &CollectionPatternShape,
) -> String {
    match shape {
        CollectionPatternShape::List {
            prefix_len,
            completeness,
        } => match completeness {
            ListPatternCompleteness::Exact => format!("List exact length {prefix_len}"),
            ListPatternCompleteness::Rest => format!("List prefix length {prefix_len} with rest"),
        },
        CollectionPatternShape::Map { keys, completeness } => {
            let marker = match completeness {
                MapPatternCompleteness::Exact => "exact",
                MapPatternCompleteness::Subset => "subset",
            };
            let key_labels = keys
                .iter()
                .map(ArtifactValue::label)
                .collect::<Vec<_>>()
                .join(",");
            format!("Map {marker} keys [{key_labels}]")
        }
    }
}

pub(in crate::language::checker::source_functions) fn collection_pattern_capacity(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
) -> Result<usize> {
    match semantic_index.collection_type(expected_type)? {
        Some(CollectionType::List { capacity, .. } | CollectionType::Map { capacity, .. }) => {
            Ok(capacity)
        }
        None => Err(Error::new(format!(
            "collection pattern expected List<T,N> or Map<K,V,N>, found {expected_type}"
        ))),
    }
}

pub(in crate::language::checker::source_functions) fn first_overlapping_collection_pattern<'a>(
    existing: &'a [CollectionPatternShape],
    candidate: &CollectionPatternShape,
    capacity: usize,
) -> Option<&'a CollectionPatternShape> {
    existing
        .iter()
        .find(|shape| collection_pattern_shapes_overlap(shape, candidate, capacity))
}

fn collection_pattern_shapes_overlap(
    left: &CollectionPatternShape,
    right: &CollectionPatternShape,
    capacity: usize,
) -> bool {
    match (left, right) {
        (
            CollectionPatternShape::List {
                prefix_len: left,
                completeness: left_completeness,
            },
            CollectionPatternShape::List {
                prefix_len: right,
                completeness: right_completeness,
            },
        ) => list_pattern_shapes_overlap(
            *left,
            *left_completeness,
            *right,
            *right_completeness,
            capacity,
        ),
        (
            CollectionPatternShape::Map {
                keys: left,
                completeness: left_completeness,
            },
            CollectionPatternShape::Map {
                keys: right,
                completeness: right_completeness,
            },
        ) => map_pattern_shapes_overlap(
            left,
            *left_completeness,
            right,
            *right_completeness,
            capacity,
        ),
        _ => false,
    }
}

fn list_pattern_shapes_overlap(
    left: usize,
    left_completeness: ListPatternCompleteness,
    right: usize,
    right_completeness: ListPatternCompleteness,
    capacity: usize,
) -> bool {
    match (left_completeness, right_completeness) {
        (ListPatternCompleteness::Exact, ListPatternCompleteness::Exact) => left == right,
        (ListPatternCompleteness::Exact, ListPatternCompleteness::Rest) => left >= right,
        (ListPatternCompleteness::Rest, ListPatternCompleteness::Exact) => right >= left,
        (ListPatternCompleteness::Rest, ListPatternCompleteness::Rest) => {
            left.max(right) <= capacity
        }
    }
}

fn map_pattern_shapes_overlap(
    left: &[ArtifactValue],
    left_completeness: MapPatternCompleteness,
    right: &[ArtifactValue],
    right_completeness: MapPatternCompleteness,
    capacity: usize,
) -> bool {
    match (left_completeness, right_completeness) {
        (MapPatternCompleteness::Exact, MapPatternCompleteness::Exact) => left == right,
        (MapPatternCompleteness::Exact, MapPatternCompleteness::Subset) => {
            key_set_contains_all(left, right)
        }
        (MapPatternCompleteness::Subset, MapPatternCompleteness::Exact) => {
            key_set_contains_all(right, left)
        }
        (MapPatternCompleteness::Subset, MapPatternCompleteness::Subset) => {
            sorted_key_union_len(left, right) <= capacity
        }
    }
}

fn key_set_contains_all(keys: &[ArtifactValue], required: &[ArtifactValue]) -> bool {
    required
        .iter()
        .all(|required_key| keys.binary_search(required_key).is_ok())
}

fn sorted_key_union_len(left: &[ArtifactValue], right: &[ArtifactValue]) -> usize {
    let mut index_left = 0usize;
    let mut index_right = 0usize;
    let mut count = 0usize;
    while index_left < left.len() || index_right < right.len() {
        match (left.get(index_left), right.get(index_right)) {
            (Some(left_key), Some(right_key)) if left_key == right_key => {
                index_left += 1;
                index_right += 1;
            }
            (Some(left_key), Some(right_key)) if left_key < right_key => {
                index_left += 1;
            }
            (Some(_), Some(_)) => {
                index_right += 1;
            }
            (Some(_), None) => {
                index_left += 1;
            }
            (None, Some(_)) => {
                index_right += 1;
            }
            (None, None) => break,
        }
        count += 1;
    }
    count
}

fn map_pattern_projection(pattern: &MapPattern) -> MapProjectionMode {
    match pattern.completeness {
        MapPatternCompleteness::Exact => MapProjectionMode::Exact,
        MapPatternCompleteness::Subset => MapProjectionMode::Subset,
    }
}

fn collection_pattern_error(subject: &str, pattern: &Pattern, expected_type: &TypeRef) -> Error {
    match pattern {
        Pattern::Constructor { name, .. } => Error::new(format!(
            "{subject} pattern {name} expects an enum constructor, but scrutinee is {expected_type}"
        )),
        Pattern::Record { name, .. } => Error::new(format!(
            "{subject} pattern {name} destructures a record, but scrutinee is {expected_type}"
        )),
        Pattern::List(_) => Error::new(format!(
            "{subject} list pattern cannot match {expected_type}"
        )),
        Pattern::Map(_) => Error::new(format!(
            "{subject} map pattern cannot match {expected_type}"
        )),
        Pattern::Wildcard => Error::new(format!(
            "{subject} wildcard pattern cannot infer collection type {expected_type}"
        )),
    }
}
