use super::*;

pub(in crate::language::checker) fn static_step_return_match_arm_state_args(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    source_bindings: &[SourceValueBinding<'_>],
    static_match_bindings: &[SourceValueBinding<'_>],
    match_body: &Match,
) -> Result<Vec<ValueExpr>> {
    let Some(scrutinee_binding) = static_match_bindings
        .iter()
        .find(|binding| *binding.name == match_body.scrutinee)
    else {
        return Ok(Vec::new());
    };
    let Ok(enum_decl) = semantic_index.enum_decl(module, scrutinee_binding.ty) else {
        return Ok(Vec::new());
    };
    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module,
        semantic_index,
        enum_decl,
        enum_type: scrutinee_binding.ty,
        subject: &subject,
        label: "step return match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_payload_sensitive_typed_match_arms(&pattern_context, &match_body.arms)?;
    if arms.len() != match_body.arms.len() {
        return Err(Error::new(format!(
            "process {} step return match typed arm count does not match source arm count",
            process.name
        )));
    }

    let mut state_args = Vec::with_capacity(arms.len());
    for (arm, source_arm) in arms.iter().zip(&match_body.arms) {
        if let Some(state_arg) = static_step_return_match_arm_state_arg(
            module,
            semantic_index,
            source_bindings,
            enum_decl,
            match_body,
            &arm.pattern,
            &source_arm.body,
        )? {
            state_args.push(state_arg);
        }
    }
    Ok(state_args)
}

fn static_step_return_match_arm_state_arg(
    module: &Module,
    semantic_index: &SemanticIndex,
    source_bindings: &[SourceValueBinding<'_>],
    enum_decl: &Enum,
    match_body: &Match,
    pattern: &TypedMatchPattern,
    body: &FunctionBlock,
) -> Result<Option<ValueExpr>> {
    let ReturnExpr::Call { name, arg } = &body.returns else {
        return Ok(None);
    };
    if name.as_str() != "Stop" && name.as_str() != "Continue" && name.as_str() != "Panic" {
        return Ok(None);
    }

    let mut state_arg = arg.clone();
    if let Some(substitution) =
        static_scrutinee_substitution(module, semantic_index, enum_decl, match_body, pattern)?
    {
        state_arg = substitute_step_return_bindings(state_arg, std::slice::from_ref(&substitution));
    }
    if matches!(&state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
        || source_value_uses_any_binding(&state_arg, source_bindings)
        || step_return_match_arm_uses_binding(&state_arg, pattern)
    {
        return Ok(None);
    }
    Ok(Some(state_arg))
}

fn static_scrutinee_substitution<'a>(
    module: &Module,
    semantic_index: &SemanticIndex,
    enum_decl: &Enum,
    match_body: &'a Match,
    pattern: &TypedMatchPattern,
) -> Result<Option<StepReturnSubstitution<'a>>> {
    let TypedMatchPattern::Variant {
        variant,
        payload_guard,
        ..
    } = pattern
    else {
        return Ok(None);
    };
    let Some(value) =
        static_scrutinee_value_for_arm(module, semantic_index, enum_decl, *variant, payload_guard)?
    else {
        return Ok(None);
    };
    Ok(Some(StepReturnSubstitution {
        name: &match_body.scrutinee,
        value,
    }))
}

fn static_scrutinee_value_for_arm(
    module: &Module,
    semantic_index: &SemanticIndex,
    enum_decl: &Enum,
    variant: usize,
    payload_guard: &Option<PatternPayloadGuard>,
) -> Result<Option<ValueExpr>> {
    let variant = enum_decl.variants.get(variant).ok_or_else(|| {
        Error::new(format!(
            "step return match static preadmission references missing variant id {variant}"
        ))
    })?;
    let Some(payload_type) = &variant.payload_type else {
        if payload_guard.is_some() {
            return Err(Error::new(format!(
                "step return match static preadmission variant {} does not carry a payload",
                variant.name
            )));
        }
        return Ok(Some(ValueExpr::Identifier(variant.name.clone())));
    };
    let Some(payload_guard) = payload_guard else {
        return Ok(None);
    };
    let Some(payload) =
        static_payload_guard_value(module, semantic_index, payload_type, payload_guard)?
    else {
        return Ok(None);
    };
    Ok(Some(ValueExpr::EnumVariant {
        name: variant.name.clone(),
        payload: Box::new(payload),
    }))
}

fn static_payload_guard_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    guard: &PatternPayloadGuard,
) -> Result<Option<ValueExpr>> {
    if !semantic_index.same_type(expected_type, &guard.enum_ty) {
        return Err(Error::new(format!(
            "step return match static preadmission payload guard has type {}, expected {}",
            guard.enum_ty, expected_type
        )));
    }
    let enum_decl = semantic_index.enum_decl(module, expected_type)?;
    let variant = enum_decl
        .variants
        .get(guard.variant.index())
        .ok_or_else(|| {
            Error::new(format!(
                "step return match static preadmission references missing nested variant id {}",
                guard.variant.as_u32()
            ))
        })?;
    let Some(payload_type) = &variant.payload_type else {
        if guard.payload.is_some() {
            return Err(Error::new(format!(
                "step return match static preadmission nested variant {} does not carry a payload",
                variant.name
            )));
        }
        return Ok(Some(ValueExpr::Identifier(variant.name.clone())));
    };
    let Some(payload_guard) = guard.payload.as_deref() else {
        return Ok(None);
    };
    let Some(payload) =
        static_payload_guard_value(module, semantic_index, payload_type, payload_guard)?
    else {
        return Ok(None);
    };
    Ok(Some(ValueExpr::EnumVariant {
        name: variant.name.clone(),
        payload: Box::new(payload),
    }))
}

fn source_value_uses_any_binding(value: &ValueExpr, bindings: &[SourceValueBinding<'_>]) -> bool {
    bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
}

fn step_return_match_arm_uses_binding(value: &ValueExpr, pattern: &TypedMatchPattern) -> bool {
    let TypedMatchPattern::Variant { bindings, .. } = pattern else {
        return false;
    };
    bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, &binding.name))
}
