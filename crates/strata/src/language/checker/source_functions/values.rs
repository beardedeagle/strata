use super::collection_patterns::collection_pattern_type;
use super::record_patterns::record_pattern_type;
use super::value_resolution::{
    resolve_binding_source_function_call, resolve_collection_pattern_source_function_call,
    resolve_pattern_source_function_call, resolve_record_pattern_source_function_call,
};
use super::*;

mod body_matches;
mod return_matches;

use body_matches::validate_source_function_body_match_values;
use return_matches::validate_source_function_return_match;

pub(super) fn validate_source_function_body_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match source_function_body(function)? {
        FunctionBody::Block(body) => validate_source_function_return_expr(
            scope,
            function,
            &function.return_type,
            &body.returns,
            bindings,
        ),
        FunctionBody::Match(match_body) => validate_source_function_body_match_values(
            scope,
            function,
            &function.return_type,
            match_body,
            bindings,
        ),
    }
}

fn validate_source_function_return_expr(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    returns: &ReturnExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match returns {
        ReturnExpr::Value(value) => {
            validate_source_function_value_expr(scope, expected_type, value, bindings)
        }
        ReturnExpr::Call { name, arg } => validate_source_function_value_expr(
            scope,
            expected_type,
            &ValueExpr::Call {
                name: name.clone(),
                arg: Box::new(arg.clone()),
            },
            bindings,
        ),
        ReturnExpr::Match(match_body) => validate_source_function_return_match(
            scope,
            function,
            expected_type,
            match_body,
            bindings,
        ),
    }
}

fn validate_source_function_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match value {
        ValueExpr::Identifier(_) | ValueExpr::EnumVariant { .. } => {
            check_source_value_type(scope, expected_type, value, bindings)
        }
        ValueExpr::Call { name, arg } => {
            validate_source_function_call_or_constructor(scope, expected_type, name, arg, bindings)
        }
        ValueExpr::Record(record) => {
            let record_decl = scope
                .semantic_index
                .record_decl(scope.module, expected_type)?;
            if record.name != record_decl.name {
                return Err(Error::new(format!(
                    "expected record value {}, found {}",
                    record_decl.name, record.name
                )));
            }
            let mut seen = BTreeSet::new();
            for field in &record.fields {
                let Some(field_decl) = record_decl
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field.name)
                else {
                    return Err(Error::new(format!(
                        "record {} has no field {}",
                        record.name, field.name
                    )));
                };
                if !seen.insert(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record {} field {} is assigned more than once",
                        record.name, field.name
                    )));
                }
                validate_source_function_value_expr(scope, &field_decl.ty, &field.value, bindings)?;
            }
            for field in &record_decl.fields {
                if !seen.contains(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record {} value is missing field {}",
                        record_decl.name, field.name
                    )));
                }
            }
            Ok(())
        }
        ValueExpr::List(list) => {
            let Some(CollectionType::List { element, capacity }) =
                scope.semantic_index.collection_type(expected_type)?
            else {
                return check_source_value_type(scope, expected_type, value, bindings);
            };
            validate_list_value_type(scope.semantic_index, expected_type, list, element, capacity)?;
            for item in &list.items {
                validate_source_function_value_expr(scope, element, item, bindings)?;
            }
            Ok(())
        }
        ValueExpr::Map(map) => {
            let Some(CollectionType::Map {
                key,
                value: item,
                capacity,
            }) = scope.semantic_index.collection_type(expected_type)?
            else {
                return check_source_value_type(scope, expected_type, value, bindings);
            };
            validate_map_value_type(
                scope.semantic_index,
                expected_type,
                map,
                key,
                item,
                capacity,
            )?;
            validate_concrete_map_value_keys(scope, key, map, bindings)?;
            for entry in &map.entries {
                validate_source_function_value_expr(scope, key, &entry.key, bindings)?;
                validate_source_function_value_expr(scope, item, &entry.value, bindings)?;
            }
            Ok(())
        }
    }
}

