use super::super::actions::{checked_loop_element_bindings, for_each_item_name};
use super::*;

pub(super) fn validate_step_return_match_arm_for_each_statement(
    context: &StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    function_scope: &SourceFunctionScope<'_>,
    source_bindings: &[SourceValueBinding<'_>],
    input: &StepTransitionInput<'_>,
    template_validation: ArmTemplateValidation<'_, '_, '_>,
    for_each: ArmForEach<'_>,
) -> Result<()> {
    let ValueExpr::Identifier(collection_name) = for_each.collection else {
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
        ..
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
            for_each_item_name(for_each.item)
        )));
    }
    if !template_validation
        .template_bindings
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
        for_each.collection,
        source_bindings,
    )?;
    let collection = resolve_source_value_expr(
        function_scope,
        collection_binding.ty,
        for_each.collection,
        source_bindings,
        0,
    )?;
    let collection =
        substitute_static_arm_bindings(collection, template_validation.arm_substitutions);
    checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        collection_binding.ty,
        &collection,
        template_validation.template_bindings,
    )?;

    let element_ty = types.intern(element_type)?;
    let loop_bindings = checked_loop_element_bindings(
        context,
        types,
        source_bindings,
        for_each.item,
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
    let element_id = CheckedLoopElementId::from_index(0)?;
    let mut body_template_bindings = template_validation.template_bindings.to_vec();
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
    for statement in for_each.body {
        super::validate_step_return_match_arm_action_statement(
            context,
            types,
            function_scope,
            &body_source_bindings,
            input,
            ArmStatementValidation {
                template: ArmTemplateValidation {
                    template_bindings: &body_template_bindings,
                    arm_substitutions: template_validation.arm_substitutions,
                },
                in_runtime_if_branch: false,
                in_loop_body: true,
            },
            statement,
        )?;
    }
    Ok(())
}
