use super::super::returns::{
    StepReturnMatchPreadmitBindings, preadmit_static_step_return_match_state_values,
};
use super::*;

pub(super) fn preadmit_concrete_step_state_values(
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
                let bindings = step_pattern_source_bindings(&pattern);
                let static_bindings = step_pattern_static_match_bindings(&pattern, &bindings);
                preadmit_concrete_step_return(
                    module,
                    process,
                    semantic_index,
                    state_space,
                    types,
                    body,
                    StepReturnMatchPreadmitBindings {
                        source: &bindings,
                        static_match: static_bindings,
                    },
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
                    let bindings = step_pattern_source_bindings(&pattern);
                    let static_bindings = step_pattern_static_match_bindings(&pattern, &bindings);
                    preadmit_concrete_step_return(
                        module,
                        process,
                        semantic_index,
                        state_space,
                        types,
                        &arm.body,
                        StepReturnMatchPreadmitBindings {
                            source: &bindings,
                            static_match: static_bindings,
                        },
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
                let message_bindings = step_pattern_source_bindings(&pattern);
                let message_static_bindings =
                    step_pattern_static_match_bindings(&pattern, &message_bindings);
                for arm in arms {
                    let mut bindings = message_bindings.clone();
                    let mut static_bindings = message_static_bindings.to_vec();
                    if let TypedMatchPattern::Variant {
                        variant,
                        bindings: arm_bindings,
                        payload_guard,
                    } = &arm.pattern
                    {
                        let variant_decl = &state_enum.variants[*variant];
                        if variant_decl.payload_type.is_some()
                            && arm_bindings.is_empty()
                            && payload_guard.is_none()
                        {
                            return Err(Error::new(format!(
                                "process {} state match pattern {} requires a payload binding",
                                process.name, variant_decl.name
                            )));
                        }
                        bindings.extend(arm_bindings.iter().map(pattern_source_binding));
                        static_bindings.extend(arm_bindings.iter().map(pattern_source_binding));
                    }
                    preadmit_concrete_step_return(
                        module,
                        process,
                        semantic_index,
                        state_space,
                        types,
                        arm.body,
                        StepReturnMatchPreadmitBindings {
                            source: &bindings,
                            static_match: &static_bindings,
                        },
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
        if !domain.values.iter().any(|existing| existing == &value) {
            domain.values.push(value);
        }
        return;
    }
    domains.push(ConcreteStatePayloadDomain {
        ty,
        values: vec![value],
    });
}

fn step_pattern_source_bindings(pattern: &StepPattern) -> Vec<SourceValueBinding<'_>> {
    let StepPattern::Variant { bindings, .. } = pattern else {
        return Vec::new();
    };
    bindings.iter().map(pattern_source_binding).collect()
}

fn step_pattern_static_match_bindings<'a, 'binding>(
    pattern: &StepPattern,
    bindings: &'a [SourceValueBinding<'binding>],
) -> &'a [SourceValueBinding<'binding>] {
    match pattern {
        StepPattern::Variant {
            payload_guard: Some(_),
            ..
        } => bindings,
        StepPattern::Variant { .. } | StepPattern::Wildcard => &[],
    }
}

fn pattern_source_binding(binding: &PatternPayloadParam) -> SourceValueBinding<'_> {
    SourceValueBinding {
        name: &binding.name,
        ty: &binding.ty,
    }
}

fn preadmit_concrete_step_return(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    body: &FunctionBlock,
    bindings: StepReturnMatchPreadmitBindings<'_, '_>,
) -> Result<()> {
    if let ReturnExpr::IfElse {
        then_branch,
        else_branch,
        ..
    } = &body.returns
    {
        preadmit_concrete_step_return(
            module,
            process,
            semantic_index,
            state_space,
            types,
            then_branch,
            bindings,
        )?;
        return preadmit_concrete_step_return(
            module,
            process,
            semantic_index,
            state_space,
            types,
            else_branch,
            bindings,
        );
    }

    if let ReturnExpr::Match(match_body) = &body.returns {
        return preadmit_static_step_return_match_state_values(
            module,
            process,
            semantic_index,
            state_space,
            types,
            bindings,
            match_body,
        );
    }

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
        || bindings
            .source
            .iter()
            .any(|binding| source_value_uses_binding(state_arg, binding.name))
        || body_effect_outcome_bindings_used(body, state_arg)
    {
        return Ok(());
    }

    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        process_refs: None,
        semantic_index,
    };
    let state_arg =
        resolve_source_value_expr(&function_scope, &process.state_type, state_arg, &[], 0)?;
    state_space.resolve_state_value(semantic_index, types, &state_arg)?;
    Ok(())
}

fn body_effect_outcome_bindings_used(body: &FunctionBlock, value: &ValueExpr) -> bool {
    body.statements.iter().any(|statement| match statement {
        Statement::LetSendOutcome { name, .. } | Statement::LetSpawnOutcome { name, .. } => {
            source_value_uses_binding(value, name)
        }
        _ => false,
    })
}
