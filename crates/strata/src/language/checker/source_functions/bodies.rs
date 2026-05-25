use super::*;

pub(in crate::language::checker::source_functions) fn validate_pure_source_function_block(
    owner: &str,
    function: &Function,
    body: &FunctionBlock,
) -> Result<()> {
    if body
        .statements
        .iter()
        .any(|statement| !matches!(statement, Statement::LetValue { .. }))
    {
        return Err(Error::new(format!(
            "{owner} function {} must not perform statements",
            function.name
        )));
    }
    Ok(())
}

pub(in crate::language::checker::source_functions) fn source_function_block(
    function: &Function,
) -> Result<&FunctionBlock> {
    match source_function_body(function)? {
        FunctionBody::Block(body) => Ok(body),
        FunctionBody::Match(_) => Err(Error::new(format!(
            "function {} pattern signature clauses must use block bodies",
            function.name
        ))),
    }
}

pub(in crate::language::checker::source_functions) fn source_function_body(
    function: &Function,
) -> Result<&FunctionBody> {
    function.body.as_ref().ok_or_else(|| {
        Error::new(format!(
            "function {} must have a body for buildable source",
            function.name
        ))
    })
}

pub(in crate::language::checker::source_functions) fn source_function_body_scope<'a>(
    scope: &SourceFunctionScope<'a>,
    function: &Function,
) -> SourceFunctionScope<'a> {
    if scope
        .module
        .functions
        .iter()
        .any(|candidate| std::ptr::eq(candidate, function))
    {
        SourceFunctionScope {
            module: scope.module,
            process_name: None,
            process_functions: &[],
            process_refs: None,
            semantic_index: scope.semantic_index,
        }
    } else {
        *scope
    }
}
