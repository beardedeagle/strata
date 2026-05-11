use super::*;

pub(in crate::language::checker) fn step_discovery_clauses<'a>(
    module: &Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &'a Function,
) -> Result<Vec<StepDiscoveryClause<'a>>> {
    let Some(body) = &step.body else {
        return Ok(Vec::new());
    };
    match check_step_shape(module, process, process_id, semantic_index, step)? {
        StepDispatchForm::ParameterPattern(pattern) => {
            let FunctionBody::Block(body) = body else {
                return Err(Error::new("step parameter pattern must use a block body"));
            };
            Ok(vec![StepDiscoveryClause {
                pattern,
                body,
                state_payload_bindings: Vec::new(),
            }])
        }
        StepDispatchForm::BodyMatch => {
            let FunctionBody::Match(match_body) = body else {
                return Err(Error::new("match step must use a match body"));
            };
            match_body
                .arms
                .iter()
                .map(|arm| {
                    Ok(StepDiscoveryClause {
                        pattern: check_step_pattern(
                            module,
                            process,
                            process_id,
                            semantic_index,
                            &arm.pattern,
                        )?,
                        body: &arm.body,
                        state_payload_bindings: Vec::new(),
                    })
                })
                .collect()
        }
        StepDispatchForm::StateMatch(pattern) => {
            let FunctionBody::Match(match_body) = body else {
                return Err(Error::new("state match step must use a match body"));
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
            check_typed_match_arms(&pattern_context, &match_body.arms)?
                .into_iter()
                .map(|arm| {
                    let state_payload_bindings = match &arm.pattern {
                        TypedMatchPattern::Variant { variant, bindings } => {
                            let variant_decl = &state_enum.variants[*variant];
                            if bindings.is_empty() {
                                Vec::new()
                            } else {
                                let payload_ty =
                                    variant_decl.payload_type.clone().ok_or_else(|| {
                                        Error::new(format!(
                                            "process {} state match pattern {} does not carry a payload",
                                            process.name, variant_decl.name
                                        ))
                                    })?;
                                bindings
                                    .iter()
                                    .map(|binding| StatePayloadDiscoveryBinding {
                                        name: binding.name.clone(),
                                        payload_ty: payload_ty.clone(),
                                        ty: binding.ty.clone(),
                                        path: binding.path.clone(),
                                    })
                                    .collect()
                            }
                        }
                        TypedMatchPattern::Wildcard => Vec::new(),
                    };
                    Ok(StepDiscoveryClause {
                        pattern: pattern.clone(),
                        body: arm.body,
                        state_payload_bindings,
                    })
                })
                .collect()
        }
    }
}

pub(super) fn collect_step_blocks(step: &Function) -> Vec<&FunctionBlock> {
    match &step.body {
        Some(FunctionBody::Block(body)) => vec![body],
        Some(FunctionBody::Match(match_body)) => {
            match_body.arms.iter().map(|arm| &arm.body).collect()
        }
        None => Vec::new(),
    }
}

fn check_step_match_scrutinee_parameter<'a>(
    process: &Process,
    step: &'a Function,
    match_scrutinee: &Identifier,
) -> Result<&'a Param> {
    let Some(FunctionParam::Binding(message_param)) = step.params.get(1) else {
        return Err(Error::new(format!(
            "process {} match step must declare a typed message parameter",
            process.name
        )));
    };
    if message_param.name.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} message parameter {} conflicts with a step parameter name",
            process.name, message_param.name
        )));
    }
    if message_param.name != *match_scrutinee {
        return Err(Error::new(format!(
            "process {} match scrutinee {} must be the step message parameter {}",
            process.name, match_scrutinee, message_param.name
        )));
    }
    Ok(message_param)
}

pub(in crate::language::checker) fn collect_explicit_step_variants(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<BTreeSet<CheckedMessageVariantId>> {
    let mut variants = BTreeSet::new();
    for step in &process.steps {
        for clause in step_discovery_clauses(module, process, process_id, semantic_index, step)? {
            if let StepPattern::Variant { message, .. } = clause.pattern {
                variants.insert(message);
            }
        }
    }
    Ok(variants)
}

pub(in crate::language::checker) fn matching_message_cases<'a>(
    cases: &'a [DiscoveredMessageCase],
    pattern: &StepPattern,
    explicit_variants: &BTreeSet<CheckedMessageVariantId>,
) -> Vec<&'a DiscoveredMessageCase> {
    cases
        .iter()
        .filter(|case| match pattern {
            StepPattern::Variant { message, .. } => case.variant() == *message,
            StepPattern::Wildcard => !explicit_variants.contains(&case.variant()),
        })
        .collect()
}

pub(in crate::language::checker) fn payload_value_bindings<'a>(
    pattern: &'a StepPattern,
    case: &'a DiscoveredMessageCase,
) -> Result<Vec<DiscoveryValueBinding>> {
    match (pattern, case.payload()) {
        (StepPattern::Variant { bindings, .. }, Some(payload)) => bindings
            .iter()
            .map(|param| {
                let (label, value) = checked_payload_binding(payload, param)?.ok_or_else(|| {
                    Error::new(format!(
                        "message payload {} does not match pattern binding {}",
                        payload.label(),
                        param.name
                    ))
                })?;
                Ok(DiscoveryValueBinding {
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                    label,
                    value,
                })
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}

pub(in crate::language::checker) fn resolve_send_target_process_for_discovery(
    process: &Process,
    semantic_index: &SemanticIndex,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
    pattern: &StepPattern,
    target: &Identifier,
) -> Result<CheckedProcessId> {
    if let Some(target_process) = process_refs.get(target) {
        return Ok(*target_process);
    }
    if let StepPattern::Variant { bindings, .. } = pattern
        && let Some(param) = bindings.iter().find(|binding| binding.name == *target)
    {
        return semantic_index
            .process_ref_target_type(&param.ty)?
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} send target {} is not a process reference payload",
                    process.name, target
                ))
            });
    }
    Err(Error::new(format!(
        "process {} sends to undeclared process reference {}",
        process.name, target
    )))
}

