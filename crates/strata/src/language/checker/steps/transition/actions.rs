use super::send::{resolve_checked_send_target, resolve_send_message_case};
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn checked_actions_for_statements(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    loop_elements: &mut LoopElementAllocator,
    scope: ActionCheckScope,
    statements: &[Statement],
) -> Result<Vec<CheckedAction>> {
    let mut actions = Vec::with_capacity(statements.len());
    for statement in statements {
        match statement {
            Statement::Emit(text) => {
                actions.push(CheckedAction::Emit {
                    output: outputs.intern(text.as_str())?,
                });
            }
            Statement::LetProcessRef { name, target, .. } => {
                if scope.is_in_runtime_if_branch() {
                    return Err(Error::new(format!(
                        "process {} {} cannot bind process reference {} in this source slice",
                        context.process.name,
                        scope.runtime_if_branch_label(),
                        name
                    )));
                }
                if scope.in_loop_body {
                    return Err(Error::new(format!(
                        "process {} for loop body cannot bind process reference {} in this source slice",
                        context.process.name, name
                    )));
                }
                let binding = context.process_ref_index.get(name).ok_or_else(|| {
                    Error::new(format!(
                        "process {} process reference {} was not resolved",
                        context.process.name, name
                    ))
                })?;
                actions.push(CheckedAction::Spawn {
                    target: context.semantic_index.process_id(target)?,
                    process_ref: binding.id,
                });
            }
            Statement::Send {
                target,
                message,
                payload,
            } => {
                let send_target = resolve_checked_send_target(context, payload_bindings, target)?;
                let message_id = resolve_send_message_case(
                    context,
                    types,
                    send_target.target_process,
                    message,
                    payload.as_ref(),
                    source_bindings,
                    template_bindings,
                )?;
                actions.push(CheckedAction::Send {
                    target: send_target.target,
                    message: message_id.message,
                    payload: message_id.payload.map(Box::new),
                });
            }
            Statement::IfElse {
                condition,
                then_body,
                else_body,
            } => {
                if scope.is_in_runtime_if_branch() {
                    return Err(Error::new(format!(
                        "process {} nested statement-level if branches are not supported in this source slice",
                        context.process.name
                    )));
                }
                actions.push(checked_if_else_statement_action(
                    context,
                    outputs,
                    types,
                    function_scope,
                    source_bindings,
                    template_bindings,
                    payload_bindings,
                    loop_elements,
                    scope,
                    condition,
                    then_body,
                    else_body,
                )?);
            }
            Statement::ForEach {
                item,
                collection,
                body,
            } => {
                if scope.in_loop_body {
                    return Err(Error::new(format!(
                        "process {} nested for loops are not supported in this source slice",
                        context.process.name
                    )));
                }
                if !scope.allows_for_each() {
                    return Err(Error::new(format!(
                        "process {} final-position runtime if branch cannot contain for loop actions in this source slice",
                        context.process.name
                    )));
                }
                actions.push(checked_for_each_action(
                    context,
                    outputs,
                    types,
                    function_scope,
                    source_bindings,
                    template_bindings,
                    payload_bindings,
                    loop_elements,
                    item,
                    collection,
                    body,
                )?);
            }
        }
    }
    Ok(actions)
}

#[allow(clippy::too_many_arguments)]
fn checked_if_else_statement_action(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    loop_elements: &mut LoopElementAllocator,
    scope: ActionCheckScope,
    condition: &ValueExpr,
    then_body: &[Statement],
    else_body: &[Statement],
) -> Result<CheckedAction> {
    let condition = checked_runtime_bool_condition(
        context,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        condition,
    )?;
    let branch_scope = scope.for_statement_if_branch();
    let then_actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        payload_bindings,
        loop_elements,
        branch_scope,
        then_body,
    )?;
    let else_actions = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        source_bindings,
        template_bindings,
        payload_bindings,
        loop_elements,
        branch_scope,
        else_body,
    )?;
    if then_actions.is_empty() && else_actions.is_empty() {
        return Err(Error::new(format!(
            "process {} statement-level if branches cannot both be empty",
            context.process.name
        )));
    }
    Ok(CheckedAction::IfElse {
        condition,
        then_actions,
        else_actions,
    })
}

