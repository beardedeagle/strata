use super::*;

pub(super) fn check_step_transition(
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    input: StepTransitionInput<'_>,
) -> Result<CheckedTransition> {
    let payload_template_bindings =
        input
            .payload_bindings
            .iter()
            .map(|binding| ValueTemplateBinding {
                name: &binding.name,
                ty: &binding.ty,
                checked_ty: &binding.checked_ty,
                root_checked_ty: &binding.checked_payload_ty,
                source: ValueTemplateSource::ReceivedPayload,
                path: &binding.path,
            });
    let state_template_bindings =
        input
            .state_payload_bindings
            .iter()
            .map(|binding| ValueTemplateBinding {
                name: &binding.name,
                ty: &binding.ty,
                checked_ty: &binding.checked_ty,
                root_checked_ty: &binding.checked_payload_ty,
                source: ValueTemplateSource::CurrentStatePayload,
                path: &binding.path,
            });
    let template_bindings = payload_template_bindings
        .chain(state_template_bindings)
        .collect::<Vec<_>>();
    let function_scope = SourceFunctionScope {
        module: context.module,
        process_name: Some(&context.process.name),
        process_functions: &context.process.functions,
        semantic_index: context.semantic_index,
    };
    let mut source_bindings = Vec::new();
    for binding in input.payload_bindings {
        source_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    for binding in input.state_payload_bindings {
        source_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    let mut actions = Vec::with_capacity(input.body.statements.len());
    for statement in &input.body.statements {
        match statement {
            Statement::Emit(text) => {
                actions.push(CheckedAction::Emit {
                    output: outputs.intern(text.as_str())?,
                });
            }
            Statement::LetProcessRef { name, target, .. } => {
                let binding = context.process_ref_index.get(name).ok_or_else(|| {
                    Error::new(format!(
                        "process {} process reference {} was not resolved",
                        context.process.name, name
                    ))
                })?;
                actions.push(CheckedAction::Spawn {
                    target: context.semantic_index.process_id(target)?,
                    process_ref: binding.id,
                });
            }
            Statement::Send {
                target,
                message,
                payload,
            } => {
                let send_target =
                    resolve_checked_send_target(context, input.payload_bindings, target)?;
                let message_id = resolve_send_message_case(
                    context,
                    types,
                    send_target.target_process,
                    message,
                    payload.as_ref(),
                    &source_bindings,
                    &template_bindings,
                )?;
                actions.push(CheckedAction::Send {
                    target: send_target.target,
                    message: message_id.message,
                    payload: message_id.payload,
                });
            }
        }
    }

    let (step_result, state_arg) = match &input.body.returns {
        ReturnExpr::Call { name, arg } if name.as_str() == "Stop" => (CheckedStepResult::Stop, arg),
        ReturnExpr::Call { name, arg } if name.as_str() == "Continue" => {
            (CheckedStepResult::Continue, arg)
        }
        ReturnExpr::Call { name, arg } if name.as_str() == "Panic" => {
            (CheckedStepResult::Panic, arg)
        }
        ReturnExpr::Match(_) => {
            return Err(Error::new(format!(
                "process {} step return match is not supported in this source slice",
                context.process.name
            )));
        }
        _ => {
            return Err(Error::new(
                "step body must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)",
            ));
        }
    };
    let state_arg = resolve_source_value_expr(
        &function_scope,
        &context.process.state_type,
        state_arg,
        &source_bindings,
        0,
    )?;
    let next_state = if matches!(&state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        CheckedNextState::Current
    } else if template_bindings
        .iter()
        .any(|binding| source_value_uses_binding(&state_arg, binding.name))
    {
        let template = checked_value_template_with_binding(
            context.module,
            context.semantic_index,
            types,
            &context.process.state_type,
            &state_arg,
            &template_bindings,
        )?;
        populate_template_state_values(
            context,
            state_space,
            types,
            input.variant,
            &state_arg,
            input.payload_bindings,
            input.state_payload_bindings,
        )?;
        CheckedNextState::Template(template)
    } else {
        CheckedNextState::Value(state_space.resolve_state_value(
            context.semantic_index,
            types,
            &state_arg,
        )?)
    };

    Ok(CheckedTransition::new(CheckedTransitionParts {
        current_state: input.current_state,
        message: input.message,
        step_result,
        next_state,
        effects: input.declared_effects.to_vec(),
        actions,
    }))
}

struct ResolvedCheckedSendTarget {
    target: CheckedSendTarget,
    target_process: CheckedProcessId,
}

fn resolve_checked_send_target(
    context: &StepCheckContext<'_>,
    payload_bindings: &[StepPayloadBinding],
    target: &Identifier,
) -> Result<ResolvedCheckedSendTarget> {
    if let Some(binding) = context.process_ref_index.get(target) {
        return Ok(ResolvedCheckedSendTarget {
            target: CheckedSendTarget::ProcessRef(binding.id),
            target_process: binding.target,
        });
    }
    if let Some(binding) = payload_bindings
        .iter()
        .find(|binding| binding.name == *target)
    {
        let target_process = context
            .semantic_index
            .process_ref_target_type(&binding.ty)?
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} send target {} is not a process reference payload",
                    context.process.name, target
                ))
            })?;
        return Ok(ResolvedCheckedSendTarget {
            target: CheckedSendTarget::ReceivedPayload {
                ty: binding.checked_ty.clone(),
                target: target_process,
            },
            target_process,
        });
    }
    Err(Error::new(format!(
        "process {} sends to undeclared process reference {}",
        context.process.name, target
    )))
}

