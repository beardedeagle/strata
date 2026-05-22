use super::*;

fn state_match_transition_key(
    message: CheckedMessageId,
    current_state: CheckedStateId,
    payload_guard: Option<&CheckedPayloadValue>,
) -> Result<StateMatchTransitionKey> {
    Ok(StateMatchTransitionKey {
        message,
        current_state,
        payload_guard: payload_guard
            .map(CheckedPayloadGuardKey::from_payload)
            .transpose()?,
    })
}

pub(super) struct StateMatchExpansionContext<'a, 'state> {
    pub(super) module: &'a Module,
    pub(super) process: &'a Process,
    pub(super) process_id: CheckedProcessId,
    pub(super) semantic_index: &'a SemanticIndex,
    pub(super) message_cases: &'a MessageCaseTable,
    pub(super) state_space: &'a mut StateSpace<'state>,
    pub(super) types: &'a mut CheckedTypeInterner<'state>,
}

pub(super) fn expand_state_match_step_clause_group<'a, 'state>(
    mut context: StateMatchExpansionContext<'_, 'state>,
    state_match_cases: &[StateMatchStepExpansion<'a>],
    clauses: &mut Vec<StepClause<'a>>,
) -> Result<()> {
    if state_match_cases.is_empty() {
        return Ok(());
    }

    let mut seen_transitions = BTreeSet::new();
    let mut iteration_count = 0usize;
    loop {
        let state_count = context.state_space.values().len();
        let transition_count = seen_transitions.len();
        for case in state_match_cases {
            expand_state_match_step_clause_once(
                &mut context,
                case,
                &mut seen_transitions,
                clauses,
            )?;
        }
        if context.state_space.values().len() == state_count
            && seen_transitions.len() == transition_count
        {
            break;
        }
        iteration_count = iteration_count
            .checked_add(1)
            .ok_or_else(|| Error::new("state match expansion iteration count overflowed"))?;
        if iteration_count > MAX_STATE_VALUES_PER_PROCESS.saturating_add(state_match_cases.len()) {
            return Err(Error::new(format!(
                "process {} state match expansion did not converge within the state value limit",
                context.process.name
            )));
        }
    }
    Ok(())
}

fn expand_state_match_step_clause_once<'a, 'state>(
    context: &mut StateMatchExpansionContext<'_, 'state>,
    case: &StateMatchStepExpansion<'a>,
    seen_transitions: &mut BTreeSet<StateMatchTransitionKey>,
    clauses: &mut Vec<StepClause<'a>>,
) -> Result<()> {
    let state_enum = context
        .semantic_index
        .enum_decl(context.module, &context.process.state_type)?;
    let subject = format!("process {}", context.process.name);
    let pattern_context = PatternCheckContext {
        module: context.module,
        semantic_index: context.semantic_index,
        enum_decl: state_enum,
        enum_type: &context.process.state_type,
        subject: &subject,
        label: "state match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_typed_match_arms(&pattern_context, &case.match_body.arms)?;
    let explicit_variants = arms
        .iter()
        .filter_map(|arm| match arm.pattern {
            TypedMatchPattern::Variant { variant, .. } => Some(variant),
            TypedMatchPattern::Wildcard => None,
        })
        .collect::<BTreeSet<_>>();
    for arm in &arms {
        let cases = state_match_arm_cases(context, state_enum, &explicit_variants, &arm.pattern)?;
        for (current_state, state_payload_bindings) in cases {
            validate_state_payload_binding_name(
                context.process,
                &case.payload_bindings,
                &state_payload_bindings,
            )?;
            preadmit_state_match_case_return(
                context,
                case.variant,
                arm.body,
                case.payload_guard.as_ref(),
                &case.payload_bindings,
                &state_payload_bindings,
            )?;
            let key = state_match_transition_key(
                case.message,
                current_state,
                case.payload_guard.as_ref(),
            )?;
            if seen_transitions.insert(key) {
                clauses.push(StepClause {
                    step: case.step,
                    variant: case.variant,
                    message: case.message,
                    payload_guard: case.payload_guard.clone(),
                    payload_bindings: case.payload_bindings.clone(),
                    current_state: Some(current_state),
                    state_payload_bindings,
                    body: arm.body,
                });
            }
        }
    }
    Ok(())
}

fn preadmit_state_match_case_return<'state>(
    context: &mut StateMatchExpansionContext<'_, 'state>,
    variant: CheckedMessageVariantId,
    body: &FunctionBlock,
    payload_guard: Option<&CheckedPayloadValue>,
    payload_bindings: &[StepPayloadBinding],
    state_payload_bindings: &[StepStatePayloadBinding],
) -> Result<()> {
    let mut preadmit_context = StepReturnPreadmitContext {
        module: context.module,
        process: context.process,
        process_id: context.process_id,
        semantic_index: context.semantic_index,
        message_cases: context.message_cases,
        state_space: context.state_space,
        types: context.types,
    };
    preadmit_step_return_state_value(
        &mut preadmit_context,
        &StepReturnInput {
            variant,
            payload_guard,
            payload_bindings,
            state_payload_bindings,
            body,
        },
    )
}

fn validate_state_payload_binding_name(
    process: &Process,
    message_payload_bindings: &[StepPayloadBinding],
    state_payload_bindings: &[StepStatePayloadBinding],
) -> Result<()> {
    for state_payload_binding in state_payload_bindings {
        if message_payload_bindings
            .iter()
            .any(|message_binding| message_binding.name == state_payload_binding.name)
        {
            return Err(Error::new(format!(
                "process {} state payload binding {} conflicts with message payload binding",
                process.name, state_payload_binding.name
            )));
        }
    }
    Ok(())
}

fn state_match_arm_cases(
    context: &mut StateMatchExpansionContext<'_, '_>,
    state_enum: &Enum,
    explicit_variants: &BTreeSet<usize>,
    pattern: &TypedMatchPattern,
) -> Result<Vec<(CheckedStateId, Vec<StepStatePayloadBinding>)>> {
    match pattern {
        TypedMatchPattern::Variant {
            variant,
            bindings,
            payload_guard,
        } => {
            let variant_decl = &state_enum.variants[*variant];
            match &variant_decl.payload_type {
                None if bindings.is_empty() => {
                    let value = ValueExpr::Identifier(variant_decl.name.clone());
                    let state = context.state_space.resolve_state_value(
                        context.semantic_index,
                        context.types,
                        &value,
                    )?;
                    Ok(vec![(state, Vec::new())])
                }
                None => Err(Error::new(format!(
                    "process {} state match pattern {} does not carry a payload",
                    context.process.name, variant_decl.name
                ))),
                Some(payload_type) => {
                    if bindings.is_empty() && payload_guard.is_none() {
                        return Err(Error::new(format!(
                            "process {} state match pattern {} requires a payload binding",
                            context.process.name, variant_decl.name
                        )));
                    }
                    let checked_ty = context.types.intern(payload_type)?;
                    let payloads = state_match_payload_domain(context, payload_type, &checked_ty)?;
                    payloads
                        .into_iter()
                        .map(|payload| {
                            if let Some(guard) = payload_guard
                                && !payload_matches_guard(
                                    context.module,
                                    context.semantic_index,
                                    &payload,
                                    guard,
                                )?
                            {
                                return Err(Error::new(format!(
                                    "process {} state match pattern {} does not match discovered payload {}",
                                    context.process.name,
                                    variant_decl.name,
                                    payload.label()
                                )));
                            }
                            let payload_name = Identifier::new("__state_payload")?;
                            let state_value = ValueExpr::EnumVariant {
                                name: variant_decl.name.clone(),
                                payload: Box::new(ValueExpr::Identifier(payload_name.clone())),
                            };
                            let payload_value = payload.value().cloned().ok_or_else(|| {
                                Error::new(format!(
                                    "process {} state payload {} cannot be a process reference",
                                    context.process.name,
                                    payload.label()
                                ))
                            })?;
                            let state = context.state_space.resolve_state_value_with_bindings(
                                context.semantic_index,
                                context.types,
                                &state_value,
                                &[ValueBinding {
                                    name: &payload_name,
                                    ty: payload_type,
                                    label: payload_value.label(),
                                    value: Some(payload_value),
                                }],
                            )?;
                            let state_payload_bindings = bindings
                                .iter()
                                .map(|binding| {
                                    let (label, value) = checked_payload_binding(
                                        context.module,
                                        context.semantic_index,
                                        &payload,
                                        binding,
                                    )?
                                    .ok_or_else(|| {
                                        Error::new(format!(
                                            "process {} state payload {} does not match binding {}",
                                            context.process.name,
                                            payload.label(),
                                            binding.name
                                        ))
                                    })?;
                                    let value = value.ok_or_else(|| {
                                        Error::new(format!(
                                            "process {} state payload {} does not match binding {}",
                                            context.process.name,
                                            payload.label(),
                                            binding.name
                                        ))
                                    })?;
                                    Ok(StepStatePayloadBinding {
                                        name: binding.name.clone(),
                                        payload_ty: payload_type.clone(),
                                        ty: binding.ty.clone(),
                                        checked_payload_ty: checked_ty.clone(),
                                        checked_ty: context.types.intern(&binding.ty)?,
                                        label,
                                        value,
                                        path: binding.path.clone(),
                                    })
                                })
                                .collect::<Result<Vec<_>>>()?;
                            Ok((state, state_payload_bindings))
                        })
                        .collect()
                }
            }
        }
        TypedMatchPattern::Wildcard => {
            let mut cases = Vec::new();
            for (variant_index, variant_decl) in state_enum.variants.iter().enumerate() {
                if explicit_variants.contains(&variant_index) {
                    continue;
                }
                match &variant_decl.payload_type {
                    None => {
                        let value = ValueExpr::Identifier(variant_decl.name.clone());
                        let state = context.state_space.resolve_state_value(
                            context.semantic_index,
                            context.types,
                            &value,
                        )?;
                        cases.push((state, Vec::new()));
                    }
                    Some(payload_type) => {
                        let checked_ty = context.types.intern(payload_type)?;
                        let payload_name = Identifier::new("__state_payload")?;
                        let state_value = ValueExpr::EnumVariant {
                            name: variant_decl.name.clone(),
                            payload: Box::new(ValueExpr::Identifier(payload_name.clone())),
                        };
                        for payload in
                            state_match_payload_domain(context, payload_type, &checked_ty)?
                        {
                            let payload_value = payload.value().cloned().ok_or_else(|| {
                                Error::new(format!(
                                    "process {} state payload {} cannot be a process reference",
                                    context.process.name,
                                    payload.label()
                                ))
                            })?;
                            let state = context.state_space.resolve_state_value_with_bindings(
                                context.semantic_index,
                                context.types,
                                &state_value,
                                &[ValueBinding {
                                    name: &payload_name,
                                    ty: payload_type,
                                    label: payload_value.label(),
                                    value: Some(payload_value),
                                }],
                            )?;
                            cases.push((state, Vec::new()));
                        }
                    }
                }
            }
            Ok(cases)
        }
    }
}

fn state_match_payload_domain(
    context: &StateMatchExpansionContext<'_, '_>,
    payload_type: &TypeRef,
    checked_payload_type: &CheckedTypeRef,
) -> Result<Vec<CheckedPayloadValue>> {
    let mut payloads = BTreeMap::new();
    for state in context.state_space.values() {
        if let Some(payload) = state.payload() {
            if payload.ty() == checked_payload_type {
                payloads.insert(PayloadDomainKey::from_payload(payload)?, payload.clone());
            }
        }
    }
    let msg_enum = context
        .semantic_index
        .enum_decl(context.module, &context.process.msg_type)?;
    for (variant_index, message_variant) in msg_enum.variants.iter().enumerate() {
        let Some(message_payload_type) = &message_variant.payload_type else {
            continue;
        };
        if !context
            .semantic_index
            .same_type(message_payload_type, payload_type)
        {
            continue;
        }
        let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
        for payload in context
            .message_cases
            .payload_values(context.process_id, variant_id)?
        {
            payloads.insert(PayloadDomainKey::from_payload(payload)?, payload.clone());
        }
    }
    Ok(payloads.into_values().collect())
}
