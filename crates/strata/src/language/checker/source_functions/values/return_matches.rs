use super::*;
use crate::language::checker::source_functions::collection_patterns::{
    check_collection_pattern_bindings, collection_pattern_shape,
};
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
    if scope
        .semantic_index
        .collection_type(scrutinee.ty)?
        .is_some()
    {
        return validate_source_function_return_collection_match(
            scope,
            function,
            expected_type,
            match_body,
            bindings,
            scrutinee.ty,
        );
    }

    Err(Error::new(format!(
        "function {} return match scrutinee {} must be a declared record, enum, list, or map source value",
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
            bindings: payload_bindings,
            ..
        } = &arm.pattern
        {
            for payload in payload_bindings {
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
        Pattern::List(_) => Error::new(format!(
            "function {} return match list pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Map(_) => Error::new(format!(
            "function {} return match map pattern cannot match record {}",
            function.name, record_decl.name
        )),
    }
}

fn validate_source_function_return_collection_match(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
    collection_type: &TypeRef,
) -> Result<()> {
    let mut wildcard_seen = false;
    let mut shapes = BTreeSet::new();
    for arm in &match_body.arms {
        if !arm.body.statements.is_empty() {
            return Err(Error::new(format!(
                "function {} return match arms must not perform statements",
                function.name
            )));
        }
        let mut arm_bindings = bindings.to_vec();
        match &arm.pattern {
            Pattern::Wildcard => {
                if wildcard_seen {
                    return Err(Error::new(format!(
                        "function {} return match declares duplicate wildcard pattern",
                        function.name
                    )));
                }
                wildcard_seen = true;
            }
            Pattern::List(_) | Pattern::Map(_) => {
                let subject = format!("function {} return match", function.name);
                let pattern_bindings = check_collection_pattern_bindings(
                    scope.module,
                    scope.semantic_index,
                    &subject,
                    collection_type,
                    &arm.pattern,
                )?;
                let shape = collection_pattern_shape(
                    scope.module,
                    scope.semantic_index,
                    collection_type,
                    &arm.pattern,
                )?;
                if !shapes.insert(shape) {
                    return Err(Error::new(format!(
                        "function {} return match declares duplicate collection pattern",
                        function.name
                    )));
                }
                for binding in &pattern_bindings {
                    if bindings
                        .iter()
                        .any(|existing| existing.name == &binding.name)
                    {
                        return Err(Error::new(format!(
                            "function {} return match collection pattern binding {} conflicts with an existing source value binding",
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
                )?;
                continue;
            }
            Pattern::Constructor { name, .. } => {
                return Err(Error::new(format!(
                    "function {} return match pattern {} expects an enum constructor, but scrutinee is {}",
                    function.name, name, collection_type
                )));
            }
            Pattern::Record { name, .. } => {
                return Err(Error::new(format!(
                    "function {} return match pattern {} destructures a record, but scrutinee is {}",
                    function.name, name, collection_type
                )));
            }
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
