use super::*;

pub(super) fn reject_unreachable_payload_guarded_clauses(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    message_variants: &[EnumVariant],
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    concrete_message_cases: &[StepConcreteMessageCase],
    pattern_label: &str,
) -> Result<()> {
    for (variant_index, clauses) in explicit_clauses.iter().enumerate() {
        if !concrete_message_cases
            .iter()
            .any(|case| case.variant.index() == variant_index && case.payload.is_some())
        {
            continue;
        }
        for clause in clauses {
            if clause.payload_guard.is_none() {
                continue;
            }
            let mut has_reachable_case = false;
            for concrete_case in concrete_message_cases
                .iter()
                .filter(|case| case.variant.index() == variant_index)
            {
                if step_body_clause_matches_case(
                    module,
                    semantic_index,
                    clause,
                    concrete_case.payload.as_ref(),
                )? {
                    has_reachable_case = true;
                    break;
                }
            }
            if !has_reachable_case {
                return Err(Error::new(format!(
                    "process {} {} {} has no discovered payload case",
                    process.name,
                    pattern_label,
                    step_pattern_payload_label(
                        module,
                        semantic_index,
                        &message_variants[variant_index],
                        clause.payload_guard.as_ref(),
                    )?
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn reject_unreachable_wildcard(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    wildcard_clause: Option<&StepBodyClause<'_>>,
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    concrete_message_cases: &[StepConcreteMessageCase],
) -> Result<()> {
    if wildcard_clause.is_none() {
        return Ok(());
    }
    for concrete_case in concrete_message_cases {
        if !explicit_step_body_clauses_match_case(
            module,
            semantic_index,
            &explicit_clauses[concrete_case.variant.index()],
            concrete_case.payload.as_ref(),
        )? {
            return Ok(());
        }
    }
    Err(Error::new(format!(
        "process {} wildcard step pattern is unreachable",
        process.name
    )))
}

fn explicit_step_body_clauses_match_case(
    module: &Module,
    semantic_index: &SemanticIndex,
    clauses: &[StepBodyClause<'_>],
    payload: Option<&CheckedPayloadValue>,
) -> Result<bool> {
    for clause in clauses {
        if step_body_clause_matches_case(module, semantic_index, clause, payload)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) struct MatchingStepBodyClauses<'clauses, 'source> {
    pub(super) first: Option<&'clauses StepBodyClause<'source>>,
    pub(super) count: usize,
}

pub(super) fn matching_step_body_clauses<'clauses, 'source>(
    module: &Module,
    semantic_index: &SemanticIndex,
    clauses: &'clauses [StepBodyClause<'source>],
    payload: Option<&CheckedPayloadValue>,
) -> Result<MatchingStepBodyClauses<'clauses, 'source>> {
    let mut first = None;
    let mut count = 0usize;
    for clause in clauses {
        if step_body_clause_matches_case(module, semantic_index, clause, payload)? {
            count = count.checked_add(1).ok_or_else(|| {
                Error::new("internal error: matching step body clause count overflowed")
            })?;
            first.get_or_insert(clause);
        }
    }
    Ok(MatchingStepBodyClauses { first, count })
}

fn step_body_clause_matches_case(
    module: &Module,
    semantic_index: &SemanticIndex,
    clause: &StepBodyClause<'_>,
    payload: Option<&CheckedPayloadValue>,
) -> Result<bool> {
    let Some(payload_guard) = &clause.payload_guard else {
        return Ok(true);
    };
    let Some(payload) = payload else {
        return Ok(false);
    };
    payload_matches_guard(module, semantic_index, payload, payload_guard)
}

pub(super) fn transition_payload_guard_for_case(
    clause: &StepBodyClause<'_>,
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    concrete_case: &StepConcreteMessageCase,
) -> Option<CheckedPayloadValue> {
    (clause.payload_guard.is_some()
        || has_payload_sensitive_clause(explicit_clauses, concrete_case.variant))
    .then(|| concrete_case.payload.clone())
    .flatten()
}

pub(super) fn wildcard_payload_guard_for_case(
    process: &Process,
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    wildcard_clause: &StepBodyClause<'_>,
    concrete_case: &StepConcreteMessageCase,
    message_variant: &EnumVariant,
    pattern_label: &str,
) -> Result<Option<CheckedPayloadValue>> {
    if !has_payload_sensitive_clause(explicit_clauses, concrete_case.variant) {
        return Ok(None);
    }
    let wildcard_is_state_match = is_state_match_clause(wildcard_clause);
    concrete_case.payload.clone().map(Some).ok_or_else(|| {
        Error::new(format!(
            "process {} payload-sensitive {} for message {} has no discovered payload case for wildcard fallback",
            process.name,
            if wildcard_is_state_match {
                "state match step pattern"
            } else {
                pattern_label
            },
            message_variant.name
        ))
    })
}

pub(super) fn step_dispatch_pattern_label(
    dispatch_style: Option<StepDispatchStyle>,
) -> &'static str {
    match dispatch_style {
        Some(StepDispatchStyle::BodyMatch) => "match msg pattern",
        Some(StepDispatchStyle::ParameterPattern) | None => "step pattern",
    }
}

fn has_payload_sensitive_clause(
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    variant: CheckedMessageVariantId,
) -> bool {
    explicit_clauses[variant.index()]
        .iter()
        .any(|clause| clause.payload_guard.is_some())
}

fn first_payload_sensitive_message_matching<'a>(
    message_variants: &'a [EnumVariant],
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    predicate: impl Fn(&StepBodyClause<'_>) -> bool,
) -> Result<Option<&'a EnumVariant>> {
    if message_variants.len() != explicit_clauses.len() {
        return Err(Error::new(format!(
            "internal error: step wildcard validation expected {} message variants, found {} clause buckets",
            message_variants.len(),
            explicit_clauses.len()
        )));
    }

    Ok(message_variants
        .iter()
        .zip(explicit_clauses.iter())
        .find_map(|(message_variant, clauses)| {
            clauses
                .iter()
                .any(|clause| clause.payload_guard.is_some() && predicate(clause))
                .then_some(message_variant)
        }))
}

pub(super) fn validate_process_wildcard_compatibility(
    process: &Process,
    message_variants: &[EnumVariant],
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
    wildcard_clause: Option<&StepBodyClause<'_>>,
) -> Result<()> {
    let Some(wildcard_clause) = wildcard_clause else {
        return Ok(());
    };

    if is_state_match_clause(wildcard_clause) {
        if let Some(message_variant) = first_payload_sensitive_message_matching(
            message_variants,
            explicit_clauses,
            |clause| !is_state_match_clause(clause),
        )? {
            return Err(Error::new(format!(
                "process {} declares payload-sensitive step pattern for message {} with a state match wildcard step pattern",
                process.name, message_variant.name
            )));
        }
    } else if let Some(message_variant) = first_payload_sensitive_message_matching(
        message_variants,
        explicit_clauses,
        is_state_match_clause,
    )? {
        return Err(Error::new(format!(
            "process {} declares a wildcard step pattern with a payload-sensitive state match step pattern for message {}",
            process.name, message_variant.name
        )));
    }

    Ok(())
}

fn is_state_match_clause(clause: &StepBodyClause<'_>) -> bool {
    matches!(&clause.body, StepBodySource::StateMatch(_))
}
