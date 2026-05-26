use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactValue, MAX_STATE_VALUES_PER_PROCESS,
    validate_state_value_label,
};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{
    Identifier, ListValue, MapValue, Module, Record, TypeRef, ValueExpr,
};
use super::super::super::checked::CheckedStateValue;
use super::super::super::diagnostic::{Error, Result};
use super::super::STEP_STATE_PARAMETER_NAME;
use super::super::symbols::{CollectionType, SemanticIndex};

mod scalars;

use scalars::{canonical_scalar_arithmetic_value, canonical_scalar_ordering_value};

pub(in crate::language::checker) struct ValueBinding<'a> {
    pub(in crate::language::checker) name: &'a Identifier,
    pub(in crate::language::checker) ty: &'a TypeRef,
    pub(in crate::language::checker) label: String,
    pub(in crate::language::checker) value: Option<ArtifactValue>,
}

#[derive(Clone, Copy)]
pub(super) struct CanonicalValueScope<'a, 'binding> {
    pub(super) module: &'a Module,
    pub(super) semantic_index: &'a SemanticIndex,
    pub(super) bindings: &'a [ValueBinding<'binding>],
    pub(super) context: CanonicalValueContext,
}

#[derive(Clone, Copy)]
pub(super) enum CanonicalValueContext {
    StateValue,
    SourceValue,
}

impl CanonicalValueContext {
    fn process_ref_error(self) -> Error {
        match self {
            Self::StateValue => Error::new("process reference payloads are not valid state values"),
            Self::SourceValue => Error::new("process references must be direct message payloads"),
        }
    }
}

pub(in crate::language::checker) fn canonical_source_value_with_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
) -> Result<ArtifactValue> {
    canonical_value(
        module,
        semantic_index,
        expected_type,
        value,
        bindings,
        CanonicalValueContext::SourceValue,
        0,
    )
}

pub(in crate::language::checker) fn source_value_uses_binding(
    value: &ValueExpr,
    binding: &Identifier,
) -> bool {
    match value {
        ValueExpr::Identifier(name) => name == binding,
        ValueExpr::ScalarLiteral(_) => false,
        ValueExpr::Call { arg, .. } => source_value_uses_binding(arg, binding),
        ValueExpr::EnumVariant { payload, .. } => source_value_uses_binding(payload, binding),
        ValueExpr::Record(record) => record
            .fields
            .iter()
            .any(|field| source_value_uses_binding(&field.value, binding)),
        ValueExpr::List(list) => list
            .items
            .iter()
            .any(|item| source_value_uses_binding(item, binding)),
        ValueExpr::Map(map) => map.entries.iter().any(|entry| {
            source_value_uses_binding(&entry.key, binding)
                || source_value_uses_binding(&entry.value, binding)
        }),
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => {
            source_value_uses_binding(condition, binding)
                || source_value_uses_binding(then_branch, binding)
                || source_value_uses_binding(else_branch, binding)
        }
        ValueExpr::Equality { left, right, .. } => {
            source_value_uses_binding(left, binding) || source_value_uses_binding(right, binding)
        }
        ValueExpr::ScalarArithmetic { left, right, .. }
        | ValueExpr::ScalarOrdering { left, right, .. } => {
            source_value_uses_binding(left, binding) || source_value_uses_binding(right, binding)
        }
        ValueExpr::BooleanNot { operand } => source_value_uses_binding(operand, binding),
        ValueExpr::BooleanBinary { left, right, .. } => {
            source_value_uses_binding(left, binding) || source_value_uses_binding(right, binding)
        }
        ValueExpr::Grouped { value } => source_value_uses_binding(value, binding),
    }
}

