use super::*;
use crate::language::checker::source_functions::collection_patterns::{
    check_collection_pattern_bindings, collection_pattern_capacity, collection_pattern_shape,
    collection_shape_label, first_overlapping_collection_pattern,
};
use crate::language::checker::source_functions::record_patterns::check_record_pattern_bindings;

pub(super) fn validate_binding_source_function_match_body(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    param: &Param,
    match_body: &Match,
) -> Result<()> {
    if match_body.scrutinee != param.name {
        return Err(Error::new(format!(
            "{owner} function {} match scrutinee {} must be parameter {}",
            function.name, match_body.scrutinee, param.name
        )));
    }

    if let Ok(enum_decl) = semantic_index.enum_decl(module, &param.ty) {
        return validate_binding_source_function_enum_match_body(
            module,
            semantic_index,
            owner,
            function,
            param,
            match_body,
            enum_decl,
        );
    }
    if let Ok(record_decl) = semantic_index.record_decl(module, &param.ty) {
        return validate_binding_source_function_record_match_body(
            semantic_index,
            owner,
            function,
            match_body,
            record_decl,
        );
    }
    if semantic_index.collection_type(&param.ty)?.is_some() {
        return validate_binding_source_function_collection_match_body(
            module,
            semantic_index,
            owner,
            function,
            param,
            match_body,
        );
    }

    Err(Error::new(format!(
        "{owner} function {} match scrutinee {} must be a declared record, enum, list, or map source value",
        function.name, match_body.scrutinee
    )))
}

fn validate_binding_source_function_enum_match_body(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    param: &Param,
    match_body: &Match,
    enum_decl: &Enum,
) -> Result<()> {
    let subject = format!("{owner} function {}", function.name);
    let pattern_context = PatternCheckContext {
        module,
        semantic_index,
        enum_decl,
        enum_type: &param.ty,
        subject: &subject,
        label: "match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    for arm in check_typed_match_arms(&pattern_context, &match_body.arms)? {
        validate_pure_source_function_block(owner, function, arm.body)?;
    }
    Ok(())
}

fn validate_binding_source_function_record_match_body(
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    match_body: &Match,
    record_decl: &Record,
) -> Result<()> {
    let [arm] = match_body.arms.as_slice() else {
        return Err(Error::new(format!(
            "{owner} function {} match record pattern {} must declare exactly one arm",
            function.name, record_decl.name
        )));
    };
    validate_pure_source_function_block(owner, function, &arm.body)?;

    let Pattern::Record { name, fields } = &arm.pattern else {
        return Err(source_function_record_body_match_pattern_error(
            owner,
            function,
            &arm.pattern,
            record_decl,
        ));
    };
    if name != &record_decl.name {
        return Err(Error::new(format!(
            "{owner} function {} match record pattern {} cannot match record {}",
            function.name, name, record_decl.name
        )));
    }

    let subject = format!("{owner} function {} match", function.name);
    check_record_pattern_bindings(semantic_index, &subject, record_decl, fields)?;
    Ok(())
}

fn source_function_record_body_match_pattern_error(
    owner: &str,
    function: &Function,
    pattern: &Pattern,
    record_decl: &Record,
) -> Error {
    match pattern {
        Pattern::Constructor { name, .. } => Error::new(format!(
            "{owner} function {} match pattern {} expects an enum constructor, but scrutinee is record {}",
            function.name, name, record_decl.name
        )),
        Pattern::Record { .. } => Error::new(format!(
            "{owner} function {} match record pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Wildcard => Error::new(format!(
            "{owner} function {} match over record {} cannot use a wildcard pattern",
            function.name, record_decl.name
        )),
        Pattern::List(_) => Error::new(format!(
            "{owner} function {} match list pattern cannot match record {}",
            function.name, record_decl.name
        )),
        Pattern::Map(_) => Error::new(format!(
            "{owner} function {} match map pattern cannot match record {}",
            function.name, record_decl.name
        )),
    }
}

fn validate_binding_source_function_collection_match_body(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    param: &Param,
    match_body: &Match,
) -> Result<()> {
    let mut wildcard_seen = false;
    let capacity = collection_pattern_capacity(semantic_index, &param.ty)?;
    let mut shapes = Vec::new();
    for arm in &match_body.arms {
        validate_pure_source_function_block(owner, function, &arm.body)?;
        match &arm.pattern {
            Pattern::Wildcard => {
                if wildcard_seen {
                    return Err(Error::new(format!(
                        "{owner} function {} match declares duplicate wildcard pattern",
                        function.name
                    )));
                }
                wildcard_seen = true;
            }
            Pattern::List(_) | Pattern::Map(_) => {
                let subject = format!("{owner} function {} match", function.name);
                check_collection_pattern_bindings(
                    module,
                    semantic_index,
                    &subject,
                    &param.ty,
                    &arm.pattern,
                )?;
                let shape =
                    collection_pattern_shape(module, semantic_index, &param.ty, &arm.pattern)?;
                if let Some(overlap) =
                    first_overlapping_collection_pattern(&shapes, &shape, capacity)
                {
                    return Err(Error::new(format!(
                        "{owner} function {} match declares overlapping collection patterns {} and {}",
                        function.name,
                        collection_shape_label(overlap),
                        collection_shape_label(&shape)
                    )));
                }
                shapes.push(shape);
            }
            Pattern::Constructor { name, .. } => {
                return Err(Error::new(format!(
                    "{owner} function {} match pattern {} expects an enum constructor, but scrutinee is {}",
                    function.name, name, param.ty
                )));
            }
            Pattern::Record { name, .. } => {
                return Err(Error::new(format!(
                    "{owner} function {} match pattern {} destructures a record, but scrutinee is {}",
                    function.name, name, param.ty
                )));
            }
        }
    }
    Ok(())
}