struct CheckedSendMessage {
    message: CheckedMessageId,
    payload: Option<CheckedValueTemplate>,
}

fn resolve_send_message_case(
    context: &mut StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    target_process: CheckedProcessId,
    message: &Identifier,
    payload: Option<&ValueExpr>,
    source_bindings: &[SourceValueBinding<'_>],
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<CheckedSendMessage> {
    let variant = context.semantic_index.message_id_for_process(
        context.module,
        context.process.name.as_str(),
        target_process,
        message,
    )?;
    let variant_decl =
        context
            .semantic_index
            .message_variant(context.module, target_process, variant)?;
    let payload = match (&variant_decl.payload_type, payload) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(Error::new(format!(
                "process {} sends payload to message {}, which does not accept one",
                context.process.name, variant_decl.name
            )));
        }
        (Some(_), None) => {
            return Err(Error::new(format!(
                "process {} sends message {} without required payload",
                context.process.name, variant_decl.name
            )));
        }
        (Some(payload_type), Some(payload)) => {
            let resolved_payload = {
                let function_scope = SourceFunctionScope {
                    module: context.module,
                    process_name: Some(&context.process.name),
                    process_functions: &context.process.functions,
                    semantic_index: context.semantic_index,
                };
                resolve_source_value_expr(
                    &function_scope,
                    payload_type,
                    payload,
                    source_bindings,
                    0,
                )?
            };
            Some(checked_send_payload_template(
                context,
                types,
                payload_type,
                &resolved_payload,
                bindings,
            )?)
        }
    };
    Ok(CheckedSendMessage {
        message: context.message_cases.message_id(target_process, variant)?,
        payload,
    })
}

fn checked_send_payload_template(
    context: &mut StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    payload: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<CheckedValueTemplate> {
    if let Some(target_process) = context
        .semantic_index
        .process_ref_target_type(expected_type)?
    {
        let ValueExpr::Identifier(name) = payload else {
            return Err(Error::new(format!(
                "process {} sends process reference payload of type {} using a non-reference value",
                context.process.name, expected_type
            )));
        };
        if let Some(binding) = bindings.iter().find(|binding| name == binding.name) {
            if binding.ty == expected_type {
                return Ok(match binding.source {
                    ValueTemplateSource::ReceivedPayload => CheckedValueTemplate::ReceivedPayload {
                        ty: binding.checked_ty.clone(),
                    },
                    ValueTemplateSource::CurrentStatePayload => {
                        CheckedValueTemplate::CurrentStatePayload {
                            ty: binding.checked_ty.clone(),
                        }
                    }
                });
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
        let process_ref = context.process_ref_index.get(name).ok_or_else(|| {
            Error::new(format!(
                "process {} payload {} is not a bound process reference",
                context.process.name, name
            ))
        })?;
        if process_ref.target != target_process {
            return Err(Error::new(format!(
                "process {} payload {} targets process id {}, expected {}",
                context.process.name,
                name,
                process_ref.target.as_u32(),
                target_process.as_u32()
            )));
        }
        return Ok(CheckedValueTemplate::ProcessRef {
            ty: types.intern(expected_type)?,
            target: target_process,
            process_ref: process_ref.id,
        });
    }

    checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        expected_type,
        payload,
        bindings,
    )
}

fn populate_template_state_values(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    variant: CheckedMessageVariantId,
    state_arg: &ValueExpr,
    payload_bindings: &[StepPayloadBinding],
    state_payload_bindings: &[StepStatePayloadBinding],
) -> Result<()> {
    if !payload_bindings.is_empty() {
        for payload in context
            .message_cases
            .payload_values(context.process_id, variant)?
        {
            let payload_labels = payload_bindings
                .iter()
                .map(|binding| {
                    let projected = payload_binding_label(
                        payload.label(),
                        &PatternPayloadParam {
                            name: binding.name.clone(),
                            ty: binding.ty.clone(),
                            path: binding.path.clone(),
                        },
                    )?;
                    projected.ok_or_else(|| {
                        Error::new(format!(
                            "process {} message payload {} does not match step pattern binding {}",
                            context.process.name,
                            payload.label(),
                            binding.name
                        ))
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let mut bindings = Vec::new();
            for (binding, label) in payload_bindings.iter().zip(&payload_labels) {
                bindings.push(ValueBinding {
                    name: &binding.name,
                    ty: &binding.ty,
                    label,
                });
            }
            for state_binding in state_payload_bindings {
                bindings.push(ValueBinding {
                    name: &state_binding.name,
                    ty: &state_binding.ty,
                    label: &state_binding.label,
                });
            }
            state_space.resolve_state_value_with_bindings(
                context.semantic_index,
                types,
                state_arg,
                &bindings,
            )?;
        }
        return Ok(());
    }
    if !state_payload_bindings.is_empty() {
        let bindings = state_payload_bindings
            .iter()
            .map(|binding| ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: &binding.label,
            })
            .collect::<Vec<_>>();
        state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            types,
            state_arg,
            &bindings,
        )?;
    }
    Ok(())
}