fn validate_list_value_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    list: &ListValue,
    element_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(declared_element) = &list.element_type
        && !semantic_index.same_type(declared_element, element_type)
    {
        return Err(Error::new(format!(
            "list value has element type {declared_element}, expected {element_type} for {expected_type}"
        )));
    }
    if let Some(declared_capacity) = list.capacity
        && declared_capacity != capacity
    {
        return Err(Error::new(format!(
            "list value has capacity {declared_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if list.items.len() > capacity {
        return Err(Error::new(format!(
            "list value length {} exceeds capacity {capacity} for {expected_type}",
            list.items.len()
        )));
    }
    Ok(())
}

fn validate_map_value_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    map: &MapValue,
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(declared_key) = &map.key_type
        && !semantic_index.same_type(declared_key, key_type)
    {
        return Err(Error::new(format!(
            "map value has key type {declared_key}, expected {key_type} for {expected_type}"
        )));
    }
    if let Some(declared_value) = &map.value_type
        && !semantic_index.same_type(declared_value, value_type)
    {
        return Err(Error::new(format!(
            "map value has value type {declared_value}, expected {value_type} for {expected_type}"
        )));
    }
    if let Some(declared_capacity) = map.capacity
        && declared_capacity != capacity
    {
        return Err(Error::new(format!(
            "map value has capacity {declared_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if map.entries.len() > capacity {
        return Err(Error::new(format!(
            "map value entry count {} exceeds capacity {capacity} for {expected_type}",
            map.entries.len()
        )));
    }
    Ok(())
}

fn validate_concrete_map_value_keys(
    scope: &SourceFunctionScope<'_>,
    key_type: &TypeRef,
    map: &MapValue,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for entry in &map.entries {
        if source_value_contains_call(&entry.key)
            || source_value_uses_any_binding(&entry.key, bindings)
        {
            continue;
        }
        let key = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            key_type,
            &entry.key,
            &[],
        )?;
        if !seen.insert(key.clone()) {
            return Err(Error::new(format!(
                "map value duplicates key {}",
                key.label()
            )));
        }
    }
    Ok(())
}

fn source_value_uses_any_binding(value: &ValueExpr, bindings: &[SourceValueBinding<'_>]) -> bool {
    bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
}

fn source_value_contains_call(value: &ValueExpr) -> bool {
    match value {
        ValueExpr::Identifier(_) => false,
        ValueExpr::Call { .. } => true,
        ValueExpr::EnumVariant { payload, .. } => source_value_contains_call(payload),
        ValueExpr::Record(record) => record
            .fields
            .iter()
            .any(|field| source_value_contains_call(&field.value)),
        ValueExpr::List(list) => list.items.iter().any(source_value_contains_call),
        ValueExpr::Map(map) => map.entries.iter().any(|entry| {
            source_value_contains_call(&entry.key) || source_value_contains_call(&entry.value)
        }),
    }
}

fn validate_source_function_call_or_constructor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let constructor = enum_variant_for_expected_type(scope, expected_type, name)?;
    let functions = source_function_group_option(scope, name)?;
    if constructor.is_some() && functions.is_some() {
        return Err(Error::new(format!(
            "value expression {name}(...) is ambiguous between an enum constructor and source function"
        )));
    }
    if constructor.is_some() {
        return validate_source_enum_payload_value(scope, expected_type, name, arg, bindings);
    }
    let Some(functions) = functions else {
        if identifier_starts_uppercase(name)
            && let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    validate_source_function_call(scope, expected_type, name, arg, bindings, &functions)
}

fn validate_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    functions: &[&Function],
) -> Result<()> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new(format!("function {name} is not declared")))?;
    if !scope
        .semantic_index
        .same_type(&first.return_type, expected_type)
    {
        return Err(Error::new(format!(
            "function {name} returns {}, expected {}",
            first.return_type, expected_type
        )));
    }
    match source_function_param_kind(first)? {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate binding clauses"
                )));
            }
            let FunctionParam::Binding(param) = &first.params[0] else {
                return Err(Error::new(format!(
                    "function {name} must declare a binding parameter"
                )));
            };
            validate_source_function_value_expr(scope, &param.ty, arg, bindings)
        }
        SourceFunctionParamKind::EnumPattern => {
            let enum_type = infer_pattern_function_enum_type(
                scope.module,
                scope.semantic_index,
                "source",
                functions,
            )?;
            validate_source_function_value_expr(scope, &enum_type, arg, bindings)
        }
        SourceFunctionParamKind::RecordPattern => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate record pattern clauses"
                )));
            }
            let record_type = record_pattern_type(first)?;
            validate_source_function_value_expr(scope, &record_type, arg, bindings)
        }
        SourceFunctionParamKind::ListPattern | SourceFunctionParamKind::MapPattern => {
            let collection_type = collection_pattern_type(first)?;
            validate_source_function_value_expr(scope, &collection_type, arg, bindings)
        }
    }
}

