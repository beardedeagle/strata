use super::*;

pub(super) fn validate_source_function_process_ref_shadowing(
    owner: &str,
    function: &Function,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
) -> Result<()> {
    if process_refs.is_empty() {
        return Ok(());
    }
    let Some(body) = &function.body else {
        return Ok(());
    };
    validate_function_body(owner, function, process_refs, body)
}

fn validate_function_body(
    owner: &str,
    function: &Function,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
    body: &FunctionBody,
) -> Result<()> {
    match body {
        FunctionBody::Block(body) => validate_function_block(owner, function, process_refs, body),
        FunctionBody::Match(match_body) => {
            for arm in &match_body.arms {
                validate_function_block(owner, function, process_refs, &arm.body)?;
            }
            Ok(())
        }
    }
}

fn validate_function_block(
    owner: &str,
    function: &Function,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
    body: &FunctionBlock,
) -> Result<()> {
    for statement in &body.statements {
        validate_statement(owner, function, process_refs, statement)?;
    }
    validate_return_expr(owner, function, process_refs, &body.returns)
}

fn validate_statement(
    owner: &str,
    function: &Function,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
    statement: &Statement,
) -> Result<()> {
    match statement {
        Statement::LetValue { name, .. } if process_refs.contains_key(name) => {
            Err(source_binding_process_ref_conflict(owner, function, name))
        }
        Statement::IfElse {
            then_body,
            else_body,
            ..
        } => {
            for statement in then_body {
                validate_statement(owner, function, process_refs, statement)?;
            }
            for statement in else_body {
                validate_statement(owner, function, process_refs, statement)?;
            }
            Ok(())
        }
        Statement::ForEach { body, .. } => {
            for statement in body {
                validate_statement(owner, function, process_refs, statement)?;
            }
            Ok(())
        }
        Statement::LetValue { .. }
        | Statement::Emit(_)
        | Statement::LetProcessRef { .. }
        | Statement::Send { .. } => Ok(()),
    }
}

fn validate_return_expr(
    owner: &str,
    function: &Function,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
    returns: &ReturnExpr,
) -> Result<()> {
    match returns {
        ReturnExpr::Match(match_body) => {
            for arm in &match_body.arms {
                validate_function_block(owner, function, process_refs, &arm.body)?;
            }
            Ok(())
        }
        ReturnExpr::IfElse {
            then_branch,
            else_branch,
            ..
        } => {
            validate_function_block(owner, function, process_refs, then_branch)?;
            validate_function_block(owner, function, process_refs, else_branch)
        }
        ReturnExpr::Value(_) | ReturnExpr::Call { .. } => Ok(()),
    }
}

fn source_binding_process_ref_conflict(
    owner: &str,
    function: &Function,
    name: &Identifier,
) -> Error {
    Error::new(format!(
        "{owner} function {} source-local binding {name} conflicts with a process reference binding",
        function.name
    ))
}
