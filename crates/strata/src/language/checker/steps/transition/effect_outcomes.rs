use super::*;
use crate::language::checked::{CheckedEffectOutcomeId, CheckedTypeRef};
use mantle_artifact::MAX_EFFECT_OUTCOMES_PER_TRANSITION;

pub(super) struct EffectOutcomeBinding<'a> {
    pub(super) name: &'a Identifier,
    pub(super) ty: &'a TypeRef,
    pub(super) checked_ty: CheckedTypeRef,
    pub(super) id: CheckedEffectOutcomeId,
    pub(super) path: PayloadBindingPath,
}

pub(super) fn checked_effect_outcome_bindings<'a>(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    statements: &'a [Statement],
    source_bindings: &[SourceValueBinding<'_>],
) -> Result<Vec<EffectOutcomeBinding<'a>>> {
    let mut bindings = Vec::new();
    for statement in statements {
        let (name, ty) = match statement {
            Statement::LetSendOutcome { name, ty, .. }
            | Statement::LetSpawnOutcome { name, ty, .. } => (name, ty),
            _ => continue,
        };
        if bindings.len() >= MAX_EFFECT_OUTCOMES_PER_TRANSITION {
            return Err(Error::new(format!(
                "process {} step binds more than {MAX_EFFECT_OUTCOMES_PER_TRANSITION} effect outcomes",
                context.process.name
            )));
        }
        if bindings
            .iter()
            .any(|binding: &EffectOutcomeBinding<'_>| binding.name == name)
        {
            return Err(Error::new(format!(
                "process {} declares duplicate effect outcome binding {}",
                context.process.name, name
            )));
        }
        validate_effect_outcome_binding_name(context, source_bindings, name)?;
        let id = CheckedEffectOutcomeId::from_index(bindings.len())?;
        bindings.push(EffectOutcomeBinding {
            name,
            ty,
            checked_ty: types.intern(ty)?,
            id,
            path: PayloadBindingPath::whole(),
        });
    }
    Ok(bindings)
}

fn validate_effect_outcome_binding_name(
    context: &StepCheckContext<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    name: &Identifier,
) -> Result<()> {
    if name.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with the step state parameter",
            context.process.name, name
        )));
    }
    if context.process_ref_index.contains_key(name) {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with a process reference binding",
            context.process.name, name
        )));
    }
    if context.supervisor_child_index.contains_key(name) {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with a supervisor child binding",
            context.process.name, name
        )));
    }
    if context.semantic_index.process_id(name).is_ok() {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with a process declaration",
            context.process.name, name
        )));
    }
    if context
        .semantic_index
        .identifier_conflicts_with_declared_value(name)
    {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with a declared type or value constructor",
            context.process.name, name
        )));
    }
    if context
        .module
        .functions
        .iter()
        .any(|function| function.name == *name)
        || context
            .process
            .functions
            .iter()
            .any(|function| function.name == *name)
    {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with a source function declaration",
            context.process.name, name
        )));
    }
    if source_bindings.iter().any(|existing| existing.name == name) {
        return Err(Error::new(format!(
            "process {} effect outcome binding {} conflicts with an existing source value binding",
            context.process.name, name
        )));
    }
    Ok(())
}

pub(super) fn validate_effect_outcome_statement_order(
    context: &StepCheckContext<'_>,
    statements: &[Statement],
    bindings: &[EffectOutcomeBinding<'_>],
) -> Result<()> {
    let mut bound = Vec::new();
    let mut ordinary_effect_seen = false;
    for statement in statements {
        for binding in bindings {
            if bound.contains(&binding.name) {
                continue;
            }
            if statement_uses_effect_outcome_binding(statement, binding.name) {
                return Err(Error::new(format!(
                    "process {} effect outcome binding {} is used before it is bound",
                    context.process.name, binding.name
                )));
            }
        }
        match statement {
            Statement::LetSendOutcome { name, .. } | Statement::LetSpawnOutcome { name, .. } => {
                if ordinary_effect_seen {
                    return Err(Error::new(format!(
                        "process {} effect outcome binding {} must appear before ordinary effect statements in the step body",
                        context.process.name, name
                    )));
                }
                bound.push(name);
            }
            Statement::LetProcessRef { .. } => {}
            Statement::Emit(_)
            | Statement::LetValue { .. }
            | Statement::Send { .. }
            | Statement::IfElse { .. }
            | Statement::ForEach { .. } => {
                ordinary_effect_seen = true;
            }
        }
    }
    Ok(())
}

fn statements_use_effect_outcome_binding(statements: &[Statement], name: &Identifier) -> bool {
    statements
        .iter()
        .any(|statement| statement_uses_effect_outcome_binding(statement, name))
}

fn statement_uses_effect_outcome_binding(statement: &Statement, name: &Identifier) -> bool {
    match statement {
        Statement::Emit(_)
        | Statement::LetProcessRef { .. }
        | Statement::LetSpawnOutcome { .. } => false,
        Statement::LetValue { value, .. } => source_value_uses_binding(value, name),
        Statement::Send { payload, .. } | Statement::LetSendOutcome { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| source_value_uses_binding(payload, name)),
        Statement::IfElse {
            condition,
            then_body,
            else_body,
        } => {
            source_value_uses_binding(condition, name)
                || statements_use_effect_outcome_binding(then_body, name)
                || statements_use_effect_outcome_binding(else_body, name)
        }
        Statement::ForEach {
            collection, body, ..
        } => {
            source_value_uses_binding(collection, name)
                || statements_use_effect_outcome_binding(body, name)
        }
    }
}