#[allow(clippy::too_many_arguments)]
fn checked_for_each_action(
    context: &mut StepCheckContext<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    template_bindings: &[ValueTemplateBinding<'_>],
    payload_bindings: &[StepPayloadBinding],
    loop_elements: &mut LoopElementAllocator,
    item: &ForEachItem,
    collection: &ValueExpr,
    body: &[Statement],
) -> Result<CheckedAction> {
    let ValueExpr::Identifier(collection_name) = collection else {
        return Err(Error::new(format!(
            "process {} for loop collection must be a runtime list binding",
            context.process.name
        )));
    };
    let Some(collection_binding) = source_bindings
        .iter()
        .find(|binding| binding.name == collection_name)
    else {
        return Err(Error::new(format!(
            "process {} for loop collection {} is not a source value binding",
            context.process.name, collection_name
        )));
    };
    let Some(CollectionType::List {
        element: element_type,
        capacity,
    }) = context
        .semantic_index
        .collection_type(collection_binding.ty)?
    else {
        return Err(Error::new(format!(
            "process {} for loop collection {} must have type List<T,N>",
            context.process.name, collection_name
        )));
    };
    if context
        .semantic_index
        .process_ref_target_type(element_type)?
        .is_some()
    {
        return Err(Error::new(format!(
            "process {} for loop element binding {} cannot have process reference type",
            context.process.name,
            for_each_item_name(item)
        )));
    }
    if !template_bindings
        .iter()
        .any(|binding| binding.name == collection_name)
    {
        return Err(Error::new(format!(
            "process {} for loop collection {} must be runtime-bound",
            context.process.name, collection_name
        )));
    }
    validate_source_function_value_expr(
        function_scope,
        collection_binding.ty,
        collection,
        source_bindings,
    )?;
    let collection = resolve_source_value_expr(
        function_scope,
        collection_binding.ty,
        collection,
        source_bindings,
        0,
    )?;
    let collection_template = checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        collection_binding.ty,
        &collection,
        template_bindings,
    )?;

    let element_id = loop_elements.next_id()?;
    let element_ty = types.intern(element_type)?;
    let loop_element = CheckedLoopElement::new(element_id, element_ty.clone());
    let loop_bindings = checked_loop_element_bindings(
        context,
        types,
        source_bindings,
        item,
        element_type,
        &element_ty,
    )?;
    let mut body_source_bindings = source_bindings.to_vec();
    for binding in &loop_bindings {
        body_source_bindings.push(SourceValueBinding {
            name: binding.name,
            ty: &binding.ty,
        });
    }
    let mut body_template_bindings = template_bindings.to_vec();
    for binding in &loop_bindings {
        body_template_bindings.push(ValueTemplateBinding {
            name: binding.name,
            ty: &binding.ty,
            checked_ty: &binding.checked_ty,
            root_checked_ty: &element_ty,
            source: ValueTemplateSource::LoopElement(element_id),
            path: &binding.path,
        });
    }
    let body = checked_actions_for_statements(
        context,
        outputs,
        types,
        function_scope,
        &body_source_bindings,
        &body_template_bindings,
        payload_bindings,
        loop_elements,
        ActionCheckScope::TOP_LEVEL.for_loop_body(),
        body,
    )?;

    Ok(CheckedAction::ForEach {
        element: loop_element,
        collection: collection_template,
        max_items: capacity,
        body,
    })
}

struct LoopElementBinding<'a> {
    name: &'a Identifier,
    ty: TypeRef,
    checked_ty: CheckedTypeRef,
    path: PayloadBindingPath,
}

