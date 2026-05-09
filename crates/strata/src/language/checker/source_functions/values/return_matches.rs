use super::*;
use crate::language::checker::source_functions::record_patterns::check_record_pattern_bindings;

pub(super) fn validate_source_function_return_match(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let scrutinee = bindings
        .iter()
        .find(|binding| *binding.name == match_body.scrutinee)
        .ok_or_else(|| {
            Error::new(format!(
                "function {} return match scrutinee {} must be a source value binding",
                function.name, match_body.scrutinee
            ))
        })?;

    if let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, scrutinee.ty) {
        return validate_source_function_return_enum_match(
            scope,
            function,
            expected_type,
            match_body,
            bindings,
            scrutinee.ty,
            enum_decl,
        );
    }

    if let Ok(record_decl) = scope.semantic_index.record_decl(scope.module, scrutinee.ty) {
        return validate_source_function_return_record_match(
            scope,
            function,
            expected_type,
            match_body,
            bindings,
            record_decl,
        );
    }

    Err(Error::new(format!(
        "function {} return match scrutinee {} must be a declared record or enum source value",
        function.name, match_body.scrutinee
    )))
}

fn validate_source_function_return_enum_match(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
    enum_type: &TypeRef,
    enum_decl: &Enum,
) -> Result<()> {
    let subject = format!("function {}", function.name);
    let pattern_context = PatternCheckContext {
        module: scope.module,
        semantic_index: scope.semantic_index,
        enum_decl,
        enum_type,
        subject: &subject,
        label: "return match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    for arm in check_typed_match_arms(&pattern_context, &match_body.arms)? {
        if !arm.body.statements.is_empty() {
            return Err(Error::new(format!(
                "function {} return match arms must not perform statements",
                function.name
            )));
        }
        let mut arm_bindings = bindings.to_vec();
        if let TypedMatchPattern::Variant {
            binding: Some(payload),
            ..
        } = &arm.pattern
        {
            if bindings.iter().any(|binding| binding.name == &payload.name) {
                return Err(Error::new(format!(
                    "function {} return match payload binding {} conflicts with an existing source value binding",
                    function.name, payload.name
                )));
            }
            arm_bindings.push(SourceValueBinding {
                name: &payload.name,
                ty: &payload.ty,
            });
        }
        validate_source_function_return_expr(
            scope,
            function,
            expected_type,
            &arm.body.returns,
            &arm_bindings,
        )?;
    }
    Ok(())
}

fn validate_source_function_return_record_match(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
    record_decl: &Record,
) -> Result<()> {
    let [arm] = match_body.arms.as_slice() else {
        return Err(Error::new(format!(
            "function {} return match record pattern {} must declare exactly one arm",
            function.name, record_decl.name
        )));
    };
    if !arm.body.statements.is_empty() {
        return Err(Error::new(format!(
            "function {} return match arms must not perform statements",
            function.name
        )));
    }

    let Pattern::Record { name, fields } = &arm.pattern else {
        return Err(source_function_record_return_match_pattern_error(
            function,
            &arm.pattern,
            record_decl,
        ));
    };
    if name != &record_decl.name {
        return Err(Error::new(format!(
            "function {} return match record pattern {} cannot match record {}",
            function.name, name, record_decl.name
        )));
    }

    let subject = format!("function {} return match", function.name);
    let pattern_bindings =
        check_record_pattern_bindings(scope.semantic_index, &subject, record_decl, fields)?;
    let mut arm_bindings = bindings.to_vec();
    for binding in &pattern_bindings {
        if bindings
            .iter()
            .any(|existing| existing.name == &binding.name)
        {
            return Err(Error::new(format!(
                "function {} return match record pattern binding {} conflicts with an existing source value binding",
                function.name, binding.name
            )));
        }
        arm_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }

    validate_source_function_return_expr(
        scope,
        function,
        expected_type,
        &arm.body.returns,
        &arm_bindings,
    )
}

fn source_function_record_return_match_pattern_error(
    function: &Function,
    pattern: &Pattern,
    record_decl: &Record,
) -> Error {
    match pattern {
        Pattern::Constructor { name, .. } => Error::new(format!(
            "function {} return match pattern {} expects an enum constructor, but scrutinee is record {}",
            function.name, name, record_decl.name
        )),
        Pattern::Record { .. } => Error::new(format!(
            "function {} return match record pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Wildcard => Error::new(format!(
            "function {} return match over record {} cannot use a wildcard pattern",
            function.name, record_decl.name
        )),
    }
}