fn validate_source_enum_payload_value(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    payload: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let variant = enum_variant_for_expected_type(scope, expected_type, name)?
        .ok_or_else(|| enum_value_error(scope, expected_type, name))?;
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    validate_source_function_value_expr(scope, payload_type, payload, bindings)
}

pub(in crate::language::checker) fn resolve_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }

    match value {
        ValueExpr::Identifier(_) => Ok(value.clone()),
        ValueExpr::Call { name, arg } => {
            resolve_source_call_or_constructor(scope, expected_type, name, arg, bindings, depth + 1)
        }
        ValueExpr::EnumVariant { name, payload } => resolve_source_enum_payload_value(
            scope,
            expected_type,
            name,
            payload,
            bindings,
            depth + 1,
        ),
        ValueExpr::Record(record) => {
            resolve_record_source_value_expr(scope, expected_type, record, bindings, depth + 1)
        }
        ValueExpr::List(list) => {
            resolve_list_source_value_expr(scope, expected_type, list, bindings, depth + 1)
        }
        ValueExpr::Map(map) => {
            resolve_map_source_value_expr(scope, expected_type, map, bindings, depth + 1)
        }
    }
}

fn resolve_list_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    list: &ListValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Some(CollectionType::List { element, .. }) =
        scope.semantic_index.collection_type(expected_type)?
    else {
        return Ok(ValueExpr::List(list.clone()));
    };
    let items = list
        .items
        .iter()
        .map(|item| resolve_source_value_expr(scope, element, item, bindings, depth + 1))
        .collect::<Result<Vec<_>>>()?;
    Ok(ValueExpr::List(ListValue {
        element_type: list.element_type.clone(),
        capacity: list.capacity,
        items,
    }))
}

fn resolve_map_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    map: &MapValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Some(CollectionType::Map { key, value, .. }) =
        scope.semantic_index.collection_type(expected_type)?
    else {
        return Ok(ValueExpr::Map(map.clone()));
    };
    let entries = map
        .entries
        .iter()
        .map(|entry| {
            Ok(MapValueEntry {
                key: resolve_source_value_expr(scope, key, &entry.key, bindings, depth + 1)?,
                value: resolve_source_value_expr(scope, value, &entry.value, bindings, depth + 1)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ValueExpr::Map(MapValue {
        key_type: map.key_type.clone(),
        value_type: map.value_type.clone(),
        capacity: map.capacity,
        entries,
    }))
}

fn resolve_record_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    record: &RecordValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Ok(record_decl) = scope
        .semantic_index
        .record_decl(scope.module, expected_type)
    else {
        return Ok(ValueExpr::Record(record.clone()));
    };
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let Some(field_decl) = record_decl
            .fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            fields.push(field.clone());
            continue;
        };
        fields.push(RecordValueField {
            name: field.name.clone(),
            value: resolve_source_value_expr(
                scope,
                &field_decl.ty,
                &field.value,
                bindings,
                depth + 1,
            )?,
        });
    }
    Ok(ValueExpr::Record(RecordValue {
        name: record.name.clone(),
        fields,
    }))
}

fn enum_variant_for_expected_type<'module>(
    scope: &SourceFunctionScope<'module>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<&'module EnumVariant>> {
    let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type) else {
        return Ok(None);
    };
    Ok(enum_decl
        .variants
        .iter()
        .find(|variant| variant.name == *name))
}

