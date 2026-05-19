use super::*;

pub(in crate::language::checker) fn payload_binding_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload_value: &ArtifactValue,
    binding: &PatternPayloadParam,
) -> Result<Option<ArtifactValue>> {
    let mut value = payload_value.clone();
    for segment in binding.path.segments() {
        value = match &segment.kind {
            PayloadProjectionSegmentKind::EnumPayload { enum_ty, variant } => {
                let ArtifactValue::EnumVariant {
                    variant: actual,
                    payload,
                } = value
                else {
                    return Ok(None);
                };
                let enum_decl = semantic_index.enum_decl(module, enum_ty)?;
                let Some(expected) = enum_decl
                    .variants
                    .get(variant.index())
                    .map(|variant| variant.name.as_str())
                else {
                    return Ok(None);
                };
                if actual != expected {
                    return Ok(None);
                }
                *payload
            }
            PayloadProjectionSegmentKind::RecordField { field } => {
                let Ok(projected) = value.project_record_field(field.as_str()) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::ListIndex { index, len } => {
                let Ok(projected) = value.project_list_element(*index, *len) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::ListPrefixIndex { index, prefix_len } => {
                let Ok(projected) = value.project_list_prefix_element(*index, *prefix_len) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::ListRest { prefix_len } => {
                let Ok(projected) = value.project_list_rest(*prefix_len) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::MapValue {
                key,
                keys,
                projection,
            } => {
                let Ok(projected) = value.project_map_value(key, keys, *projection) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::MapRest { excluded_keys } => {
                let Ok(projected) = value.project_map_rest(excluded_keys) else {
                    return Ok(None);
                };
                projected
            }
        };
    }
    Ok(Some(value))
}

pub(in crate::language::checker) fn payload_matches_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload: &CheckedPayloadValue,
    guard: &PatternPayloadGuard,
) -> Result<bool> {
    let Some(value) = payload.value() else {
        return Ok(false);
    };
    artifact_value_matches_guard(module, semantic_index, value, guard)
}

pub(in crate::language::checker) fn source_payload_matches_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload: Option<&ValueExpr>,
    guard: Option<&PatternPayloadGuard>,
) -> Result<bool> {
    let Some(guard) = guard else {
        return Ok(true);
    };
    let Some(payload) = payload else {
        return Ok(false);
    };
    let value =
        canonical_source_value_with_bindings(module, semantic_index, &guard.enum_ty, payload, &[])?;
    artifact_value_matches_guard(module, semantic_index, &value, guard)
}

pub(in crate::language::checker) fn artifact_value_matches_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    value: &ArtifactValue,
    guard: &PatternPayloadGuard,
) -> Result<bool> {
    let enum_decl = semantic_index.enum_decl(module, &guard.enum_ty)?;
    let Some(variant) = enum_decl.variants.get(guard.variant.index()) else {
        return Ok(false);
    };
    match (&variant.payload_type, &guard.payload, value) {
        (None, None, ArtifactValue::Atom(actual)) => Ok(actual == variant.name.as_str()),
        (None, None, _) => Ok(false),
        (None, Some(_), _) => Err(Error::new(format!(
            "fieldless enum variant {} has a nested payload guard",
            variant.name
        ))),
        (
            Some(_),
            nested_guard,
            ArtifactValue::EnumVariant {
                variant: actual,
                payload,
            },
        ) if actual == variant.name.as_str() => match nested_guard {
            Some(nested_guard) => {
                artifact_value_matches_guard(module, semantic_index, payload, nested_guard)
            }
            None => Ok(true),
        },
        (Some(_), _, _) => Ok(false),
    }
}

pub(in crate::language::checker) fn checked_payload_binding(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload: &CheckedPayloadValue,
    binding: &PatternPayloadParam,
) -> Result<Option<(String, Option<ArtifactValue>)>> {
    let Some(payload_value) = payload.value() else {
        return Ok(binding
            .path
            .is_whole()
            .then(|| (payload.label().to_string(), None)));
    };
    Ok(
        payload_binding_value(module, semantic_index, payload_value, binding)?
            .map(|value| (value.label(), Some(value))),
    )
}