pub(super) fn canonical_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<ArtifactValue> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if semantic_index
        .process_ref_target_type(expected_type)?
        .is_some()
    {
        return Err(context.process_ref_error());
    }
    if let ValueExpr::Identifier(name) = value {
        if let Some(binding) = bindings.iter().find(|binding| binding.name == name) {
            if semantic_index.same_type(binding.ty, expected_type) {
                return binding
                    .value
                    .clone()
                    .ok_or_else(|| context.process_ref_error());
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
    }
    if let ValueExpr::Call { name, .. } = value {
        return Err(Error::new(format!(
            "function call {name} must be resolved before checking value of type {expected_type}"
        )));
    }
    let scope = CanonicalValueScope {
        module,
        semantic_index,
        bindings,
        context,
    };
    if let ValueExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = value
    {
        return canonical_if_else_value(
            scope,
            expected_type,
            condition,
            then_branch,
            else_branch,
            depth,
        );
    }
    if matches!(value, ValueExpr::Equality { .. }) {
        return Err(Error::new(format!(
            "equality expression must be resolved before checking value of type {expected_type}"
        )));
    }
    if let ValueExpr::ScalarArithmetic {
        operator,
        left,
        right,
    } = value
    {
        return canonical_scalar_arithmetic_value(
            scope,
            expected_type,
            *operator,
            left,
            right,
            depth,
        );
    }
    if let ValueExpr::ScalarOrdering {
        operator,
        left,
        right,
    } = value
    {
        let bool_type = semantic_index.bool_type(module)?;
        if semantic_index.same_type(expected_type, &bool_type) {
            return canonical_scalar_ordering_value(scope, *operator, left, right, depth);
        }
        return Err(Error::new(format!(
            "scalar expression must be resolved before checking value of type {expected_type}"
        )));
    }
    if matches!(
        value,
        ValueExpr::BooleanNot { .. } | ValueExpr::BooleanBinary { .. }
    ) {
        return Err(Error::new(format!(
            "boolean predicate expression must be resolved before checking value of type {expected_type}"
        )));
    }
    if matches!(value, ValueExpr::Grouped { .. }) {
        return Err(Error::new(format!(
            "parenthesized value expression must be resolved before checking value of type {expected_type}"
        )));
    }
    if let Ok(record) = semantic_index.record_decl(module, expected_type) {
        return canonical_record_value(
            module,
            semantic_index,
            record,
            value,
            bindings,
            context,
            depth,
        );
    }
    if semantic_index.collection_type(expected_type)?.is_some() {
        return canonical_collection_value(
            module,
            semantic_index,
            expected_type,
            value,
            bindings,
            context,
            depth,
        );
    }
    if let Some(scalar) = semantic_index.scalar_type(expected_type)? {
        return canonical_scalar_value(scalar, expected_type, value);
    }

    canonical_enum_value(
        module,
        semantic_index,
        expected_type,
        value,
        bindings,
        context,
        depth,
    )
}

fn canonical_if_else_value(
    scope: CanonicalValueScope<'_, '_>,
    expected_type: &TypeRef,
    condition: &ValueExpr,
    then_branch: &ValueExpr,
    else_branch: &ValueExpr,
    depth: usize,
) -> Result<ArtifactValue> {
    let bool_type = scope.semantic_index.bool_type(scope.module)?;
    let condition = canonical_value(
        scope.module,
        scope.semantic_index,
        &bool_type,
        condition,
        scope.bindings,
        scope.context,
        depth + 1,
    )?;
    let branch = match condition {
        ArtifactValue::Atom(label) if label == "True" => then_branch,
        ArtifactValue::Atom(label) if label == "False" => else_branch,
        _ => {
            return Err(Error::new(
                "if condition must evaluate to unit Bool value False or True",
            ));
        }
    };
    canonical_value(
        scope.module,
        scope.semantic_index,
        expected_type,
        branch,
        scope.bindings,
        scope.context,
        depth + 1,
    )
}

fn canonical_scalar_value(
    scalar: mantle_artifact::ArtifactScalarType,
    expected_type: &TypeRef,
    value: &ValueExpr,
) -> Result<ArtifactValue> {
    let ValueExpr::ScalarLiteral(value) = value else {
        return Err(Error::new(format!(
            "expected scalar literal for type {expected_type}"
        )));
    };
    if value.ty() != scalar {
        return Err(Error::new(format!(
            "scalar literal {} has type {}, expected {}",
            value.label(),
            value.ty().source_name(),
            expected_type
        )));
    }
    Ok(ArtifactValue::Scalar(*value))
}

fn canonical_enum_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<ArtifactValue> {
    let enum_decl = semantic_index.enum_decl(module, expected_type)?;
    match value {
        ValueExpr::Identifier(name) => {
            if let Some(variant) = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name == *name)
            {
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "enum variant {} requires a payload and cannot be used as a fieldless value",
                        variant.name
                    )));
                }
                return Ok(ArtifactValue::Atom(name.to_string()));
            }
            Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )))
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
            let payload = canonical_value(
                module,
                semantic_index,
                payload_type,
                payload,
                bindings,
                context,
                depth + 1,
            )?;
            let value = ArtifactValue::EnumVariant {
                variant: name.to_string(),
                payload: Box::new(payload),
            };
            validate_state_value_metadata_label(&value)?;
            Ok(value)
        }
        ValueExpr::Call { .. }
        | ValueExpr::ScalarLiteral(_)
        | ValueExpr::Record(_)
        | ValueExpr::List(_)
        | ValueExpr::Map(_)
        | ValueExpr::IfElse { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::ScalarArithmetic { .. }
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => Err(Error::new(format!(
            "expected enum variant value for enum {}",
            enum_decl.name
        ))),
    }
}

