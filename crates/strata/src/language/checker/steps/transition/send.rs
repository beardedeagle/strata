use super::*;

pub(super) struct ResolvedCheckedSendTarget {
    pub(super) target: CheckedSendTarget,
    pub(super) target_process: CheckedProcessId,
}

pub(super) fn resolve_checked_send_target(
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
    if let Some(binding) = context.supervisor_child_index.get(target) {
        return Ok(ResolvedCheckedSendTarget {
            target: CheckedSendTarget::SupervisorChild {
                supervisor: binding.supervisor,
                child: binding.child,
                target: binding.target,
            },
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
        "process {} sends to undeclared process reference or supervisor child {}",
        context.process.name, target
    )))
}

pub(super) struct CheckedSendMessage {
    pub(super) message: CheckedMessageId,
    pub(super) payload: Option<CheckedValueTemplate>,
}

pub(super) fn resolve_send_message_case(
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
                    process_refs: None,
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

pub(super) fn checked_send_payload_template(
    context: &StepCheckContext<'_>,
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
                    ValueTemplateSource::LoopElement(element) => {
                        CheckedValueTemplate::LoopElement {
                            ty: binding.checked_ty.clone(),
                            element,
                        }
                    }
                    ValueTemplateSource::EffectOutcome(outcome) => {
                        CheckedValueTemplate::EffectOutcome {
                            ty: binding.checked_ty.clone(),
                            outcome,
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
