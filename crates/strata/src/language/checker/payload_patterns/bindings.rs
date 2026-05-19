use super::*;

mod collections;

pub(in crate::language::checker) use collections::list_element_binding_segment;
use collections::{
    check_list_payload_pattern_bindings, check_map_payload_pattern_bindings,
    validate_list_payload_pattern_capacity, validate_map_payload_pattern_capacity,
};

pub(in crate::language::checker) fn map_rest_type(
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
    excluded_key_count: usize,
) -> Result<TypeRef> {
    let rest_capacity = capacity.checked_sub(excluded_key_count).ok_or_else(|| {
        Error::new(format!(
            "map rest binding excludes {excluded_key_count} keys from capacity {capacity}"
        ))
    })?;
    Ok(TypeRef::Applied {
        constructor: Identifier::new(MAP_TYPE)?,
        args: vec![key_type.clone(), value_type.clone()],
        const_args: vec![rest_capacity],
    })
}

pub(in crate::language::checker) fn list_rest_type(
    element_type: &TypeRef,
    capacity: usize,
    prefix_len: usize,
) -> Result<TypeRef> {
    let rest_capacity = capacity.checked_sub(prefix_len).ok_or_else(|| {
        Error::new(format!(
            "list rest binding removes {prefix_len} prefix elements from capacity {capacity}"
        ))
    })?;
    Ok(TypeRef::Applied {
        constructor: Identifier::new(LIST_TYPE)?,
        args: vec![element_type.clone()],
        const_args: vec![rest_capacity],
    })
}

pub(in crate::language::checker) fn check_pattern_payload_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    payload: Option<&ConstructorPayloadPattern>,
    context: &str,
    payload_context: PatternPayloadContext,
    binding_context: PatternBindingContext<'_>,
) -> Result<Vec<PatternPayloadParam>> {
    match (&variant.payload_type, payload) {
        (None, None) => Ok(Vec::new()),
        (None, Some(_)) => {
            let noun = match payload_context {
                PatternPayloadContext::StepPattern => "message",
                PatternPayloadContext::SourceValue => "pattern",
            };
            let subject = pattern_binding_subject(binding_context);
            Err(Error::new(format!(
                "{subject} {context} {noun} {} does not carry a payload",
                variant.name
            )))
        }
        (Some(_), None) => Ok(Vec::new()),
        (Some(payload_type), Some(ConstructorPayloadPattern::Binding(binding))) => {
            check_whole_payload_binding(
                semantic_index,
                binding_context,
                binding,
                payload_type,
                context,
            )
        }
        (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
            check_destructured_payload_bindings(
                module,
                semantic_index,
                binding_context,
                context,
                payload_type,
                pattern,
            )
        }
    }
}

pub(in crate::language::checker) fn check_pattern_payload_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    payload: Option<&ConstructorPayloadPattern>,
) -> Result<Option<PatternPayloadGuard>> {
    let Some(payload_type) = &variant.payload_type else {
        return Ok(None);
    };
    let Some(ConstructorPayloadPattern::Destructure(pattern)) = payload else {
        return Ok(None);
    };
    nested_pattern_payload_guard(module, semantic_index, payload_type, pattern)
}

fn nested_pattern_payload_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    pattern: &Pattern,
) -> Result<Option<PatternPayloadGuard>> {
    let Pattern::Constructor { name, payload } = pattern else {
        return Ok(None);
    };
    let enum_decl = semantic_index
        .enum_decl(module, expected_type)
        .map_err(|_| {
            Error::new(format!(
                "nested constructor pattern {name} cannot match value type {expected_type}"
            ))
        })?;
    let variant_index = semantic_index.enum_variant_index(module, expected_type, name)?;
    let variant = &enum_decl.variants[variant_index];
    let variant_id = CheckedEnumVariantId::from_index(variant_index)?;
    let payload_guard = match (&variant.payload_type, payload) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(Error::new(format!(
                "nested constructor pattern {name} does not carry a payload"
            )));
        }
        (Some(_), None) => {
            return Err(Error::new(format!(
                "nested constructor pattern {name} requires a payload pattern"
            )));
        }
        (Some(_), Some(ConstructorPayloadPattern::Binding(_))) => None,
        (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
            nested_pattern_payload_guard(module, semantic_index, payload_type, pattern)?
                .map(Box::new)
        }
    };
    Ok(Some(PatternPayloadGuard {
        enum_ty: expected_type.clone(),
        variant: variant_id,
        payload: payload_guard,
    }))
}

