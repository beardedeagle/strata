use super::*;
use crate::language::{LIST_TYPE, MAP_TYPE};

mod resolution;
mod shapes;

pub(in crate::language::checker::source_functions) use resolution::source_nested_pattern_substitutions;
use resolution::{
    canonical_map_pattern_keys, check_list_pattern_bindings, check_map_pattern_bindings,
    list_pattern_substitutions, map_pattern_substitutions,
};
pub(in crate::language::checker::source_functions) use shapes::{
    collection_pattern_capacity, collection_shape_label, first_overlapping_collection_pattern,
};

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
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
    functions: &[&Function],
) -> Result<()> {
    validate_collection_pattern_source_function_group(
        module,
        semantic_index,
        owner,
        process,
        process_refs,
        functions,
        CollectionPatternKind::List,
    )
}

pub(in crate::language::checker::source_functions) fn validate_map_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
    functions: &[&Function],
) -> Result<()> {
    validate_collection_pattern_source_function_group(
        module,
        semantic_index,
        owner,
        process,
        process_refs,
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
    process_refs: Option<&BTreeMap<Identifier, CheckedProcessId>>,
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
        process_refs,
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
        validate_source_pattern_binding_scope_conflicts(
            &scope,
            &format!(
                "{owner} function {} collection pattern binding",
                function.name
            ),
            &pattern_bindings,
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
