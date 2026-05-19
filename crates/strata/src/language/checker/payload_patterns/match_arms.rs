use super::*;

pub(in crate::language::checker) fn check_typed_match_arms<'a>(
    context: &PatternCheckContext<'_>,
    arms: &'a [MatchArm],
) -> Result<Vec<TypedMatchArm<'a>>> {
    let mut explicit_arms = vec![false; context.enum_decl.variants.len()];
    let mut wildcard_seen = false;
    let mut checked_arms = Vec::with_capacity(arms.len());
    let label = context.label;

    for arm in arms {
        let pattern = check_typed_match_pattern(context, &arm.pattern)?;
        match pattern {
            TypedMatchPattern::Variant { variant, .. } => {
                if explicit_arms[variant] {
                    return Err(Error::new(format!(
                        "{} {label} declares duplicate pattern for variant {}",
                        context.subject, context.enum_decl.variants[variant].name,
                    )));
                }
                explicit_arms[variant] = true;
            }
            TypedMatchPattern::Wildcard => {
                if wildcard_seen {
                    return Err(Error::new(format!(
                        "{} {label} declares duplicate wildcard pattern",
                        context.subject
                    )));
                }
                wildcard_seen = true;
            }
        }
        checked_arms.push(TypedMatchArm {
            pattern,
            body: &arm.body,
        });
    }

    if wildcard_seen && explicit_arms.iter().all(|is_present| *is_present) {
        return Err(Error::new(format!(
            "{} {label} wildcard pattern is unreachable",
            context.subject
        )));
    }
    if !wildcard_seen {
        for (index, variant) in context.enum_decl.variants.iter().enumerate() {
            if !explicit_arms[index] {
                return Err(Error::new(format!(
                    "{} {label} must handle variant {}",
                    context.subject, variant.name,
                )));
            }
        }
    }

    Ok(checked_arms)
}

pub(in crate::language::checker) fn check_payload_sensitive_typed_match_arms<'a>(
    context: &PatternCheckContext<'_>,
    arms: &'a [MatchArm],
) -> Result<Vec<TypedMatchArm<'a>>> {
    let mut seen_patterns = Vec::new();
    let mut unguarded_variants = vec![false; context.enum_decl.variants.len()];
    let mut guarded_variants = vec![false; context.enum_decl.variants.len()];
    let mut wildcard_seen = false;
    let mut checked_arms = Vec::with_capacity(arms.len());
    let label = context.label;

    for arm in arms {
        let pattern = check_typed_match_pattern(context, &arm.pattern)?;
        match &pattern {
            TypedMatchPattern::Variant {
                variant,
                payload_guard,
                ..
            } => {
                for seen in seen_patterns
                    .iter()
                    .filter(|seen: &&PayloadSensitivePattern| seen.variant == *variant)
                {
                    if payload_patterns_overlap(
                        context.semantic_index,
                        payload_guard.as_ref(),
                        seen.payload_guard.as_ref(),
                    )? {
                        return Err(Error::new(format!(
                            "{} {label} pattern {} overlaps an earlier pattern for the same typed payload shape",
                            context.subject,
                            typed_match_pattern_label(context, *variant, payload_guard.as_ref())?
                        )));
                    }
                }
                if payload_guard.is_some() {
                    guarded_variants[*variant] = true;
                } else {
                    unguarded_variants[*variant] = true;
                }
                seen_patterns.push(PayloadSensitivePattern {
                    variant: *variant,
                    payload_guard: payload_guard.clone(),
                });
            }
            TypedMatchPattern::Wildcard => {
                if wildcard_seen {
                    return Err(Error::new(format!(
                        "{} {label} declares duplicate wildcard pattern",
                        context.subject
                    )));
                }
                wildcard_seen = true;
            }
        }
        checked_arms.push(TypedMatchArm {
            pattern,
            body: &arm.body,
        });
    }

    if wildcard_seen && unguarded_variants.iter().all(|is_present| *is_present) {
        return Err(Error::new(format!(
            "{} {label} wildcard pattern is unreachable",
            context.subject
        )));
    }
    if !wildcard_seen {
        for (index, variant) in context.enum_decl.variants.iter().enumerate() {
            if !unguarded_variants[index] && !guarded_variants[index] {
                return Err(Error::new(format!(
                    "{} {label} must handle variant {}",
                    context.subject, variant.name,
                )));
            }
        }
    }

    Ok(checked_arms)
}