fn check_whole_payload_binding(
    semantic_index: &SemanticIndex,
    binding_context: PatternBindingContext<'_>,
    binding: &Param,
    payload_type: &TypeRef,
    context: &str,
) -> Result<Vec<PatternPayloadParam>> {
    validate_pattern_binding_name(binding_context, semantic_index, &binding.name)?;
    if !semantic_index.same_type(&binding.ty, payload_type) {
        let subject = pattern_binding_subject(binding_context);
        return Err(Error::new(format!(
            "{subject} {context} payload {} has type {}, expected {}",
            binding.name, binding.ty, payload_type
        )));
    }
    Ok(vec![PatternPayloadParam {
        name: binding.name.clone(),
        ty: binding.ty.clone(),
        path: PayloadBindingPath::whole(),
    }])
}

fn check_destructured_payload_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    binding_context: PatternBindingContext<'_>,
    context: &str,
    payload_type: &TypeRef,
    pattern: &Pattern,
) -> Result<Vec<PatternPayloadParam>> {
    let subject = pattern_binding_subject(binding_context);
    let mut seen_bindings = BTreeSet::new();
    let base_path = PayloadBindingPath::whole();
    let mut nested_scope = NestedPatternBindingScope {
        module,
        semantic_index,
        binding_context,
        context,
        seen_bindings: &mut seen_bindings,
    };
    match pattern {
        Pattern::Record { name, fields } => {
            let record = semantic_index.record_decl(module, payload_type).map_err(|_| {
                Error::new(format!(
                    "{subject} {context} record payload pattern {name} cannot match payload type {payload_type}"
                ))
            })?;
            if record.name != *name {
                return Err(Error::new(format!(
                    "{subject} {context} record payload pattern {name} cannot match record {}",
                    record.name
                )));
            }
            check_record_payload_pattern_bindings(&mut nested_scope, record, fields, &base_path)
        }
        Pattern::List(pattern) => {
            let Some(CollectionType::List { element, capacity }) =
                semantic_index.collection_type(payload_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {context} list payload pattern cannot match payload type {payload_type}"
                )));
            };
            if let Some(pattern_type) = &pattern.element_type
                && !semantic_index.same_type(pattern_type, element)
            {
                return Err(Error::new(format!(
                    "{subject} {context} list payload pattern has element type {pattern_type}, expected {element}"
                )));
            }
            validate_list_payload_pattern_capacity(
                &subject,
                context,
                payload_type,
                pattern,
                capacity,
            )?;
            check_list_payload_pattern_bindings(
                &mut nested_scope,
                element,
                capacity,
                pattern,
                &base_path,
            )
        }
        Pattern::Map(pattern) => {
            let Some(CollectionType::Map {
                key,
                value,
                capacity,
            }) = semantic_index.collection_type(payload_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {context} map payload pattern cannot match payload type {payload_type}"
                )));
            };
            if let Some(pattern_key_type) = &pattern.key_type
                && !semantic_index.same_type(pattern_key_type, key)
            {
                return Err(Error::new(format!(
                    "{subject} {context} map payload pattern has key type {pattern_key_type}, expected {key}"
                )));
            }
            if let Some(pattern_value_type) = &pattern.value_type
                && !semantic_index.same_type(pattern_value_type, value)
            {
                return Err(Error::new(format!(
                    "{subject} {context} map payload pattern has value type {pattern_value_type}, expected {value}"
                )));
            }
            validate_map_payload_pattern_capacity(
                &subject,
                context,
                payload_type,
                pattern,
                capacity,
            )?;
            check_map_payload_pattern_bindings(
                &mut nested_scope,
                MapPatternType {
                    key,
                    value,
                    capacity,
                },
                pattern,
                &base_path,
            )
        }
        Pattern::Constructor { .. } => check_constructor_payload_pattern_bindings(
            &mut nested_scope,
            payload_type,
            pattern,
            &base_path,
            EmptyConstructorPattern::Allow,
        ),
        Pattern::Wildcard => Ok(Vec::new()),
    }
}

