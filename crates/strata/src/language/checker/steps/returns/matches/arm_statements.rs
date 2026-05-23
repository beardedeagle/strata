use super::*;

pub(super) fn validate_step_return_match_arm_statements(
    process: &Process,
    statements: &[Statement],
) -> Result<()> {
    let mut runtime_if_count = 0usize;
    let mut runtime_for_count = 0usize;
    for statement in statements {
        match statement {
            Statement::Emit(_) | Statement::Send { .. } => {}
            Statement::LetProcessRef { name, .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot bind process reference {} in this source slice",
                    process.name, name
                )));
            }
            Statement::IfElse {
                then_body,
                else_body,
                ..
            } => {
                runtime_if_count = runtime_if_count.saturating_add(1);
                if runtime_if_count > 1 {
                    return Err(Error::new(format!(
                        "process {} step return match arm cannot perform more than one runtime if in this source slice",
                        process.name
                    )));
                }
                validate_step_return_match_arm_runtime_if_branch(process, then_body)?;
                validate_step_return_match_arm_runtime_if_branch(process, else_body)?;
            }
            Statement::ForEach { body, .. } => {
                runtime_for_count = runtime_for_count.saturating_add(1);
                if runtime_for_count > 1 {
                    return Err(Error::new(format!(
                        "process {} step return match arm cannot perform more than one for loop in this source slice",
                        process.name
                    )));
                }
                validate_step_return_match_arm_for_body(process, body)?;
            }
        }
    }
    Ok(())
}

fn validate_step_return_match_arm_runtime_if_branch(
    process: &Process,
    statements: &[Statement],
) -> Result<()> {
    let mut runtime_for_count = 0usize;
    for statement in statements {
        match statement {
            Statement::Emit(_) | Statement::Send { .. } => {}
            Statement::LetProcessRef { name, .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot bind process reference {} in this source slice",
                    process.name, name
                )));
            }
            Statement::IfElse { .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot perform nested runtime if in this source slice",
                    process.name
                )));
            }
            Statement::ForEach { body, .. } => {
                runtime_for_count = runtime_for_count.saturating_add(1);
                if runtime_for_count > 1 {
                    return Err(Error::new(format!(
                        "process {} step return match arm cannot perform more than one for loop in this source slice",
                        process.name
                    )));
                }
                validate_step_return_match_arm_for_body(process, body)?;
            }
        }
    }
    Ok(())
}

fn validate_step_return_match_arm_for_body(
    process: &Process,
    statements: &[Statement],
) -> Result<()> {
    let mut runtime_if_count = 0usize;
    for statement in statements {
        match statement {
            Statement::Emit(_) | Statement::Send { .. } => {}
            Statement::LetProcessRef { name, .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot bind process reference {} in this source slice",
                    process.name, name
                )));
            }
            Statement::IfElse {
                then_body,
                else_body,
                ..
            } => {
                runtime_if_count = runtime_if_count.saturating_add(1);
                if runtime_if_count > 1 {
                    return Err(Error::new(format!(
                        "process {} step return match arm cannot perform more than one runtime if in this source slice",
                        process.name
                    )));
                }
                validate_step_return_match_arm_action_only_runtime_if_branch(process, then_body)?;
                validate_step_return_match_arm_action_only_runtime_if_branch(process, else_body)?;
            }
            Statement::ForEach { .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot perform nested for loops in this source slice",
                    process.name
                )));
            }
        }
    }
    Ok(())
}

fn validate_step_return_match_arm_action_only_runtime_if_branch(
    process: &Process,
    statements: &[Statement],
) -> Result<()> {
    for statement in statements {
        match statement {
            Statement::Emit(_) | Statement::Send { .. } => {}
            Statement::LetProcessRef { name, .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot bind process reference {} in this source slice",
                    process.name, name
                )));
            }
            Statement::IfElse { .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot perform nested runtime if in this source slice",
                    process.name
                )));
            }
            Statement::ForEach { .. } => {
                return Err(Error::new(format!(
                    "process {} step return match arm cannot perform for loops in this source slice",
                    process.name
                )));
            }
        }
    }
    Ok(())
}
