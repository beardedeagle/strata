use super::*;

pub(in crate::language::checker::source_functions) fn validate_record_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[&Function],
) -> Result<()> {
    let Some(function) = functions.first() else {
        return Ok(());
    };
    if functions.len() != 1 {
        return Err(Error::new(format!(
            "{owner} function {} declares duplicate record pattern clauses",
            function.name
        )));
    }
    validate_pure_source_function_block(owner, function, source_function_block(function)?)?;

    let FunctionParam::Pattern(Pattern::Record { name, fields }) = &function.params[0] else {
        return Err(Error::new(format!(
            "{owner} function {} must declare a record pattern parameter",
            function.name
        )));
    };
    let record_type = TypeRef::Named(name.clone());
    let record = semantic_index.record_decl(module, &record_type)?;
    let subject = format!("{owner} function {}", function.name);
    let pattern_bindings = check_record_pattern_bindings(semantic_index, &subject, record, fields)?;
    let body_bindings = pattern_bindings
        .iter()
        .map(|binding| SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        })
        .collect::<Vec<_>>();

    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
        semantic_index,
    };
    validate_source_function_body_values(&scope, function, &body_bindings)
}

pub(in crate::language::checker::source_functions) fn check_record_pattern_bindings(
    semantic_index: &SemanticIndex,
    subject: &str,
    record: &Record,
    fields: &[RecordPatternField],
) -> Result<Vec<PatternPayloadParam>> {
    if fields.is_empty() {
        return Err(Error::new(format!(
            "{subject} record pattern {} must bind at least one field",
            record.name
        )));
    }

    let mut seen_fields = BTreeSet::new();
    let mut seen_bindings = BTreeSet::new();
    let mut bindings = Vec::with_capacity(fields.len());
    for field in fields {
        if !seen_fields.insert(field.field.as_str()) {
            return Err(Error::new(format!(
                "{subject} record pattern {} binds field {} more than once",
                record.name, field.field
            )));
        }
        let Some(field_decl) = record
            .fields
            .iter()
            .find(|candidate| candidate.name == field.field)
        else {
            return Err(Error::new(format!(
                "{subject} record pattern {} has no field {}",
                record.name, field.field
            )));
        };
        if !seen_bindings.insert(field.binding.as_str()) {
            return Err(Error::new(format!(
                "{subject} record pattern binding {} is declared more than once",
                field.binding
            )));
        }
        validate_record_pattern_binding_name(subject, semantic_index, &field.binding)?;
        bindings.push(PatternPayloadParam {
            name: field.binding.clone(),
            ty: field_decl.ty.clone(),
            path: PayloadBindingPath::RecordField {
                field: field.field.clone(),
            },
        });
    }
    Ok(bindings)
}

pub(in crate::language::checker::source_functions) fn record_pattern_type(
    function: &Function,
) -> Result<TypeRef> {
    let FunctionParam::Pattern(Pattern::Record { name, .. }) = &function.params[0] else {
        return Err(Error::new(format!(
            "function {} must declare a record pattern parameter",
            function.name
        )));
    };
    Ok(TypeRef::Named(name.clone()))
}

fn validate_record_pattern_binding_name(
    subject: &str,
    semantic_index: &SemanticIndex,
    binding: &Identifier,
) -> Result<()> {
    if binding.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "{subject} record pattern binding {binding} conflicts with a reserved state parameter name"
        )));
    }
    if semantic_index.process_id(binding).is_ok() {
        return Err(Error::new(format!(
            "{subject} record pattern binding {binding} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(binding) {
        return Err(Error::new(format!(
            "{subject} record pattern binding {binding} conflicts with a declared type or value constructor"
        )));
    }
    Ok(())
}