pub(in crate::language::checker) fn check_nested_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    expected_type: &TypeRef,
    pattern: &Pattern,
    base_path: &PayloadBindingPath,
    empty_constructor: EmptyConstructorPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let subject = scope.subject();
    match pattern {
        Pattern::Record { name, fields } => {
            let record = scope
                .semantic_index
                .record_decl(scope.module, expected_type)
                .map_err(|_| {
                    Error::new(format!(
                        "{subject} {} nested record pattern {name} cannot match value type {expected_type}",
                        scope.context
                    ))
                })?;
            if record.name != *name {
                return Err(Error::new(format!(
                    "{subject} {} nested record pattern {name} cannot match record {}",
                    scope.context, record.name
                )));
            }
            check_record_payload_pattern_bindings(scope, record, fields, base_path)
        }
        Pattern::List(pattern) => {
            let Some(CollectionType::List { element, capacity }) =
                scope.semantic_index.collection_type(expected_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {} nested list pattern cannot match value type {expected_type}",
                    scope.context
                )));
            };
            if let Some(pattern_type) = &pattern.element_type
                && !scope.semantic_index.same_type(pattern_type, element)
            {
                return Err(Error::new(format!(
                    "{subject} {} nested list pattern has element type {pattern_type}, expected {element}",
                    scope.context
                )));
            }
            validate_list_payload_pattern_capacity(
                &subject,
                scope.context,
                expected_type,
                pattern,
                capacity,
            )?;
            check_list_payload_pattern_bindings(scope, element, capacity, pattern, base_path)
        }
        Pattern::Map(pattern) => {
            let Some(CollectionType::Map {
                key,
                value,
                capacity,
            }) = scope.semantic_index.collection_type(expected_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {} nested map pattern cannot match value type {expected_type}",
                    scope.context
                )));
            };
            if let Some(pattern_key_type) = &pattern.key_type
                && !scope.semantic_index.same_type(pattern_key_type, key)
            {
                return Err(Error::new(format!(
                    "{subject} {} nested map pattern has key type {pattern_key_type}, expected {key}",
                    scope.context
                )));
            }
            if let Some(pattern_value_type) = &pattern.value_type
                && !scope.semantic_index.same_type(pattern_value_type, value)
            {
                return Err(Error::new(format!(
                    "{subject} {} nested map pattern has value type {pattern_value_type}, expected {value}",
                    scope.context
                )));
            }
            validate_map_payload_pattern_capacity(
                &subject,
                scope.context,
                expected_type,
                pattern,
                capacity,
            )?;
            check_map_payload_pattern_bindings(
                scope,
                MapPatternType {
                    key,
                    value,
                    capacity,
                },
                pattern,
                base_path,
            )
        }
        Pattern::Constructor { .. } => check_constructor_payload_pattern_bindings(
            scope,
            expected_type,
            pattern,
            base_path,
            empty_constructor,
        ),
        Pattern::Wildcard => Ok(Vec::new()),
    }
}

