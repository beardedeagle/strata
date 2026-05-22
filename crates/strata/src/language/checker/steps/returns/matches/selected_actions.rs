use super::*;

pub(in crate::language::checker) fn selected_step_return_match_action_statements(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    match_body: &Match,
    scrutinee_ty: &TypeRef,
    scrutinee_value: &ArtifactValue,
) -> Result<Vec<Statement>> {
    let enum_decl = semantic_index
        .enum_decl(module, scrutinee_ty)
        .map_err(|_| {
            Error::new(format!(
                "process {} step return match scrutinee {} must be a concrete enum source value binding",
                process.name, match_body.scrutinee
            ))
        })?;
    let (selected_variant, selected_payload) =
        concrete_artifact_enum_value(process, enum_decl, &match_body.scrutinee, scrutinee_value)?;

    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module,
        semantic_index,
        enum_decl,
        enum_type: scrutinee_ty,
        subject: &subject,
        label: "step return match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_payload_sensitive_typed_match_arms(&pattern_context, &match_body.arms)?;
    let mut wildcard = None;
    for (arm, source_arm) in arms.iter().zip(&match_body.arms) {
        match &arm.pattern {
            TypedMatchPattern::Variant {
                variant,
                payload_guard,
                bindings,
            } => {
                if *variant != selected_variant {
                    continue;
                }
                if !artifact_payload_matches_guard(
                    module,
                    semantic_index,
                    selected_payload,
                    payload_guard.as_ref(),
                )? {
                    continue;
                }
                let substitutions = step_return_match_substitutions(
                    module,
                    semantic_index,
                    process,
                    enum_decl,
                    *variant,
                    selected_payload,
                    bindings,
                )?;
                return Ok(substitute_step_return_statements(
                    &source_arm.body.statements,
                    &substitutions,
                ));
            }
            TypedMatchPattern::Wildcard => {
                wildcard = Some(&source_arm.body.statements);
            }
        }
    }

    if let Some(statements) = wildcard {
        return Ok(statements.to_vec());
    }

    Err(Error::new(format!(
        "process {} step return match has no matching pattern for concrete {}",
        process.name,
        scrutinee_value.label()
    )))
}
