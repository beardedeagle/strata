use super::discovery::{check_step_pattern, check_step_shape};
use super::*;

pub(super) fn check_step_clauses<'a>(
    module: &Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<Vec<StepClause<'a>>> {
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    let mut explicit_clauses = vec![None; msg_enum.variants.len()];
    let mut wildcard_clause = None;
    let mut dispatch_style = None;
    let mut match_body_seen = false;

    preadmit_concrete_step_state_values(
        module,
        process,
        process_id,
        semantic_index,
        state_space,
        types,
    )?;

    for step in &process.steps {
        let Some(body) = &step.body else {
            return Err(Error::new("step must have a body for buildable source"));
        };
        match check_step_shape(module, process, process_id, semantic_index, step)? {
            StepDispatchForm::ParameterPattern(pattern) => {
                set_step_dispatch_style(
                    process,
                    &mut dispatch_style,
                    StepDispatchStyle::ParameterPattern,
                )?;
                let FunctionBody::Block(body) = body else {
                    return Err(Error::new("step parameter pattern must use a block body"));
                };
                insert_step_body_clause(
                    process,
                    &msg_enum.variants,
                    &mut explicit_clauses,
                    &mut wildcard_clause,
                    pattern,
                    StepBodyClause {
                        step,
                        body: StepBodySource::Block(body),
                        payload_params: Vec::new(),
                    },
                )?;
            }
            StepDispatchForm::BodyMatch => {
                set_step_dispatch_style(
                    process,
                    &mut dispatch_style,
                    StepDispatchStyle::BodyMatch,
                )?;
                if match_body_seen {
                    return Err(Error::new(format!(
                        "process {} declares duplicate match step body",
                        process.name
                    )));
                }
                match_body_seen = true;
                let FunctionBody::Match(match_body) = body else {
                    return Err(Error::new("match step must use a match body"));
                };
                for arm in &match_body.arms {
                    let pattern = check_step_pattern(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        &arm.pattern,
                    )?;
                    insert_step_body_clause(
                        process,
                        &msg_enum.variants,
                        &mut explicit_clauses,
                        &mut wildcard_clause,
                        pattern,
                        StepBodyClause {
                            step,
                            body: StepBodySource::Block(&arm.body),
                            payload_params: Vec::new(),
                        },
                    )?;
                }
            }
            StepDispatchForm::StateMatch(pattern) => {
                set_step_dispatch_style(
                    process,
                    &mut dispatch_style,
                    StepDispatchStyle::ParameterPattern,
                )?;
                let FunctionBody::Match(match_body) = body else {
                    return Err(Error::new("state match step must use a match body"));
                };
                insert_step_body_clause(
                    process,
                    &msg_enum.variants,
                    &mut explicit_clauses,
                    &mut wildcard_clause,
                    pattern,
                    StepBodyClause {
                        step,
                        body: StepBodySource::StateMatch(match_body),
                        payload_params: Vec::new(),
                    },
                )?;
            }
        }
    }

    if wildcard_clause.is_some() && explicit_clauses.iter().all(Option::is_some) {
        return Err(Error::new(format!(
            "process {} wildcard step pattern is unreachable",
            process.name
        )));
    }

    let message_cases_for_process = message_cases.cases_for(process_id)?;
    let mut clauses = Vec::with_capacity(message_cases_for_process.len());
    for (index, message_variant) in msg_enum.variants.iter().enumerate() {
        let Some(clause) = explicit_clauses[index]
            .as_ref()
            .or(wildcard_clause.as_ref())
        else {
            return Err(Error::new(format!(
                "process {} must declare step pattern for message {}",
                process.name, message_variant.name
            )));
        };
        let variant_id = CheckedMessageVariantId::from_index(index)?;
        let _case = message_cases_for_process
            .iter()
            .find(|case| case.variant() == variant_id)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} has no checked message case for message {}",
                    process.name, message_variant.name
                ))
            })?;
        let payload_bindings = clause
            .payload_params
            .iter()
            .map(|param| {
                let payload_ty = message_variant.payload_type.as_ref().ok_or_else(|| {
                    Error::new(format!(
                        "process {} step pattern for message {} binds payload {}, but the message has no payload",
                        process.name, message_variant.name, param.name
                    ))
                })?;
                Ok(StepPayloadBinding {
                    name: param.name.clone(),
                    payload_ty: payload_ty.clone(),
                    ty: param.ty.clone(),
                    checked_payload_ty: types.intern(payload_ty)?,
                    checked_ty: types.intern(&param.ty)?,
                    path: param.path.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let message = message_cases.message_id(process_id, variant_id)?;
        match &clause.body {
            StepBodySource::Block(body) => {
                clauses.push(StepClause {
                    step: clause.step,
                    variant: variant_id,
                    message,
                    payload_bindings,
                    current_state: None,
                    state_payload_bindings: Vec::new(),
                    body,
                });
            }
            StepBodySource::StateMatch(match_body) => expand_state_match_step_clauses(
                module,
                process,
                process_id,
                semantic_index,
                message_cases,
                state_space,
                types,
                clause.step,
                variant_id,
                message,
                payload_bindings,
                match_body,
                &mut clauses,
            )?,
        }
    }

    Ok(clauses)
}

fn preadmit_concrete_step_state_values(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<()> {
    for step in &process.steps {
        let Some(body) = &step.body else {
            continue;
        };
        match check_step_shape(module, process, process_id, semantic_index, step)? {
            StepDispatchForm::ParameterPattern(pattern) => {
                let FunctionBody::Block(body) = body else {
                    continue;
                };
                preadmit_concrete_step_return(
                    module,
                    process,
                    semantic_index,
                    state_space,
                    types,
                    body,
                    &step_pattern_binding_names(&pattern),
                )?;
            }
            StepDispatchForm::BodyMatch => {
                let FunctionBody::Match(match_body) = body else {
                    continue;
                };
                for arm in &match_body.arms {
                    let pattern = check_step_pattern(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        &arm.pattern,
                    )?;
                    preadmit_concrete_step_return(
                        module,
                        process,
                        semantic_index,
                        state_space,
                        types,
                        &arm.body,
                        &step_pattern_binding_names(&pattern),
                    )?;
                }
            }
            StepDispatchForm::StateMatch(pattern) => {
                let FunctionBody::Match(match_body) = body else {
                    continue;
                };
                let state_enum = semantic_index.enum_decl(module, &process.state_type)?;
                let subject = format!("process {}", process.name);
                let pattern_context = PatternCheckContext {
                    module,
                    semantic_index,
                    enum_decl: state_enum,
                    enum_type: &process.state_type,
                    subject: &subject,
                    label: "state match",
                    payload_context: PatternPayloadContext::SourceValue,
                    binding_context: PatternBindingContext::Source { owner: &subject },
                };
                let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;
                let message_bindings = step_pattern_binding_names(&pattern);
                for arm in arms {
                    let mut bindings = message_bindings.clone();
                    if let TypedMatchPattern::Variant {
                        variant,
                        bindings: arm_bindings,
                    } = &arm.pattern
                    {
                        let variant_decl = &state_enum.variants[*variant];
                        if variant_decl.payload_type.is_some() && arm_bindings.is_empty() {
                            return Err(Error::new(format!(
                                "process {} state match pattern {} requires a payload binding",
                                process.name, variant_decl.name
                            )));
                        }
                        bindings.extend(arm_bindings.iter().map(|binding| &binding.name));
                    }
                    preadmit_concrete_step_return(
                        module,
                        process,
                        semantic_index,
                        state_space,
                        types,
                        arm.body,
                        &bindings,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(in crate::language::checker) fn collect_concrete_state_payload_domains(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<Vec<ConcreteStatePayloadDomain>> {
    let mut local_types = CheckedTypeInterner::new(module, semantic_index);
    let mut state_space = StateSpace::new(module, semantic_index, process, &mut local_types)?;
    check_init(
        module,
        semantic_index,
        process,
        &mut state_space,
        &mut local_types,
    )?;
    preadmit_concrete_step_state_values(
        module,
        process,
        process_id,
        semantic_index,
        &mut state_space,
        &mut local_types,
    )?;

    let mut domains = Vec::new();
    for state in state_space.values() {
        if let Some(payload) = state.payload() {
            let value = payload.value().cloned().ok_or_else(|| {
                Error::new(format!(
                    "process {} state payload {} cannot be a process reference",
                    process.name,
                    payload.label()
                ))
            })?;
            insert_concrete_state_payload_domain(
                semantic_index,
                &mut domains,
                local_types.source_type(payload.ty())?.clone(),
                value,
            );
        }
    }
    Ok(domains)
}

fn insert_concrete_state_payload_domain(
    semantic_index: &SemanticIndex,
    domains: &mut Vec<ConcreteStatePayloadDomain>,
    ty: TypeRef,
    value: ArtifactValue,
) {
    if let Some(domain) = domains
        .iter_mut()
        .find(|domain| semantic_index.same_type(&domain.ty, &ty))
    {
        domain.values.insert(value);
        return;
    }
    domains.push(ConcreteStatePayloadDomain {
        ty,
        values: BTreeSet::from([value]),
    });
}

fn step_pattern_binding_names(pattern: &StepPattern) -> Vec<&Identifier> {
    match pattern {
        StepPattern::Variant { bindings, .. } => {
            bindings.iter().map(|binding| &binding.name).collect()
        }
        StepPattern::Wildcard => Vec::new(),
    }
}

fn preadmit_concrete_step_return(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    body: &FunctionBlock,
    binding_names: &[&Identifier],
) -> Result<()> {
    let state_arg = match &body.returns {
        ReturnExpr::Call { name, arg }
            if name.as_str() == "Stop"
                || name.as_str() == "Continue"
                || name.as_str() == "Panic" =>
        {
            arg
        }
        _ => return Ok(()),
    };
    if matches!(state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
        || binding_names
            .iter()
            .any(|binding| source_value_uses_binding(state_arg, binding))
    {
        return Ok(());
    }

    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        semantic_index,
    };
    let state_arg =
        resolve_source_value_expr(&function_scope, &process.state_type, state_arg, &[], 0)?;
    state_space.resolve_state_value(semantic_index, types, &state_arg)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn expand_state_match_step_clauses<'a>(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    step: &'a Function,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_bindings: Vec<StepPayloadBinding>,
    match_body: &'a crate::language::ast::Match,
    clauses: &mut Vec<StepClause<'a>>,
) -> Result<()> {
    let state_enum = semantic_index.enum_decl(module, &process.state_type)?;
    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module,
        semantic_index,
        enum_decl: state_enum,
        enum_type: &process.state_type,
        subject: &subject,
        label: "state match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;
    let explicit_variants = arms
        .iter()
        .filter_map(|arm| match arm.pattern {
            TypedMatchPattern::Variant { variant, .. } => Some(variant),
            TypedMatchPattern::Wildcard => None,
        })
        .collect::<BTreeSet<_>>();
    let mut seen_current_states = BTreeSet::new();
    let mut expanded_clauses = Vec::new();
    let mut iteration_count = 0usize;

    loop {
        let state_count = state_space.values().len();
        let clause_count = expanded_clauses.len();
        for arm in &arms {
            let cases = state_match_arm_cases(
                module,
                process,
                process_id,
                semantic_index,
                message_cases,
                state_space,
                types,
                state_enum,
                &explicit_variants,
                &arm.pattern,
            )?;
            for (current_state, state_payload_bindings) in cases {
                validate_state_payload_binding_name(
                    process,
                    &payload_bindings,
                    &state_payload_bindings,
                )?;
                preadmit_state_match_case_return(
                    module,
                    process,
                    process_id,
                    semantic_index,
                    message_cases,
                    state_space,
                    types,
                    variant,
                    arm.body,
                    &payload_bindings,
                    &state_payload_bindings,
                )?;
                if seen_current_states.insert(current_state.as_u32()) {
                    expanded_clauses.push(StepClause {
                        step,
                        variant,
                        message,
                        payload_bindings: payload_bindings.clone(),
                        current_state: Some(current_state),
                        state_payload_bindings,
                        body: arm.body,
                    });
                }
            }
        }
        if state_space.values().len() == state_count && expanded_clauses.len() == clause_count {
            break;
        }
        iteration_count = iteration_count
            .checked_add(1)
            .ok_or_else(|| Error::new("state match expansion iteration count overflowed"))?;
        if iteration_count > MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} state match expansion did not converge within the state value limit",
                process.name
            )));
        }
    }
    clauses.extend(expanded_clauses);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn preadmit_state_match_case_return(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    variant: CheckedMessageVariantId,
    body: &FunctionBlock,
    payload_bindings: &[StepPayloadBinding],
    state_payload_bindings: &[StepStatePayloadBinding],
) -> Result<()> {
    let state_arg = match &body.returns {
        ReturnExpr::Call { name, arg }
            if name.as_str() == "Stop"
                || name.as_str() == "Continue"
                || name.as_str() == "Panic" =>
        {
            arg
        }
        _ => return Ok(()),
    };
    if matches!(state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        return Ok(());
    }

    let mut source_bindings = Vec::with_capacity(
        payload_bindings
            .len()
            .saturating_add(state_payload_bindings.len()),
    );
    for binding in payload_bindings {
        source_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    for binding in state_payload_bindings {
        source_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        semantic_index,
    };
    let state_arg = resolve_source_value_expr(
        &function_scope,
        &process.state_type,
        state_arg,
        &source_bindings,
        0,
    )?;
    let uses_payload = payload_bindings
        .iter()
        .any(|binding| source_value_uses_binding(&state_arg, &binding.name));
    let uses_state = state_payload_bindings
        .iter()
        .any(|binding| source_value_uses_binding(&state_arg, &binding.name));
    if !uses_payload && !uses_state {
        state_space.resolve_state_value(semantic_index, types, &state_arg)?;
        return Ok(());
    }

    if uses_payload && !payload_bindings.is_empty() {
        for payload in message_cases.payload_values(process_id, variant)? {
            let mut owned_bindings = Vec::new();
            for binding in payload_bindings {
                let (label, value) = checked_payload_binding(
                    module,
                    semantic_index,
                    payload,
                    &PatternPayloadParam {
                        name: binding.name.clone(),
                        ty: binding.ty.clone(),
                        path: binding.path.clone(),
                    },
                )?
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} message payload {} does not match state match binding {}",
                        process.name,
                        payload.label(),
                        binding.name
                    ))
                })?;
                owned_bindings.push(DiscoveryValueBinding {
                    name: binding.name.clone(),
                    ty: binding.ty.clone(),
                    label,
                    value,
                });
            }
            for binding in state_payload_bindings {
                owned_bindings.push(DiscoveryValueBinding {
                    name: binding.name.clone(),
                    ty: binding.ty.clone(),
                    label: binding.label.clone(),
                    value: Some(binding.value.clone()),
                });
            }
            let value_bindings = owned_bindings
                .iter()
                .map(|binding| ValueBinding {
                    name: &binding.name,
                    ty: &binding.ty,
                    label: binding.label.clone(),
                    value: binding.value.clone(),
                })
                .collect::<Vec<_>>();
            state_space.resolve_state_value_with_bindings(
                semantic_index,
                types,
                &state_arg,
                &value_bindings,
            )?;
        }
        return Ok(());
    }

    let owned_bindings = state_payload_bindings
        .iter()
        .map(|binding| DiscoveryValueBinding {
            name: binding.name.clone(),
            ty: binding.ty.clone(),
            label: binding.label.clone(),
            value: Some(binding.value.clone()),
        })
        .collect::<Vec<_>>();
    let value_bindings = owned_bindings
        .iter()
        .map(|binding| ValueBinding {
            name: &binding.name,
            ty: &binding.ty,
            label: binding.label.clone(),
            value: binding.value.clone(),
        })
        .collect::<Vec<_>>();
    state_space.resolve_state_value_with_bindings(
        semantic_index,
        types,
        &state_arg,
        &value_bindings,
    )?;
    Ok(())
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

#[allow(clippy::too_many_arguments)]
fn state_match_arm_cases(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    state_enum: &Enum,
    explicit_variants: &BTreeSet<usize>,
    pattern: &TypedMatchPattern,
) -> Result<Vec<(CheckedStateId, Vec<StepStatePayloadBinding>)>> {
    match pattern {
        TypedMatchPattern::Variant { variant, bindings } => {
            let variant_decl = &state_enum.variants[*variant];
            match &variant_decl.payload_type {
                None if bindings.is_empty() => {
                    let value = ValueExpr::Identifier(variant_decl.name.clone());
                    let state = state_space.resolve_state_value(semantic_index, types, &value)?;
                    Ok(vec![(state, Vec::new())])
                }
                None => Err(Error::new(format!(
                    "process {} state match pattern {} does not carry a payload",
                    process.name, variant_decl.name
                ))),
                Some(payload_type) => {
                    if bindings.is_empty() {
                        return Err(Error::new(format!(
                            "process {} state match pattern {} requires a payload binding",
                            process.name, variant_decl.name
                        )));
                    }
                    let checked_ty = types.intern(payload_type)?;
                    let payloads = state_match_payload_domain(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        message_cases,
                        state_space,
                        payload_type,
                        &checked_ty,
                    )?;
                    payloads
                        .into_iter()
                        .map(|payload| {
                            let payload_name = Identifier::new("__state_payload")?;
                            let state_value = ValueExpr::EnumVariant {
                                name: variant_decl.name.clone(),
                                payload: Box::new(ValueExpr::Identifier(payload_name.clone())),
                            };
                            let payload_value = payload.value().cloned().ok_or_else(|| {
                                Error::new(format!(
                                    "process {} state payload {} cannot be a process reference",
                                    process.name,
                                    payload.label()
                                ))
                            })?;
                            let state = state_space.resolve_state_value_with_bindings(
                                semantic_index,
                                types,
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
                                        module,
                                        semantic_index,
                                        &payload,
                                        binding,
                                    )?
                                    .ok_or_else(|| {
                                        Error::new(format!(
                                            "process {} state payload {} does not match binding {}",
                                            process.name,
                                            payload.label(),
                                            binding.name
                                        ))
                                    })?;
                                    let value = value.ok_or_else(|| {
                                        Error::new(format!(
                                            "process {} state payload {} does not match binding {}",
                                            process.name,
                                            payload.label(),
                                            binding.name
                                        ))
                                    })?;
                                    Ok(StepStatePayloadBinding {
                                        name: binding.name.clone(),
                                        payload_ty: payload_type.clone(),
                                        ty: binding.ty.clone(),
                                        checked_payload_ty: checked_ty.clone(),
                                        checked_ty: types.intern(&binding.ty)?,
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
                        let state =
                            state_space.resolve_state_value(semantic_index, types, &value)?;
                        cases.push((state, Vec::new()));
                    }
                    Some(payload_type) => {
                        let checked_ty = types.intern(payload_type)?;
                        let payload_name = Identifier::new("__state_payload")?;
                        let state_value = ValueExpr::EnumVariant {
                            name: variant_decl.name.clone(),
                            payload: Box::new(ValueExpr::Identifier(payload_name.clone())),
                        };
                        for payload in state_match_payload_domain(
                            module,
                            process,
                            process_id,
                            semantic_index,
                            message_cases,
                            state_space,
                            payload_type,
                            &checked_ty,
                        )? {
                            let payload_value = payload.value().cloned().ok_or_else(|| {
                                Error::new(format!(
                                    "process {} state payload {} cannot be a process reference",
                                    process.name,
                                    payload.label()
                                ))
                            })?;
                            let state = state_space.resolve_state_value_with_bindings(
                                semantic_index,
                                types,
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

#[allow(clippy::too_many_arguments)]
fn state_match_payload_domain(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &StateSpace<'_>,
    payload_type: &TypeRef,
    checked_payload_type: &CheckedTypeRef,
) -> Result<Vec<CheckedPayloadValue>> {
    let mut payloads = BTreeMap::new();
    for state in state_space.values() {
        if let Some(payload) = state.payload() {
            if payload.ty() == checked_payload_type {
                payloads.insert(PayloadDomainKey::from_payload(payload), payload.clone());
            }
        }
    }
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    for (variant_index, message_variant) in msg_enum.variants.iter().enumerate() {
        let Some(message_payload_type) = &message_variant.payload_type else {
            continue;
        };
        if !semantic_index.same_type(message_payload_type, payload_type) {
            continue;
        }
        let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
        for payload in message_cases.payload_values(process_id, variant_id)? {
            payloads.insert(PayloadDomainKey::from_payload(payload), payload.clone());
        }
    }
    Ok(payloads.into_values().collect())
}

fn insert_step_body_clause<'a>(
    process: &Process,
    message_variants: &[EnumVariant],
    explicit_clauses: &mut [Option<StepBodyClause<'a>>],
    wildcard_clause: &mut Option<StepBodyClause<'a>>,
    pattern: StepPattern,
    mut clause: StepBodyClause<'a>,
) -> Result<()> {
    match pattern {
        StepPattern::Variant { message, bindings } => {
            clause.payload_params = bindings;
            if explicit_clauses[message.index()].replace(clause).is_some() {
                return Err(Error::new(format!(
                    "process {} declares duplicate step pattern for message {}",
                    process.name,
                    message_variants[message.index()].name
                )));
            }
        }
        StepPattern::Wildcard => {
            if wildcard_clause.replace(clause).is_some() {
                return Err(Error::new(format!(
                    "process {} declares duplicate wildcard step pattern",
                    process.name
                )));
            }
        }
    }
    Ok(())
}

fn set_step_dispatch_style(
    process: &Process,
    dispatch_style: &mut Option<StepDispatchStyle>,
    next: StepDispatchStyle,
) -> Result<()> {
    if let Some(existing) = dispatch_style {
        if *existing != next {
            return Err(Error::new(format!(
                "process {} cannot mix match step bodies with step parameter patterns",
                process.name
            )));
        }
    } else {
        *dispatch_style = Some(next);
    }
    Ok(())
}
