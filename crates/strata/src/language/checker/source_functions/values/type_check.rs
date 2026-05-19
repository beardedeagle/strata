use super::*;

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
    if let ValueExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = value
    {
        return validate_source_function_if_else(
            scope,
            expected_type,
            condition,
            then_branch,
            else_branch,
            bindings,
        );
    }
    if let ValueExpr::Equality {
        operator,
        left,
        right,
    } = value
    {
        return validate_source_equality_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
        );
    }
    if let ValueExpr::BooleanNot { operand } = value {
        return validate_source_boolean_not_expr(scope, expected_type, operand, bindings);
    }
    if let ValueExpr::BooleanBinary {
        operator,
        left,
        right,
    } = value
    {
        return validate_source_boolean_binary_expr(
            scope,
            expected_type,
            *operator,
            left,
            right,
            bindings,
        );
    }
    if let ValueExpr::Grouped { value } = value {
        return validate_source_grouped_value_expr(scope, expected_type, value, bindings);
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
        ValueExpr::Call { .. }
        | ValueExpr::Record(_)
        | ValueExpr::List(_)
        | ValueExpr::Map(_)
        | ValueExpr::Equality { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. }
        | ValueExpr::IfElse { .. } => Err(Error::new(format!(
            "expected enum variant value for enum {}",
            enum_decl.name
        ))),
    }
}