fn check_constructor_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    enum_type: &TypeRef,
    pattern: &Pattern,
    base_path: &PayloadBindingPath,
    empty_constructor: EmptyConstructorPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let Pattern::Constructor { name, payload } = pattern else {
        return Err(Error::new("expected constructor payload pattern"));
    };
    let subject = scope.subject();
    let enum_decl = scope
        .semantic_index
        .enum_decl(scope.module, enum_type)
        .map_err(|_| {
            Error::new(format!(
                "{subject} {} nested constructor pattern {name} cannot match value type {enum_type}",
                scope.context
            ))
        })?;
    let variant_index = scope
        .semantic_index
        .enum_variant_index(scope.module, enum_type, name)?;
    let variant = &enum_decl.variants[variant_index];
    let variant_id = CheckedEnumVariantId::from_index(variant_index)?;
    match (&variant.payload_type, payload) {
        (None, None) => match empty_constructor {
            EmptyConstructorPattern::Allow => Ok(Vec::new()),
            EmptyConstructorPattern::Reject => Err(Error::new(format!(
                "{subject} {} nested constructor pattern {name} must bind at least one nested value",
                scope.context
            ))),
        },
        (None, Some(_)) => Err(Error::new(format!(
            "{subject} {} nested constructor pattern {name} does not carry a payload",
            scope.context
        ))),
        (Some(_), None) => Err(Error::new(format!(
            "{subject} {} nested constructor pattern {name} requires a payload pattern",
            scope.context
        ))),
        (Some(payload_type), Some(ConstructorPayloadPattern::Binding(binding))) => {
            validate_pattern_binding_name(
                scope.binding_context,
                scope.semantic_index,
                &binding.name,
            )?;
            if !scope.semantic_index.same_type(&binding.ty, payload_type) {
                return Err(Error::new(format!(
                    "{subject} {} nested constructor payload {} has type {}, expected {}",
                    scope.context, binding.name, binding.ty, payload_type
                )));
            }
            if scope
                .semantic_index
                .process_ref_target_type(payload_type)?
                .is_some()
            {
                return Err(Error::new(format!(
                    "{subject} {} nested constructor payload {} cannot bind process reference payload type {}; process references must be direct message payload bindings",
                    scope.context, binding.name, payload_type
                )));
            }
            add_pattern_payload_binding(
                &subject,
                scope.seen_bindings,
                PatternPayloadParam {
                    name: binding.name.clone(),
                    ty: binding.ty.clone(),
                    path: base_path.then(PayloadProjectionSegment::enum_payload(
                        enum_type.clone(),
                        payload_type.clone(),
                        variant_id,
                    )),
                },
            )
        }
        (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
            let nested_path = base_path.then(PayloadProjectionSegment::enum_payload(
                enum_type.clone(),
                payload_type.clone(),
                variant_id,
            ));
            check_nested_pattern_bindings(
                scope,
                payload_type,
                pattern,
                &nested_path,
                EmptyConstructorPattern::Allow,
            )
        }
    }
}

fn add_pattern_payload_binding(
    subject: &str,
    seen_bindings: &mut BTreeSet<String>,
    binding: PatternPayloadParam,
) -> Result<Vec<PatternPayloadParam>> {
    if !seen_bindings.insert(binding.name.to_string()) {
        return Err(Error::new(format!(
            "{subject} payload binding {} is declared more than once",
            binding.name
        )));
    }
    Ok(vec![binding])
}

fn check_record_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    record: &Record,
    fields: &[RecordPatternField],
    base_path: &PayloadBindingPath,
) -> Result<Vec<PatternPayloadParam>> {
    if fields.is_empty() {
        let subject = scope.subject();
        return Err(Error::new(format!(
            "{subject} {} record payload pattern {} must bind at least one field",
            scope.context, record.name
        )));
    }

    let mut seen_fields = BTreeSet::new();
    let mut bindings = Vec::with_capacity(fields.len());
    for field in fields {
        let subject = scope.subject();
        if !seen_fields.insert(field.field.as_str()) {
            return Err(Error::new(format!(
                "{subject} {} record payload pattern {} binds field {} more than once",
                scope.context, record.name, field.field
            )));
        }
        let Some(field_decl) = record
            .fields
            .iter()
            .find(|candidate| candidate.name == field.field)
        else {
            return Err(Error::new(format!(
                "{subject} {} record payload pattern {} has no field {}",
                scope.context, record.name, field.field
            )));
        };
        if !scope.seen_bindings.insert(field.binding.to_string()) {
            return Err(Error::new(format!(
                "{subject} {} payload binding {} is declared more than once",
                scope.context, field.binding
            )));
        }
        validate_pattern_binding_name(scope.binding_context, scope.semantic_index, &field.binding)?;
        bindings.push(PatternPayloadParam {
            name: field.binding.clone(),
            ty: field_decl.ty.clone(),
            path: base_path.then(PayloadProjectionSegment::record_field(
                field_decl.ty.clone(),
                field.field.clone(),
            )),
        });
    }
    Ok(bindings)
}