fn enum_value_error(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Error {
    match scope.semantic_index.enum_decl(scope.module, expected_type) {
        Ok(enum_decl) => Error::new(format!(
            "value {name} is not a variant of enum {}",
            enum_decl.name
        )),
        Err(_) => Error::new(format!(
            "value {name} cannot construct non-enum value of type {expected_type}"
        )),
    }
}

fn identifier_starts_uppercase(name: &Identifier) -> bool {
    name.as_str()
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn resolve_source_call_or_constructor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let constructor = enum_variant_for_expected_type(scope, expected_type, name)?;
    let functions = source_function_group_option(scope, name)?;
    if constructor.is_some() && functions.is_some() {
        return Err(Error::new(format!(
            "value expression {name}(...) is ambiguous between an enum constructor and source function"
        )));
    }
    if constructor.is_some() {
        return resolve_source_enum_payload_value(scope, expected_type, name, arg, bindings, depth);
    }
    let Some(functions) = functions else {
        if identifier_starts_uppercase(name)
            && let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    resolve_source_function_call(scope, expected_type, name, arg, bindings, depth, &functions)
}

fn resolve_source_enum_payload_value(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    payload: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let variant = enum_variant_for_expected_type(scope, expected_type, name)?
        .ok_or_else(|| enum_value_error(scope, expected_type, name))?;
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    let payload = resolve_source_value_expr(scope, payload_type, payload, bindings, depth + 1)?;
    if scope
        .semantic_index
        .process_ref_target_type(payload_type)?
        .is_none()
    {
        check_source_value_type(scope, payload_type, &payload, bindings)?;
    }
    Ok(ValueExpr::EnumVariant {
        name: name.clone(),
        payload: Box::new(payload),
    })
}

fn resolve_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
    functions: &[&Function],
) -> Result<ValueExpr> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new(format!("function {name} is not declared")))?;
    if !scope
        .semantic_index
        .same_type(&first.return_type, expected_type)
    {
        return Err(Error::new(format!(
            "function {name} returns {}, expected {}",
            first.return_type, expected_type
        )));
    }

    match source_function_param_kind(first)? {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate binding clauses"
                )));
            }
            resolve_binding_source_function_call(
                scope,
                expected_type,
                first,
                arg,
                bindings,
                depth + 1,
            )
        }
        SourceFunctionParamKind::EnumPattern => resolve_pattern_source_function_call(
            scope,
            expected_type,
            functions,
            arg,
            bindings,
            depth + 1,
        ),
        SourceFunctionParamKind::RecordPattern => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate record pattern clauses"
                )));
            }
            resolve_record_pattern_source_function_call(
                scope,
                expected_type,
                first,
                arg,
                bindings,
                depth + 1,
            )
        }
        SourceFunctionParamKind::ListPattern | SourceFunctionParamKind::MapPattern => {
            resolve_collection_pattern_source_function_call(
                scope,
                expected_type,
                functions,
                arg,
                bindings,
                depth + 1,
            )
        }
    }
}

fn source_function_group_option<'a>(
    scope: &SourceFunctionScope<'a>,
    name: &Identifier,
) -> Result<Option<Vec<&'a Function>>> {
    let local: Vec<_> = scope
        .process_functions
        .iter()
        .filter(|function| function.name == *name)
        .collect();
    let module: Vec<_> = scope
        .module
        .functions
        .iter()
        .filter(|function| function.name == *name)
        .collect();

    match (local.is_empty(), module.is_empty()) {
        (false, false) => Err(Error::new(format!(
            "{} function {name} conflicts with module function {name}",
            source_function_scope_label(scope)
        ))),
        (false, true) => Ok(Some(local)),
        (true, false) => Ok(Some(module)),
        (true, true) => Ok(None),
    }
}

fn source_function_scope_label(scope: &SourceFunctionScope<'_>) -> String {
    scope
        .process_name
        .map(|name| format!("process {name}"))
        .unwrap_or_else(|| "module".to_string())
}

pub(in crate::language::checker) fn check_source_value_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    check_source_value_type_inner(scope, expected_type, value, bindings, 0)
}

fn check_source_value_type_inner(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<()> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if scope
        .semantic_index
        .process_ref_target_type(expected_type)?
        .is_some()
    {
        return Err(Error::new(
            "process references must be direct message payloads",
        ));
    }
    if !source_value_uses_any_binding(value, bindings) {
        canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            expected_type,
            value,
            &[],
        )?;
        return Ok(());
    }
    if let ValueExpr::Identifier(name) = value
        && let Some(binding) = bindings.iter().find(|binding| binding.name == name)
    {
        if scope.semantic_index.same_type(binding.ty, expected_type) {
            return Ok(());
        }
        return Err(Error::new(format!(
            "value binding {} has type {}, expected {}",
            binding.name, binding.ty, expected_type
        )));
    }
    if let ValueExpr::Call { name, .. } = value {
        return Err(Error::new(format!(
            "function call {name} must be resolved before checking value of type {expected_type}"
        )));
    }
    if let Ok(record) = scope
        .semantic_index
        .record_decl(scope.module, expected_type)
    {
        return check_record_source_value_type(scope, record, value, bindings, depth);
    }
    if scope
        .semantic_index
        .collection_type(expected_type)?
        .is_some()
    {
        return check_collection_source_value_type(scope, expected_type, value, bindings, depth);
    }
    check_enum_source_value_type(scope, expected_type, value, bindings, depth)
}

