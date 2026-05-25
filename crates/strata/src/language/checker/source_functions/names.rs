use super::*;

pub(in crate::language::checker::source_functions) fn validate_source_pattern_binding_name(
    subject: &str,
    semantic_index: &SemanticIndex,
    binding: &Identifier,
) -> Result<()> {
    if binding.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "{subject} pattern binding {binding} conflicts with a reserved state parameter name"
        )));
    }
    if semantic_index.process_id(binding).is_ok() {
        return Err(Error::new(format!(
            "{subject} pattern binding {binding} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(binding) {
        return Err(Error::new(format!(
            "{subject} pattern binding {binding} conflicts with a declared type or value constructor"
        )));
    }
    Ok(())
}

pub(in crate::language::checker::source_functions) fn validate_source_pattern_binding_scope_conflicts(
    scope: &SourceFunctionScope<'_>,
    binding_label: &str,
    bindings: &[PatternPayloadParam],
) -> Result<()> {
    for binding in bindings {
        validate_source_binding_scope_conflict(scope, binding_label, &binding.name)?;
    }
    Ok(())
}

pub(in crate::language::checker::source_functions) fn validate_source_value_binding_name(
    scope: &SourceFunctionScope<'_>,
    binding_label: &str,
    bindings: &[SourceValueBinding<'_>],
    name: &Identifier,
) -> Result<()> {
    if bindings.iter().any(|binding| binding.name == name) {
        return Err(Error::new(format!(
            "{binding_label} {name} conflicts with an existing source value binding"
        )));
    }
    if scope.semantic_index.process_id(name).is_ok() {
        return Err(Error::new(format!(
            "{binding_label} {name} conflicts with a process declaration"
        )));
    }
    if scope
        .semantic_index
        .identifier_conflicts_with_declared_value(name)
    {
        return Err(Error::new(format!(
            "{binding_label} {name} conflicts with a declared type or value constructor"
        )));
    }
    validate_source_binding_scope_conflict(scope, binding_label, name)
}

pub(in crate::language::checker::source_functions) fn validate_source_binding_scope_conflict(
    scope: &SourceFunctionScope<'_>,
    binding_label: &str,
    name: &Identifier,
) -> Result<()> {
    if scope
        .process_refs
        .is_some_and(|process_refs| process_refs.contains_key(name))
    {
        return Err(Error::new(format!(
            "{binding_label} {name} conflicts with a process reference binding"
        )));
    }
    if source_function_name_conflicts(scope, name) {
        return Err(Error::new(format!(
            "{binding_label} {name} conflicts with a source function declaration"
        )));
    }
    Ok(())
}

fn source_function_name_conflicts(scope: &SourceFunctionScope<'_>, name: &Identifier) -> bool {
    scope
        .module
        .functions
        .iter()
        .any(|function| function.name == *name)
        || scope
            .process_functions
            .iter()
            .any(|function| function.name == *name)
}
