use super::discovery::collect_step_blocks;
use super::*;

pub(super) fn collect_process_refs(
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step_clauses: &[StepClause<'_>],
) -> Result<(
    Vec<CheckedProcessRef>,
    BTreeMap<Identifier, ProcessRefBinding>,
)> {
    let mut process_refs = Vec::new();
    let mut process_ref_index = BTreeMap::new();
    let context = ProcessRefCollectionContext {
        process,
        process_id,
        entry_process,
        semantic_index,
    };
    for clause in step_clauses {
        collect_process_refs_from_block(
            &context,
            clause.body,
            &clause.payload_bindings,
            &clause.state_payload_bindings,
            &mut process_refs,
            &mut process_ref_index,
        )?;
    }
    Ok((process_refs, process_ref_index))
}

pub(in crate::language::checker) fn collect_message_case_process_refs(
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<BTreeMap<Identifier, CheckedProcessId>> {
    let mut refs = BTreeMap::new();
    for step in &process.steps {
        for body in collect_step_blocks(step) {
            for statement in &body.statements {
                let Statement::LetProcessRef { name, ty, target } = statement else {
                    continue;
                };
                validate_process_ref_name(process, semantic_index, name)?;
                let annotated_target = process_ref_type_target(process, semantic_index, name, ty)?;
                let target_id = semantic_index.process_id(target)?;
                if annotated_target != target_id {
                    return Err(Error::new(format!(
                        "process {} process reference {} has type {ty} but spawns {}",
                        process.name, name, target
                    )));
                }
                if target_id == process_id {
                    return Err(Error::new(format!(
                        "process {} spawns itself, which is not supported",
                        process.name
                    )));
                }
                let existing = refs.insert(name.clone(), target_id);
                if existing.is_some_and(|existing| existing != target_id) {
                    return Err(Error::new(format!(
                        "process {} process reference {} is bound to multiple process definitions",
                        process.name, name
                    )));
                }
            }
        }
    }
    Ok(refs)
}

pub(super) fn collect_process_refs_from_block(
    context: &ProcessRefCollectionContext<'_>,
    block: &FunctionBlock,
    payload_bindings: &[StepPayloadBinding],
    state_payload_bindings: &[StepStatePayloadBinding],
    process_refs: &mut Vec<CheckedProcessRef>,
    process_ref_index: &mut BTreeMap<Identifier, ProcessRefBinding>,
) -> Result<()> {
    for statement in &block.statements {
        let Statement::LetProcessRef { name, ty, target } = statement else {
            continue;
        };
        if payload_bindings.iter().any(|binding| binding.name == *name) {
            return Err(Error::new(format!(
                "process {} process reference {} conflicts with payload binding",
                context.process.name, name
            )));
        }
        if state_payload_bindings
            .iter()
            .any(|binding| binding.name == *name)
        {
            return Err(Error::new(format!(
                "process {} process reference {} conflicts with state payload binding",
                context.process.name, name
            )));
        }
        validate_process_ref_name(context.process, context.semantic_index, name)?;
        let annotated_target =
            process_ref_type_target(context.process, context.semantic_index, name, ty)?;
        let target_id = context.semantic_index.process_id(target)?;
        if annotated_target != target_id {
            return Err(Error::new(format!(
                "process {} process reference {} has type {ty} but spawns {}",
                context.process.name, name, target
            )));
        }
        if target_id == context.entry_process {
            return Err(Error::new(format!(
                "process {} spawns entry process {}, which is already started",
                context.process.name, target
            )));
        }
        if target_id == context.process_id {
            return Err(Error::new(format!(
                "process {} spawns itself, which is not supported",
                context.process.name
            )));
        }
        if let Some(existing) = process_ref_index.get(name) {
            if existing.target != target_id {
                return Err(Error::new(format!(
                    "process {} process reference {} is bound to multiple process definitions",
                    context.process.name, name
                )));
            }
            continue;
        }
        let process_ref_id = CheckedProcessRefId::from_index(process_refs.len())?;
        process_refs.push(CheckedProcessRef::new(name.clone(), target_id));
        process_ref_index.insert(
            name.clone(),
            ProcessRefBinding {
                id: process_ref_id,
                target: target_id,
            },
        );
    }
    Ok(())
}

fn validate_process_ref_name(
    process: &Process,
    semantic_index: &SemanticIndex,
    process_ref: &Identifier,
) -> Result<()> {
    if process_ref.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} process reference {} conflicts with a step parameter name",
            process.name, process_ref
        )));
    }
    if semantic_index.process_id(process_ref).is_ok() {
        return Err(Error::new(format!(
            "process {} process reference {} conflicts with a process declaration",
            process.name, process_ref
        )));
    }
    Ok(())
}

fn process_ref_type_target(
    process: &Process,
    semantic_index: &SemanticIndex,
    process_ref: &Identifier,
    ty: &TypeRef,
) -> Result<CheckedProcessId> {
    let TypeRef::Applied {
        constructor,
        args,
        const_args,
    } = ty
    else {
        return Err(Error::new(format!(
            "process {} process reference {} must be typed as {PROCESS_REF_TYPE}<ProcessName>",
            process.name, process_ref
        )));
    };
    if constructor.as_str() != PROCESS_REF_TYPE || args.len() != 1 || !const_args.is_empty() {
        return Err(Error::new(format!(
            "process {} process reference {} must be typed as {PROCESS_REF_TYPE}<ProcessName>",
            process.name, process_ref
        )));
    }
    let TypeRef::Named(target) = &args[0] else {
        return Err(Error::new(format!(
            "process {} process reference {} has nested process reference target type {}",
            process.name, process_ref, args[0]
        )));
    };
    semantic_index.process_id(target).map_err(|_| {
        Error::new(format!(
            "process {} process reference {} targets undeclared process {}",
            process.name, process_ref, target
        ))
    })
}
