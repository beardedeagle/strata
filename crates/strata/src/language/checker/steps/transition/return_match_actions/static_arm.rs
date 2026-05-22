use crate::language::MAX_VALUE_NESTING;
use crate::language::ast::{ListValue, MapValue, MapValueEntry, RecordValue, RecordValueField};

use super::*;

pub(super) fn static_step_return_match_arm_substitutions<'a>(
    context: &StepCheckContext<'_>,
    pattern: &'a TypedMatchPattern,
) -> Result<Vec<StaticArmSubstitution<'a>>> {
    let TypedMatchPattern::Variant { bindings, .. } = pattern else {
        return Ok(Vec::new());
    };
    if bindings.is_empty() {
        return Ok(Vec::new());
    }
    bindings
        .iter()
        .map(|binding| {
            Ok(StaticArmSubstitution {
                name: &binding.name,
                value: static_source_value_for_type(
                    context.module,
                    context.semantic_index,
                    &binding.ty,
                    0,
                )?,
            })
        })
        .collect()
}

fn static_source_value_for_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    ty: &TypeRef,
    depth: usize,
) -> Result<ValueExpr> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if semantic_index.process_ref_target_type(ty)?.is_some() {
        return Err(Error::new(
            "process references must be direct message payloads",
        ));
    }
    if let Ok(record) = semantic_index.record_decl(module, ty) {
        if record.fields.is_empty() {
            return Ok(ValueExpr::Identifier(record.name.clone()));
        }
        let mut fields = Vec::with_capacity(record.fields.len());
        for field in &record.fields {
            fields.push(RecordValueField {
                name: field.name.clone(),
                value: static_source_value_for_type(module, semantic_index, &field.ty, depth + 1)?,
            });
        }
        return Ok(ValueExpr::Record(RecordValue {
            name: record.name.clone(),
            fields,
        }));
    }
    if let Some(collection) = semantic_index.collection_type(ty)? {
        return Ok(match collection {
            CollectionType::List { element, capacity } => ValueExpr::List(ListValue {
                element_type: Some(element.clone()),
                capacity: Some(capacity),
                items: Vec::new(),
            }),
            CollectionType::Map {
                key,
                value,
                capacity,
            } => ValueExpr::Map(MapValue {
                key_type: Some(key.clone()),
                value_type: Some(value.clone()),
                capacity: Some(capacity),
                entries: Vec::<MapValueEntry>::new(),
            }),
        });
    }
    let enum_decl = semantic_index.enum_decl(module, ty)?;
    let variant = enum_decl
        .variants
        .iter()
        .find(|variant| variant.payload_type.is_none())
        .or_else(|| enum_decl.variants.first())
        .ok_or_else(|| {
            Error::new(format!(
                "enum {} must declare at least one variant",
                enum_decl.name
            ))
        })?;
    match &variant.payload_type {
        Some(payload_type) => Ok(ValueExpr::EnumVariant {
            name: variant.name.clone(),
            payload: Box::new(static_source_value_for_type(
                module,
                semantic_index,
                payload_type,
                depth + 1,
            )?),
        }),
        None => Ok(ValueExpr::Identifier(variant.name.clone())),
    }
}