fn check_record_source_value_type(
    scope: &SourceFunctionScope<'_>,
    record_decl: &Record,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<()> {
    if let ValueExpr::Record(record) = value
        && record.fields.is_empty()
    {
        return Err(Error::new(format!(
            "fieldless record values use `{}`; braced record values must declare at least one field",
            record.name
        )));
    }
    if record_decl.fields.is_empty() {
        return match value {
            ValueExpr::Identifier(name) if name == &record_decl.name => Ok(()),
            _ => Err(Error::new(format!(
                "provided value is not a value of record {}",
                record_decl.name
            ))),
        };
    }
    let ValueExpr::Record(record) = value else {
        return Err(Error::new(format!(
            "record state type {} must be constructed with {} {{ ... }}",
            record_decl.name, record_decl.name
        )));
    };
    if record.name != record_decl.name {
        return Err(Error::new(format!(
            "record constructor {} does not match expected record {}",
            record.name, record_decl.name
        )));
    }
    let declared_fields = record_decl
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut provided = BTreeMap::new();
    for field in &record.fields {
        if provided.insert(field.name.as_str(), &field.value).is_some() {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        if !declared_fields.contains(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} declares unknown field {}",
                record.name, field.name
            )));
        }
    }
    for field in &record_decl.fields {
        let Some(value) = provided.get(field.name.as_str()) else {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record_decl.name, field.name
            )));
        };
        check_source_value_type_inner(scope, &field.ty, value, bindings, depth + 1)?;
    }
    Ok(())
}

fn check_collection_source_value_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<()> {
    match scope
        .semantic_index
        .collection_type(expected_type)?
        .ok_or_else(|| Error::new(format!("type {expected_type} is not a collection type")))?
    {
        CollectionType::List { element, capacity } => {
            let ValueExpr::List(list) = value else {
                return Err(Error::new(format!(
                    "list value type {expected_type} must be constructed with List<T,N>[...]"
                )));
            };
            validate_list_value_type(scope.semantic_index, expected_type, list, element, capacity)?;
            for item in &list.items {
                check_source_value_type_inner(scope, element, item, bindings, depth + 1)?;
            }
            Ok(())
        }
        CollectionType::Map {
            key,
            value: item,
            capacity,
        } => {
            let ValueExpr::Map(map) = value else {
                return Err(Error::new(format!(
                    "map value type {expected_type} must be constructed with Map<K,V,N>[...]"
                )));
            };
            validate_map_value_type(
                scope.semantic_index,
                expected_type,
                map,
                key,
                item,
                capacity,
            )?;
            validate_concrete_map_value_keys(scope, key, map, bindings)?;
            for entry in &map.entries {
                check_source_value_type_inner(scope, key, &entry.key, bindings, depth + 1)?;
                check_source_value_type_inner(scope, item, &entry.value, bindings, depth + 1)?;
            }
            Ok(())
        }
    }
}

fn check_enum_source_value_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<()> {
    let enum_decl = scope
        .semantic_index
        .enum_decl(scope.module, expected_type)?;
    match value {
        ValueExpr::Identifier(name) => {
            let Some(variant) = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name == *name)
            else {
                return Err(Error::new(format!(
                    "value {name} is not a variant of enum {}",
                    enum_decl.name
                )));
            };
            if variant.payload_type.is_some() {
                return Err(Error::new(format!(
                    "enum variant {} requires a payload and cannot be used as a fieldless value",
                    variant.name
                )));
            }
            Ok(())
        }
        ValueExpr::EnumVariant { name, payload } => {
            let variant = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or_else(|| {
                    Error::new(format!(
                        "value {name} is not a variant of enum {}",
                        enum_decl.name
                    ))
                })?;
            let Some(payload_type) = &variant.payload_type else {
                return Err(Error::new(format!(
                    "enum variant {name} does not accept a payload"
                )));
            };
            check_source_value_type_inner(scope, payload_type, payload, bindings, depth + 1)
        }
        ValueExpr::Call { .. } | ValueExpr::Record(_) | ValueExpr::List(_) | ValueExpr::Map(_) => {
            Err(Error::new(format!(
                "expected enum variant value for enum {}",
                enum_decl.name
            )))
        }
    }
}