pub(super) fn check_step_pattern(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_pattern: &Pattern,
) -> Result<StepPattern> {
    step_pattern_from_typed(check_step_typed_pattern(
        module,
        process,
        process_id,
        semantic_index,
        message_pattern,
    )?)
}

fn check_step_typed_pattern(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_pattern: &Pattern,
) -> Result<TypedMatchPattern> {
    match message_pattern {
        Pattern::Constructor { name, payload } => {
            let message = semantic_index.message_id_for_step_pattern(module, process_id, name)?;
            let variant = semantic_index.message_variant(module, process_id, message)?;
            let bindings = check_step_payload_pattern(
                module,
                process,
                semantic_index,
                variant,
                payload.as_ref(),
            )?;
            Ok(TypedMatchPattern::Variant {
                variant: message.index(),
                bindings,
            })
        }
        Pattern::Record { name, .. } => Err(Error::new(format!(
            "process {} step pattern {name} destructures a record, but step patterns expect message constructors",
            process.name
        ))),
        Pattern::List(_) => Err(Error::new(format!(
            "process {} step pattern List[...] destructures a list, but step patterns expect message constructors",
            process.name
        ))),
        Pattern::Map(_) => Err(Error::new(format!(
            "process {} step pattern Map[...] destructures a map, but step patterns expect message constructors",
            process.name
        ))),
        Pattern::Wildcard => Ok(TypedMatchPattern::Wildcard),
    }
}

fn step_pattern_from_typed(pattern: TypedMatchPattern) -> Result<StepPattern> {
    match pattern {
        TypedMatchPattern::Variant { variant, bindings } => Ok(StepPattern::Variant {
            message: CheckedMessageVariantId::from_index(variant)?,
            bindings,
        }),
        TypedMatchPattern::Wildcard => Ok(StepPattern::Wildcard),
    }
}

pub(in crate::language::checker) fn check_step_shape(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &Function,
) -> Result<StepDispatchForm> {
    if step.params.len() != 2 {
        return Err(Error::new(
            "step must declare state parameter and message pattern",
        ));
    }
    let FunctionParam::Binding(state_param) = &step.params[0] else {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    };
    if state_param.name.as_str() != STEP_STATE_PARAMETER_NAME
        || !semantic_index.same_type(&state_param.ty, &process.state_type)
    {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    }
    if !semantic_index.is_proc_result_of(&step.return_type, &process.state_type) {
        return Err(Error::new(format!(
            "step returns {}, expected {}",
            step.return_type,
            format_args!("{PROC_RESULT_TYPE}<{}>", process.state_type)
        )));
    }
    if !step.may.is_empty() {
        return Err(Error::new("step may-behaviors must be empty"));
    }
    if step.determinism != Determinism::Det {
        return Err(Error::new("step must be deterministic"));
    }

    if let Some(FunctionBody::Match(match_body)) = &step.body {
        if match_body.scrutinee.as_str() == STEP_STATE_PARAMETER_NAME {
            let FunctionParam::Pattern(message_pattern) = &step.params[1] else {
                return Err(Error::new(
                    "state match step second parameter must be a message constructor pattern or wildcard pattern",
                ));
            };
            return Ok(StepDispatchForm::StateMatch(check_step_pattern(
                module,
                process,
                process_id,
                semantic_index,
                message_pattern,
            )?));
        }
        let message_param =
            check_step_match_scrutinee_parameter(process, step, &match_body.scrutinee)?;
        if !semantic_index.same_type(&message_param.ty, &process.msg_type) {
            return Err(Error::new(format!(
                "process {} message parameter {} has type {}, expected {}",
                process.name, message_param.name, message_param.ty, process.msg_type
            )));
        }
        return Ok(StepDispatchForm::BodyMatch);
    }

    let FunctionParam::Pattern(message_pattern) = &step.params[1] else {
        return Err(Error::new(
            "step second parameter must be a message constructor pattern or wildcard pattern",
        ));
    };
    Ok(StepDispatchForm::ParameterPattern(check_step_pattern(
        module,
        process,
        process_id,
        semantic_index,
        message_pattern,
    )?))
}

fn check_step_payload_pattern(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    payload: Option<&ConstructorPayloadPattern>,
) -> Result<Vec<PatternPayloadParam>> {
    check_pattern_payload_bindings(
        module,
        semantic_index,
        variant,
        payload,
        "step pattern",
        PatternPayloadContext::StepPattern,
        PatternBindingContext::Step { process },
    )
}

pub(in crate::language::checker) fn pattern_binding_subject(
    context: PatternBindingContext<'_>,
) -> String {
    match context {
        PatternBindingContext::Step { process } => format!("process {}", process.name),
        PatternBindingContext::Source { owner } => owner.to_string(),
    }
}

pub(in crate::language::checker) fn validate_pattern_binding_name(
    context: PatternBindingContext<'_>,
    semantic_index: &SemanticIndex,
    binding: &Identifier,
) -> Result<()> {
    let subject = pattern_binding_subject(context);
    if binding.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "{subject} payload binding {binding} conflicts with a reserved state parameter name"
        )));
    }
    if semantic_index.process_id(binding).is_ok() {
        return Err(Error::new(format!(
            "{subject} payload binding {binding} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(binding) {
        return Err(Error::new(format!(
            "{subject} payload binding {binding} conflicts with a declared type or value constructor"
        )));
    }
    Ok(())
}