fn checked_loop_element_bindings<'a>(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    item: &'a ForEachItem,
    element_type: &'a TypeRef,
    element_ty: &CheckedTypeRef,
) -> Result<Vec<LoopElementBinding<'a>>> {
    match item {
        ForEachItem::Binding(item) => {
            validate_loop_element_binding(context, source_bindings, item)?;
            Ok(vec![LoopElementBinding {
                name: item,
                ty: element_type.clone(),
                checked_ty: element_ty.clone(),
                path: PayloadBindingPath::whole(),
            }])
        }
        ForEachItem::RecordPattern { name, fields } => checked_record_loop_element_bindings(
            context,
            types,
            source_bindings,
            name,
            fields,
            element_type,
        ),
    }
}

fn for_each_item_name(item: &ForEachItem) -> &Identifier {
    match item {
        ForEachItem::Binding(name) | ForEachItem::RecordPattern { name, .. } => name,
    }
}

fn checked_record_loop_element_bindings<'a>(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    name: &Identifier,
    fields: &'a [RecordPatternField],
    element_type: &'a TypeRef,
) -> Result<Vec<LoopElementBinding<'a>>> {
    let record = context
        .semantic_index
        .record_decl(context.module, element_type)
        .map_err(|_| {
            Error::new(format!(
                "process {} for loop record pattern {name} cannot match loop element type {element_type}",
                context.process.name
            ))
        })?;
    if record.name != *name {
        return Err(Error::new(format!(
            "process {} for loop record pattern {name} cannot match record {}",
            context.process.name, record.name
        )));
    }
    if fields.is_empty() {
        return Err(Error::new(format!(
            "process {} for loop record pattern {name} must bind at least one field",
            context.process.name
        )));
    }

    let mut seen_fields = BTreeSet::new();
    let mut seen_bindings = BTreeSet::new();
    let mut bindings = Vec::with_capacity(fields.len());
    for field in fields {
        if !seen_fields.insert(field.field.as_str()) {
            return Err(Error::new(format!(
                "process {} for loop record pattern {name} binds field {} more than once",
                context.process.name, field.field
            )));
        }
        let Some(field_decl) = record
            .fields
            .iter()
            .find(|candidate| candidate.name == field.field)
        else {
            return Err(Error::new(format!(
                "process {} for loop record pattern {name} has no field {}",
                context.process.name, field.field
            )));
        };
        if !seen_bindings.insert(field.binding.as_str()) {
            return Err(Error::new(format!(
                "process {} loop element binding {} is declared more than once",
                context.process.name, field.binding
            )));
        }
        validate_loop_element_binding(context, source_bindings, &field.binding)?;
        if context
            .semantic_index
            .process_ref_target_type(&field_decl.ty)?
            .is_some()
        {
            return Err(Error::new(format!(
                "process {} loop element binding {} cannot have process reference type",
                context.process.name, field.binding
            )));
        }
        bindings.push(LoopElementBinding {
            name: &field.binding,
            ty: field_decl.ty.clone(),
            checked_ty: types.intern(&field_decl.ty)?,
            path: PayloadBindingPath::whole().then(PayloadProjectionSegment::record_field(
                field_decl.ty.clone(),
                field.field.clone(),
            )),
        });
    }
    Ok(bindings)
}

fn validate_loop_element_binding(
    context: &StepCheckContext<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    item: &Identifier,
) -> Result<()> {
    if item.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a reserved state parameter name",
            context.process.name, item
        )));
    }
    if source_bindings.iter().any(|binding| binding.name == item) {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with an existing source value binding",
            context.process.name, item
        )));
    }
    if context.process_ref_index.contains_key(item) {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a process reference binding",
            context.process.name, item
        )));
    }
    if context.semantic_index.process_id(item).is_ok() {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a process declaration",
            context.process.name, item
        )));
    }
    if context
        .semantic_index
        .identifier_conflicts_with_declared_value(item)
    {
        return Err(Error::new(format!(
            "process {} loop element binding {} conflicts with a declared type or value constructor",
            context.process.name, item
        )));
    }
    Ok(())
}
