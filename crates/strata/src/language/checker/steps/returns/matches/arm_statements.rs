use super::*;
use mantle_artifact::MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH;

#[derive(Clone, Copy)]
struct ArmStatementScope {
    runtime_if_depth: usize,
    in_loop_body: bool,
}

impl ArmStatementScope {
    const fn for_runtime_if_branch(self) -> Self {
        Self {
            runtime_if_depth: self.runtime_if_depth.saturating_add(1),
            in_loop_body: self.in_loop_body,
        }
    }

    const fn for_loop_body(self) -> Self {
        Self {
            runtime_if_depth: 0,
            in_loop_body: true,
        }
    }
}

pub(super) fn validate_step_return_match_arm_statements(
    process: &Process,
    statements: &[Statement],
) -> Result<()> {
    let scope = ArmStatementScope {
        runtime_if_depth: 0,
        in_loop_body: false,
    };
    for statement in statements {
        validate_step_return_match_arm_statement(process, scope, statement)?;
    }
    Ok(())
}

fn validate_step_return_match_arm_statement(
    process: &Process,
    scope: ArmStatementScope,
    statement: &Statement,
) -> Result<()> {
    match statement {
        Statement::Emit(_) | Statement::Send { .. } => {}
        Statement::LetValue { name, .. } => {
            return Err(Error::new(format!(
                "process {} step return match arm cannot bind source-local value {}",
                process.name, name
            )));
        }
        Statement::LetProcessRef { name, .. } => {
            return Err(Error::new(format!(
                "process {} step return match arm cannot bind process reference {}",
                process.name, name
            )));
        }
        Statement::IfElse {
            then_body,
            else_body,
            ..
        } => {
            if scope.runtime_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
                return Err(Error::new(format!(
                    "process {} statement-level if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH}",
                    process.name
                )));
            }
            let branch_scope = scope.for_runtime_if_branch();
            validate_step_return_match_arm_branch_statements(process, branch_scope, then_body)?;
            validate_step_return_match_arm_branch_statements(process, branch_scope, else_body)?;
        }
        Statement::ForEach { body, .. } => {
            if scope.in_loop_body {
                return Err(Error::new(format!(
                    "process {} nested for loops are not supported",
                    process.name
                )));
            }
            let body_scope = scope.for_loop_body();
            validate_step_return_match_arm_branch_statements(process, body_scope, body)?;
        }
    }
    Ok(())
}

fn validate_step_return_match_arm_branch_statements(
    process: &Process,
    scope: ArmStatementScope,
    statements: &[Statement],
) -> Result<()> {
    for statement in statements {
        match statement {
            Statement::Emit(_) | Statement::Send { .. } => {}
            Statement::LetValue { name, .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot bind source-local value {}",
                    process.name, name
                )));
            }
            Statement::LetProcessRef { name, .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot bind process reference {}",
                    process.name, name
                )));
            }
            Statement::IfElse {
                then_body,
                else_body,
                ..
            } => {
                if scope.runtime_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
                    return Err(Error::new(format!(
                        "process {} statement-level if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH}",
                        process.name
                    )));
                }
                let branch_scope = scope.for_runtime_if_branch();
                validate_step_return_match_arm_branch_statements(process, branch_scope, then_body)?;
                validate_step_return_match_arm_branch_statements(process, branch_scope, else_body)?;
            }
            Statement::ForEach { body, .. } => {
                if scope.in_loop_body {
                    return Err(Error::new(format!(
                        "process {} nested for loops are not supported",
                        process.name
                    )));
                }
                let body_scope = scope.for_loop_body();
                validate_step_return_match_arm_branch_statements(process, body_scope, body)?;
            }
        }
    }
    Ok(())
}
