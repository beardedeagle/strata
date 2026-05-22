use super::*;

#[derive(Clone, Copy)]
struct ForEachSendDiscovery<'a> {
    item: &'a ForEachItem,
    collection: &'a ValueExpr,
    body: &'a [Statement],
}

pub(super) fn discover_send_statements(
    builders: &mut [MessageCaseBuilder<'_>],
    process: &Process,
    pattern: &StepPattern,
    statements: &[Statement],
    bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    let mut changed = false;
    for statement in statements {
        changed |= discover_send_statement(
            builders,
            process,
            pattern,
            statement,
            bindings,
            state_payload_bindings,
            context,
        )?;
    }
    Ok(changed)
}

fn discover_send_statement(
    builders: &mut [MessageCaseBuilder<'_>],
    process: &Process,
    pattern: &StepPattern,
    statement: &Statement,
    bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    match statement {
        Statement::Send {
            target,
            message,
            payload,
        } => {
            let target_process_id = resolve_send_target_process_for_discovery(
                process,
                context.semantic_index,
                context.process_refs,
                pattern,
                target,
            )?;
            let target_variant = context.semantic_index.message_id_for_process(
                context.module,
                process.name.as_str(),
                target_process_id,
                message,
            )?;
            let builder = builders.get_mut(target_process_id.index()).ok_or_else(|| {
                Error::new(format!(
                    "process id {} is not declared",
                    target_process_id.as_u32()
                ))
            })?;
            add_discovered_send_payload_cases(
                builder,
                target_variant,
                payload.as_ref(),
                bindings,
                state_payload_bindings,
                context,
            )
        }
        Statement::IfElse {
            then_body,
            else_body,
            ..
        } => {
            let then_changed = discover_send_statements(
                builders,
                process,
                pattern,
                then_body,
                bindings,
                state_payload_bindings,
                context,
            )?;
            let else_changed = discover_send_statements(
                builders,
                process,
                pattern,
                else_body,
                bindings,
                state_payload_bindings,
                context,
            )?;
            Ok(then_changed || else_changed)
        }
        Statement::ForEach {
            item,
            collection,
            body,
        } => discover_for_each_send_statements(
            builders,
            process,
            pattern,
            ForEachSendDiscovery {
                item,
                collection,
                body,
            },
            bindings,
            state_payload_bindings,
            context,
        ),
        Statement::Emit(_) | Statement::LetProcessRef { .. } => Ok(false),
    }
}

fn discover_for_each_send_statements(
    builders: &mut [MessageCaseBuilder<'_>],
    process: &Process,
    pattern: &StepPattern,
    for_each: ForEachSendDiscovery<'_>,
    bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    let ValueExpr::Identifier(collection_name) = for_each.collection else {
        return Ok(false);
    };
    if let Some(collection_binding) = bindings
        .iter()
        .find(|binding| binding.name == *collection_name)
        && collection_binding.value.is_some()
    {
        return discover_for_each_send_statements_for_bindings(
            builders,
            process,
            pattern,
            for_each,
            bindings,
            state_payload_bindings,
            context,
        );
    }

    let mut changed = false;
    for binding_set in super::discovery_value_binding_sets(
        Some(for_each.collection),
        bindings,
        state_payload_bindings,
        context,
    )? {
        changed |= discover_for_each_send_statements_for_bindings(
            builders,
            process,
            pattern,
            for_each,
            &binding_set,
            state_payload_bindings,
            context,
        )?;
    }
    Ok(changed)
}

fn discover_for_each_send_statements_for_bindings(
    builders: &mut [MessageCaseBuilder<'_>],
    process: &Process,
    pattern: &StepPattern,
    for_each: ForEachSendDiscovery<'_>,
    bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    let ValueExpr::Identifier(collection_name) = for_each.collection else {
        return Ok(false);
    };
    let Some(collection_binding) = bindings
        .iter()
        .find(|binding| binding.name == *collection_name)
    else {
        return Ok(false);
    };
    let Some(collection_value) = collection_binding.value.as_ref() else {
        return Ok(false);
    };
    let Some(CollectionType::List {
        element: element_type,
        ..
    }) = context
        .semantic_index
        .collection_type(&collection_binding.ty)?
    else {
        return Err(Error::new(format!(
            "process {} for loop collection {} must have type List<T,N>",
            process.name, collection_name
        )));
    };
    let ArtifactValue::List(items) = collection_value else {
        return Err(Error::new(format!(
            "process {} for loop collection {} must resolve to a list value during discovery",
            process.name, collection_name
        )));
    };

    let mut changed = false;
    for item_value in items {
        let loop_bindings = discovery_loop_element_bindings(
            process,
            context,
            bindings,
            for_each.item,
            element_type,
            item_value,
        )?;
        let mut body_bindings =
            Vec::with_capacity(bindings.len().saturating_add(loop_bindings.len()));
        body_bindings.extend_from_slice(bindings);
        body_bindings.extend(loop_bindings);
        changed |= discover_send_statements(
            builders,
            process,
            pattern,
            for_each.body,
            &body_bindings,
            state_payload_bindings,
            context,
        )?;
    }
    Ok(changed)
}

fn discovery_loop_element_bindings(
    process: &Process,
    context: &SendPayloadDiscoveryContext<'_, '_, '_>,
    bindings: &[DiscoveryValueBinding],
    item: &ForEachItem,
    element_type: &TypeRef,
    item_value: &ArtifactValue,
) -> Result<Vec<DiscoveryValueBinding>> {
    match item {
        ForEachItem::Binding(name) => {
            validate_discovery_loop_binding(process, context, bindings, name, element_type)?;
            Ok(vec![DiscoveryValueBinding {
                name: name.clone(),
                ty: element_type.clone(),
                label: item_value.label(),
                value: Some(item_value.clone()),
            }])
        }
        ForEachItem::RecordPattern { name, fields } => {
            let record = context
                .semantic_index
                .record_decl(context.module, element_type)
                .map_err(|_| {
                    Error::new(format!(
                        "process {} for loop record pattern {name} cannot match loop element type {element_type}",
                        process.name
                    ))
                })?;
            let ArtifactValue::Record {
                constructor,
                fields: value_fields,
            } = item_value
            else {
                return Err(Error::new(format!(
                    "process {} for loop record pattern {name} cannot match non-record loop item",
                    process.name
                )));
            };
            if record.name != *name || constructor != name.as_str() {
                return Err(Error::new(format!(
                    "process {} for loop record pattern {name} cannot match record {}",
                    process.name, record.name
                )));
            }
            if fields.is_empty() {
                return Err(Error::new(format!(
                    "process {} for loop record pattern {name} must bind at least one field",
                    process.name
                )));
            }
            let mut seen_fields = BTreeSet::new();
            let mut seen_bindings = BTreeSet::new();
            let mut loop_bindings = Vec::with_capacity(fields.len());
            for field in fields {
                if !seen_fields.insert(field.field.as_str()) {
                    return Err(Error::new(format!(
                        "process {} for loop record pattern {name} binds field {} more than once",
                        process.name, field.field
                    )));
                }
                if !seen_bindings.insert(field.binding.as_str()) {
                    return Err(Error::new(format!(
                        "process {} loop element binding {} is declared more than once",
                        process.name, field.binding
                    )));
                }
                let Some(field_decl) = record
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field.field)
                else {
                    return Err(Error::new(format!(
                        "process {} for loop record pattern {name} has no field {}",
                        process.name, field.field
                    )));
                };
                validate_discovery_loop_binding(
                    process,
                    context,
                    bindings,
                    &field.binding,
                    &field_decl.ty,
                )?;
                let Some(value_field) = value_fields
                    .iter()
                    .find(|candidate| candidate.name == field.field.as_str())
                else {
                    return Err(Error::new(format!(
                        "process {} for loop record pattern {name} could not read field {}",
                        process.name, field.field
                    )));
                };
                loop_bindings.push(DiscoveryValueBinding {
                    name: field.binding.clone(),
                    ty: field_decl.ty.clone(),
                    label: value_field.value.label(),
                    value: Some(value_field.value.clone()),
                });
            }
            Ok(loop_bindings)
        }
    }
}

fn validate_discovery_loop_binding(
    process: &Process,
    context: &SendPayloadDiscoveryContext<'_, '_, '_>,
    bindings: &[DiscoveryValueBinding],
    name: &Identifier,
    ty: &TypeRef,
) -> Result<()> {
    if bindings.iter().any(|binding| binding.name == *name) {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with an existing source value binding",
            process.name, name
        )));
    }
    if context.process_refs.contains_key(name) {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a process reference binding",
            process.name, name
        )));
    }
    if context
        .semantic_index
        .process_ref_target_type(ty)?
        .is_some()
    {
        return Err(Error::new(format!(
            "process {} loop element binding {} cannot have process reference type",
            process.name, name
        )));
    }
    Ok(())
}
