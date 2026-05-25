use super::*;
use crate::language::checker::source_functions::collection_patterns::{
    check_collection_pattern_bindings, collection_pattern_capacity, collection_pattern_shape,
    collection_shape_label, first_overlapping_collection_pattern,
};
use crate::language::checker::source_functions::record_patterns::check_record_pattern_bindings;

pub(super) fn validate_source_function_body_match_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let FunctionParam::Binding(param) = &function.params[0] else {
        return Err(Error::new(format!(
            "function {} match body requires a binding parameter",
            function.name
        )));
    };
    if match_body.scrutinee != param.name {
        return Err(Error::new(format!(
            "function {} match scrutinee {} must be parameter {}",
            function.name, match_body.scrutinee, param.name
        )));
    }
    let scrutinee = bindings
        .iter()
        .find(|binding| *binding.name == match_body.scrutinee)
        .ok_or_else(|| {
            Error::new(format!(
                "function {} match scrutinee {} must be a source value binding",
                function.name, match_body.scrutinee
            ))
        })?;

    if let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, scrutinee.ty) {
        return validate_source_function_body_enum_match_values(
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
        return validate_source_function_body_record_match_values(
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
        return validate_source_function_body_collection_match_values(
            scope,
            function,
            expected_type,
            match_body,
            bindings,
            scrutinee.ty,
        );
    }

    Err(Error::new(format!(
        "function {} match scrutinee {} must be a declared record, enum, list, or map source value",
        function.name, match_body.scrutinee
    )))
}

fn validate_source_function_body_enum_match_values(
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
        label: "match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    for arm in &match_body.arms {
        let payload_bindings = match check_typed_match_pattern(&pattern_context, &arm.pattern)? {
            TypedMatchPattern::Variant { bindings, .. } => bindings,
            TypedMatchPattern::Wildcard => Vec::new(),
        };
        validate_source_pattern_binding_scope_conflicts(
            scope,
            &format!("function {} match payload binding", function.name),
            &payload_bindings,
        )?;
        let mut arm_bindings = bindings.to_vec();
        for payload in &payload_bindings {
            if bindings.iter().any(|binding| binding.name == &payload.name) {
                return Err(Error::new(format!(
                    "function {} match payload binding {} conflicts with an existing source value binding",
                    function.name, payload.name
                )));
            }
            arm_bindings.push(SourceValueBinding {
                name: &payload.name,
                ty: &payload.ty,
            });
        }
        validate_source_function_block_values(
            scope,
            function,
            expected_type,
            &arm.body,
            &arm_bindings,
        )?;
    }
    Ok(())
}

fn validate_source_function_body_record_match_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
    record_decl: &Record,
) -> Result<()> {
    let [arm] = match_body.arms.as_slice() else {
        return Err(Error::new(format!(
            "function {} match record pattern {} must declare exactly one arm",
            function.name, record_decl.name
        )));
    };
    let Pattern::Record { name, fields } = &arm.pattern else {
        return Err(source_function_record_body_match_pattern_error(
            function,
            &arm.pattern,
            record_decl,
        ));
    };
    if name != &record_decl.name {
        return Err(Error::new(format!(
            "function {} match record pattern {} cannot match record {}",
            function.name, name, record_decl.name
        )));
    }

    let subject = format!("function {} match", function.name);
    let pattern_bindings =
        check_record_pattern_bindings(scope.semantic_index, &subject, record_decl, fields)?;
    validate_source_pattern_binding_scope_conflicts(
        scope,
        &format!("function {} match record pattern binding", function.name),
        &pattern_bindings,
    )?;
    let mut arm_bindings = bindings.to_vec();
    for binding in &pattern_bindings {
        if bindings
            .iter()
            .any(|existing| existing.name == &binding.name)
        {
            return Err(Error::new(format!(
                "function {} match record pattern binding {} conflicts with an existing source value binding",
                function.name, binding.name
            )));
        }
        arm_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }

    validate_source_function_block_values(scope, function, expected_type, &arm.body, &arm_bindings)
}

fn source_function_record_body_match_pattern_error(
    function: &Function,
    pattern: &Pattern,
    record_decl: &Record,
) -> Error {
    match pattern {
        Pattern::Constructor { name, .. } => Error::new(format!(
            "function {} match pattern {} expects an enum constructor, but scrutinee is record {}",
            function.name, name, record_decl.name
        )),
        Pattern::Record { .. } => Error::new(format!(
            "function {} match record pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Wildcard => Error::new(format!(
            "function {} match over record {} cannot use a wildcard pattern",
            function.name, record_decl.name
        )),
        Pattern::List(_) => Error::new(format!(
            "function {} match list pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Map(_) => Error::new(format!(
            "function {} match map pattern cannot match record {}",
            function.name, record_decl.name
        )),
    }
}

fn validate_source_function_body_collection_match_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    match_body: &Match,
    bindings: &[SourceValueBinding<'_>],
    collection_type: &TypeRef,
) -> Result<()> {
    let mut wildcard_seen = false;
    let mut shapes = Vec::new();
    let capacity = collection_pattern_capacity(scope.semantic_index, collection_type)?;
    for arm in &match_body.arms {
        let mut arm_bindings = bindings.to_vec();
        match &arm.pattern {
            Pattern::Wildcard => {
                if wildcard_seen {
                    return Err(Error::new(format!(
                        "function {} match declares duplicate wildcard pattern",
                        function.name
                    )));
                }
                wildcard_seen = true;
            }
            Pattern::List(_) | Pattern::Map(_) => {
                let subject = format!("function {} match", function.name);
                let pattern_bindings = check_collection_pattern_bindings(
                    scope.module,
                    scope.semantic_index,
                    &subject,
                    collection_type,
                    &arm.pattern,
                )?;
                validate_source_pattern_binding_scope_conflicts(
                    scope,
                    &format!(
                        "function {} match collection pattern binding",
                        function.name
                    ),
                    &pattern_bindings,
                )?;
                let shape = collection_pattern_shape(
                    scope.module,
                    scope.semantic_index,
                    collection_type,
                    &arm.pattern,
                )?;
                if let Some(overlap) =
                    first_overlapping_collection_pattern(&shapes, &shape, capacity)
                {
                    return Err(Error::new(format!(
                        "function {} match declares overlapping collection patterns {} and {}",
                        function.name,
                        collection_shape_label(overlap),
                        collection_shape_label(&shape)
                    )));
                }
                shapes.push(shape);
                for binding in &pattern_bindings {
                    if bindings
                        .iter()
                        .any(|existing| existing.name == &binding.name)
                    {
                        return Err(Error::new(format!(
                            "function {} match collection pattern binding {} conflicts with an existing source value binding",
                            function.name, binding.name
                        )));
                    }
                    arm_bindings.push(SourceValueBinding {
                        name: &binding.name,
                        ty: &binding.ty,
                    });
                }
                validate_source_function_block_values(
                    scope,
                    function,
                    expected_type,
                    &arm.body,
                    &arm_bindings,
                )?;
                continue;
            }
            Pattern::Constructor { name, .. } => {
                return Err(Error::new(format!(
                    "function {} match pattern {} expects an enum constructor, but scrutinee is {}",
                    function.name, name, collection_type
                )));
            }
            Pattern::Record { name, .. } => {
                return Err(Error::new(format!(
                    "function {} match pattern {} destructures a record, but scrutinee is {}",
                    function.name, name, collection_type
                )));
            }
        }
        validate_source_function_block_values(
            scope,
            function,
            expected_type,
            &arm.body,
            &arm_bindings,
        )?;
    }
    Ok(())
}