fn canonical_collection_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<ArtifactValue> {
    let collection_type = semantic_index
        .collection_type(expected_type)?
        .ok_or_else(|| Error::new(format!("type {expected_type} is not a collection type")))?;
    match collection_type {
        CollectionType::List { element, capacity } => {
            let ValueExpr::List(list) = value else {
                return Err(Error::new(format!(
                    "list value type {expected_type} must be constructed with List<T,N>[...]"
                )));
            };
            validate_list_value_type(semantic_index, expected_type, list, element, capacity)?;
            let mut items = Vec::with_capacity(list.items.len());
            for item in &list.items {
                items.push(canonical_value(
                    module,
                    semantic_index,
                    element,
                    item,
                    bindings,
                    context,
                    depth + 1,
                )?);
            }
            let value = ArtifactValue::List(items);
            validate_state_value_metadata_label(&value)?;
            Ok(value)
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
            validate_map_value_type(semantic_index, expected_type, map, key, item, capacity)?;
            let mut entries = Vec::with_capacity(map.entries.len());
            let mut seen = BTreeSet::new();
            for entry in &map.entries {
                let key_value = canonical_value(
                    module,
                    semantic_index,
                    key,
                    &entry.key,
                    bindings,
                    context,
                    depth + 1,
                )?;
                let item_value = canonical_value(
                    module,
                    semantic_index,
                    item,
                    &entry.value,
                    bindings,
                    context,
                    depth + 1,
                )?;
                if !seen.insert(key_value.clone()) {
                    let key_label = key_value.label();
                    return Err(Error::new(format!(
                        "map value {expected_type} duplicates key {key_label}"
                    )));
                }
                entries.push(ArtifactMapEntry {
                    key: key_value,
                    value: item_value,
                });
            }
            let value = ArtifactValue::Map(entries);
            validate_state_value_metadata_label(&value)?;
            Ok(value)
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

fn canonical_record_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    record: &Record,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<ArtifactValue> {
    if let ValueExpr::Record(value) = value {
        if value.fields.is_empty() {
            return Err(Error::new(format!(
                "fieldless record values use `{}`; braced record values must declare at least one field",
                value.name
            )));
        }
    }

    if record.fields.is_empty() {
        return match value {
            ValueExpr::Identifier(name) if name == &record.name => {
                Ok(ArtifactValue::Atom(record.name.to_string()))
            }
            _ => Err(Error::new(format!(
                "provided value is not a value of record {}",
                record.name
            ))),
        };
    }

    let ValueExpr::Record(value) = value else {
        return Err(Error::new(format!(
            "record state type {} must be constructed with {} {{ ... }}",
            record.name, record.name
        )));
    };
    if value.name != record.name {
        return Err(Error::new(format!(
            "record constructor {} does not match expected record {}",
            value.name, record.name
        )));
    }

    let declared_fields = record
        .fields
        .iter()
        .map(|field| (field.name.as_str(), &field.ty))
        .collect::<BTreeMap<_, _>>();
    let mut provided = BTreeSet::new();
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &value.fields {
        if !provided.insert(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        let Some(field_ty) = declared_fields.get(field.name.as_str()) else {
            return Err(Error::new(format!(
                "record value {} declares unknown field {}",
                record.name, field.name
            )));
        };
        let field_value = canonical_value(
            module,
            semantic_index,
            field_ty,
            &field.value,
            bindings,
            context,
            depth + 1,
        )?;
        fields.push(ArtifactRecordField {
            name: field.name.to_string(),
            value: field_value,
        });
    }
    for field in &record.fields {
        if !provided.contains(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record.name, field.name
            )));
        }
    }
    let value = ArtifactValue::Record {
        constructor: record.name.to_string(),
        fields,
    };
    validate_state_value_metadata_label(&value)?;
    Ok(value)
}

fn validate_state_value_metadata_label(value: &ArtifactValue) -> Result<()> {
    validate_state_value_label(&value.label()).map_err(|err| Error::new(err.to_string()))
}

pub(super) fn validate_state_value_count(process_name: &Identifier, count: usize) -> Result<()> {
    if count == 0 {
        return Err(Error::new(format!(
            "process {} state_value_count must be greater than zero",
            process_name
        )));
    }
    if count > MAX_STATE_VALUES_PER_PROCESS {
        return Err(Error::new(format!(
            "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
            process_name
        )));
    }
    Ok(())
}

pub(super) fn reject_reserved_state_values(
    process_name: &Identifier,
    state_values: &[CheckedStateValue],
) -> Result<()> {
    if state_values.iter().any(|value| {
        matches!(value.value(), ArtifactValue::Atom(name) if name == STEP_STATE_PARAMETER_NAME)
    }) {
        return Err(Error::new(format!(
            "process {} state value {} conflicts with reserved step state parameter name",
            process_name, STEP_STATE_PARAMETER_NAME
        )));
    }
    Ok(())
}
