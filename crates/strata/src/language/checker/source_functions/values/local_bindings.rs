use super::*;

pub(super) fn validate_source_function_block_values<'a>(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    expected_type: &TypeRef,
    body: &'a FunctionBlock,
    bindings: &[SourceValueBinding<'a>],
) -> Result<()> {
    let mut block_bindings = bindings.to_vec();
    validate_source_function_local_bindings(scope, function, body, &mut block_bindings)?;
    validate_source_function_return_expr(
        scope,
        function,
        expected_type,
        &body.returns,
        &block_bindings,
    )
}

fn validate_source_function_local_bindings<'a>(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    body: &'a FunctionBlock,
    block_bindings: &mut Vec<SourceValueBinding<'a>>,
) -> Result<()> {
    for statement in &body.statements {
        let Statement::LetValue { name, ty, value } = statement else {
            return Err(Error::new(format!(
                "function {} must not perform runtime statements",
                function.name
            )));
        };
        validate_source_value_binding_name(scope, "source-local binding", block_bindings, name)?;
        if let Err(err) = scope
            .semantic_index
            .validate_source_value_type(scope.module, ty)
        {
            return Err(Error::new(format!(
                "function {} source-local binding {} must use a declared record, enum, scalar, list, or map type without process-reference authority, found {}: {}",
                function.name, name, ty, err
            )));
        }
        validate_source_function_value_expr(scope, ty, value, block_bindings).map_err(|err| {
            Error::new(format!(
                "source-local binding {name} value must produce {ty}: {err}"
            ))
        })?;
        block_bindings.push(SourceValueBinding { name, ty });
    }
    Ok(())
}