pub(in crate::language::checker) fn check_typed_match_pattern(
    context: &PatternCheckContext<'_>,
    pattern: &Pattern,
) -> Result<TypedMatchPattern> {
    match pattern {
        Pattern::Constructor { name, payload } => {
            let variant_index = context.semantic_index.enum_variant_index(
                context.module,
                context.enum_type,
                name,
            )?;
            let variant = &context.enum_decl.variants[variant_index];
            let bindings = check_pattern_payload_bindings(
                context.module,
                context.semantic_index,
                variant,
                payload.as_ref(),
                context.label,
                context.payload_context,
                context.binding_context,
            )?;
            let payload_guard = check_pattern_payload_guard(
                context.module,
                context.semantic_index,
                variant,
                payload.as_ref(),
            )?;
            Ok(TypedMatchPattern::Variant {
                variant: variant_index,
                bindings,
                payload_guard,
            })
        }
        Pattern::Record { name, .. } => Err(Error::new(format!(
            "{} {} pattern {name} destructures a record, but this match expects enum constructors",
            context.subject, context.label
        ))),
        Pattern::List(_) => Err(Error::new(format!(
            "{} {} pattern List[...] destructures a list, but this match expects enum constructors",
            context.subject, context.label
        ))),
        Pattern::Map(_) => Err(Error::new(format!(
            "{} {} pattern Map[...] destructures a map, but this match expects enum constructors",
            context.subject, context.label
        ))),
        Pattern::Wildcard => Ok(TypedMatchPattern::Wildcard),
    }
}

pub(in crate::language::checker) fn payload_patterns_overlap(
    semantic_index: &SemanticIndex,
    left: Option<&PatternPayloadGuard>,
    right: Option<&PatternPayloadGuard>,
) -> Result<bool> {
    match (left, right) {
        (Some(left), Some(right)) => Ok(!payload_guards_are_disjoint(semantic_index, left, right)?),
        _ => Ok(true),
    }
}

fn payload_guards_are_disjoint(
    semantic_index: &SemanticIndex,
    left: &PatternPayloadGuard,
    right: &PatternPayloadGuard,
) -> Result<bool> {
    if !semantic_index.same_type(&left.enum_ty, &right.enum_ty) {
        return Ok(false);
    }
    if left.variant != right.variant {
        return Ok(true);
    }
    match (&left.payload, &right.payload) {
        (Some(left), Some(right)) => payload_guards_are_disjoint(semantic_index, left, right),
        _ => Ok(false),
    }
}

fn typed_match_pattern_label(
    context: &PatternCheckContext<'_>,
    variant: usize,
    payload_guard: Option<&PatternPayloadGuard>,
) -> Result<String> {
    let variant_name = context
        .enum_decl
        .variants
        .get(variant)
        .map(|variant| variant.name.to_string())
        .ok_or_else(|| {
            Error::new(format!(
                "{} {} pattern references missing variant id {variant}",
                context.subject, context.label
            ))
        })?;
    match payload_guard {
        Some(guard) => Ok(format!(
            "{variant_name}({})",
            payload_guard_label(context.module, context.semantic_index, guard)?
        )),
        None => Ok(variant_name),
    }
}

pub(in crate::language::checker) fn payload_guard_label(
    module: &Module,
    semantic_index: &SemanticIndex,
    guard: &PatternPayloadGuard,
) -> Result<String> {
    let enum_decl = semantic_index.enum_decl(module, &guard.enum_ty)?;
    let variant = enum_decl
        .variants
        .get(guard.variant.index())
        .ok_or_else(|| {
            Error::new(format!(
                "nested payload guard references missing variant id {} for enum {}",
                guard.variant.as_u32(),
                enum_decl.name
            ))
        })?;
    match &guard.payload {
        Some(payload) => Ok(format!(
            "{}({})",
            variant.name,
            payload_guard_label(module, semantic_index, payload)?
        )),
        None => Ok(variant.name.to_string()),
    }
}
