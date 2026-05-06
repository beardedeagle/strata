mod outputs;
mod state_space;
mod static_validation;
mod symbols;

use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
};

use super::ast::{
    Determinism, Effect, Function, FunctionBlock, FunctionParam, Identifier, Module, Process,
    ReturnExpr, SignaturePattern, Statement, TypeRef, ValueExpr,
};
use super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedProcess, CheckedProcessId,
    CheckedProcessParts, CheckedProcessRef, CheckedProcessRefId, CheckedProgram,
    CheckedProgramParts, CheckedStateId, CheckedStepResult, CheckedTransition,
    CheckedTransitionParts,
};
use super::diagnostic::{Error, Result};
use super::{PROCESS_REF_TYPE, PROC_RESULT_TYPE};
use outputs::OutputPool;
use state_space::StateSpace;
use static_validation::validate_action_references;
use symbols::SemanticIndex;

const STEP_STATE_PARAMETER_NAME: &str = "state";

#[derive(Debug, Clone, Copy)]
struct ProcessRefBinding {
    id: CheckedProcessRefId,
    target: CheckedProcessId,
}

struct StepCheckContext<'a> {
    module: &'a Module,
    process: &'a Process,
    semantic_index: &'a SemanticIndex,
    process_ref_index: &'a BTreeMap<Identifier, ProcessRefBinding>,
}

struct StepClause<'a> {
    step: &'a Function,
    message: CheckedMessageId,
    body: &'a FunctionBlock,
}

pub fn check_module(module: Module) -> Result<CheckedProgram> {
    if module.records.is_empty() {
        return Err(Error::new("expected at least one record declaration"));
    }
    if module.enums.is_empty() {
        return Err(Error::new("expected at least one enum declaration"));
    }
    if module.processes.is_empty() {
        return Err(Error::new("expected at least one process declaration"));
    }
    validate_count(
        "process_count",
        module.processes.len(),
        1,
        MAX_PROCESS_COUNT,
    )?;

    let semantic_index = SemanticIndex::build(&module)?;
    let entry_process = semantic_index
        .process_id_by_name("Main")
        .map_err(|_| Error::new("entry process Main is not declared"))?;
    let mut outputs = OutputPool::new();
    let mut checked_processes = Vec::with_capacity(module.processes.len());
    for (index, process) in module.processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(index)?;
        checked_processes.push(check_process(
            &module,
            process,
            process_id,
            entry_process,
            &semantic_index,
            &mut outputs,
        )?);
    }

    let entry_message = CheckedMessageId::from_index(0)?;
    validate_action_references(&checked_processes, &entry_process, &entry_message)?;

    let entry_process_definition = checked_processes
        .get(entry_process.index())
        .ok_or_else(|| Error::new("entry process id is not defined"))?;
    if entry_process_definition.message_variants().is_empty() {
        return Err(Error::new(format!(
            "entry process {} has no messages",
            entry_process_definition.debug_name()
        )));
    }

    Ok(CheckedProgram::new(CheckedProgramParts {
        module,
        entry_process,
        entry_message,
        outputs: outputs.into_values(),
        processes: checked_processes,
    }))
}

fn check_process(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
    outputs: &mut OutputPool,
) -> Result<CheckedProcess> {
    validate_count(
        &format!("process {} mailbox_bound", process.name),
        process.mailbox_bound,
        1,
        MAX_MAILBOX_BOUND,
    )?;

    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    if msg_enum.variants.is_empty() {
        return Err(Error::new(format!(
            "enum {} must declare at least one variant",
            msg_enum.name
        )));
    }
    validate_count(
        &format!("process {} message_count", process.name),
        msg_enum.variants.len(),
        1,
        MAX_MESSAGE_VARIANTS_PER_PROCESS,
    )?;

    let mut state_space = StateSpace::new(module, semantic_index, process)?;
    let init_state = check_init(semantic_index, process, &mut state_space)?;
    let (process_refs, transitions) = check_step(
        module,
        process,
        process_id,
        entry_process,
        semantic_index,
        &mut state_space,
        outputs,
    )?;
    let state_values = state_space.into_values()?;

    Ok(CheckedProcess::new(CheckedProcessParts {
        debug_name: process.name.clone(),
        state_type: process.state_type.clone(),
        state_values,
        message_type: process.msg_type.clone(),
        message_variants: msg_enum.variants.clone(),
        process_refs,
        mailbox_bound: process.mailbox_bound,
        init_state,
        transitions,
    }))
}

fn check_init(
    semantic_index: &SemanticIndex,
    process: &Process,
    state_space: &mut StateSpace<'_>,
) -> Result<CheckedStateId> {
    let init = &process.init;
    if !init.params.is_empty() {
        return Err(Error::new("init must declare no parameters"));
    }
    if !semantic_index.same_type(&init.return_type, &process.state_type) {
        return Err(Error::new(format!(
            "init returns {}, expected {}",
            init.return_type, process.state_type
        )));
    }
    if !init.may.is_empty() {
        return Err(Error::new("init may-behaviors must be empty"));
    }
    if init.determinism != Determinism::Det {
        return Err(Error::new("init must be deterministic"));
    }

    let Some(body) = &init.body else {
        return Err(Error::new("init must have a body for buildable source"));
    };
    if !body.statements.is_empty() {
        return Err(Error::new(
            "init body must not perform statements in this slice",
        ));
    }
    validate_effects("init", &init.effects, BTreeSet::new())?;

    let ReturnExpr::Value(value) = &body.returns else {
        return Err(Error::new(format!(
            "init body must return a value of {}",
            process.state_type
        )));
    };
    state_space.resolve_state_value(semantic_index, value)
}

fn check_step(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
) -> Result<(Vec<CheckedProcessRef>, Vec<CheckedTransition>)> {
    let step_clauses = check_step_clauses(module, process, process_id, semantic_index)?;
    let (process_refs, process_ref_index) = collect_process_refs(
        process,
        process_id,
        entry_process,
        semantic_index,
        &step_clauses,
    )?;
    let step_context = StepCheckContext {
        module,
        process,
        semantic_index,
        process_ref_index: &process_ref_index,
    };

    let mut transitions = Vec::with_capacity(step_clauses.len());
    for clause in step_clauses {
        let transition = check_step_transition(
            &step_context,
            state_space,
            outputs,
            clause.message,
            clause.body,
        )?;
        let used_effects =
            transition
                .actions()
                .iter()
                .fold(BTreeSet::new(), |mut effects, action| {
                    effects.insert(action.effect());
                    effects
                });
        validate_effects("step", &clause.step.effects, used_effects)?;
        transitions.push(transition);
    }

    let action_count = total_action_count(&transitions)?;
    validate_count(
        &format!("process {} action_count", process.name),
        action_count,
        0,
        MAX_ACTIONS_PER_PROCESS,
    )?;

    Ok((process_refs, transitions))
}

fn check_step_clauses<'a>(
    module: &Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<Vec<StepClause<'a>>> {
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    let mut seen = vec![false; msg_enum.variants.len()];
    let mut clauses = Vec::with_capacity(process.steps.len());

    for step in &process.steps {
        let message = check_step_signature(module, process, process_id, semantic_index, step)?;
        if std::mem::replace(&mut seen[message.index()], true) {
            return Err(Error::new(format!(
                "process {} declares duplicate step pattern for message {}",
                process.name,
                msg_enum.variants[message.index()]
            )));
        }
        let Some(body) = &step.body else {
            return Err(Error::new("step must have a body for buildable source"));
        };
        clauses.push(StepClause {
            step,
            message,
            body,
        });
    }

    for (index, covered) in seen.iter().enumerate() {
        if !covered {
            return Err(Error::new(format!(
                "process {} must declare step pattern for message {}",
                process.name, msg_enum.variants[index]
            )));
        }
    }

    Ok(clauses)
}

fn check_step_signature(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &Function,
) -> Result<CheckedMessageId> {
    if step.params.len() != 2 {
        return Err(Error::new(
            "step must declare state parameter and message pattern",
        ));
    }
    let FunctionParam::Binding(state_param) = &step.params[0] else {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    };
    if state_param.name.as_str() != STEP_STATE_PARAMETER_NAME
        || !semantic_index.same_type(&state_param.ty, &process.state_type)
    {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    }
    let FunctionParam::Pattern(SignaturePattern::Variant(message)) = &step.params[1] else {
        return Err(Error::new(
            "step second parameter must be a message variant pattern",
        ));
    };

    if !semantic_index.is_proc_result_of(&step.return_type, &process.state_type) {
        return Err(Error::new(format!(
            "step returns {}, expected {}",
            step.return_type,
            format_args!("{PROC_RESULT_TYPE}<{}>", process.state_type)
        )));
    }
    if !step.may.is_empty() {
        return Err(Error::new("step may-behaviors must be empty"));
    }
    if step.determinism != Determinism::Det {
        return Err(Error::new("step must be deterministic"));
    }

    semantic_index.message_id_for_step_pattern(module, process_id, message)
}

fn collect_process_refs(
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
    for clause in step_clauses {
        collect_process_refs_from_block(
            process,
            process_id,
            entry_process,
            semantic_index,
            clause.body,
            &mut process_refs,
            &mut process_ref_index,
        )?;
    }
    Ok((process_refs, process_ref_index))
}

fn collect_process_refs_from_block(
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
    block: &FunctionBlock,
    process_refs: &mut Vec<CheckedProcessRef>,
    process_ref_index: &mut BTreeMap<Identifier, ProcessRefBinding>,
) -> Result<()> {
    for statement in &block.statements {
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
        if target_id == entry_process {
            return Err(Error::new(format!(
                "process {} spawns entry process {}, which is already started",
                process.name, target
            )));
        }
        if target_id == process_id {
            return Err(Error::new(format!(
                "process {} spawns itself, which is not supported",
                process.name
            )));
        }
        if let Some(existing) = process_ref_index.get(name) {
            if existing.target != target_id {
                return Err(Error::new(format!(
                    "process {} process reference {} is bound to multiple process definitions",
                    process.name, name
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

fn check_step_transition(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    message: CheckedMessageId,
    block: &FunctionBlock,
) -> Result<CheckedTransition> {
    let mut actions = Vec::with_capacity(block.statements.len());
    for statement in &block.statements {
        match statement {
            Statement::Emit(text) => {
                actions.push(CheckedAction::Emit {
                    output: outputs.intern(text.as_str())?,
                });
            }
            Statement::LetProcessRef { name, target, .. } => {
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
            Statement::Send { target, message } => {
                let binding = context.process_ref_index.get(target).ok_or_else(|| {
                    Error::new(format!(
                        "process {} sends to undeclared process reference {}",
                        context.process.name, target
                    ))
                })?;
                let message_id = context.semantic_index.message_id_for_process(
                    context.module,
                    context.process.name.as_str(),
                    binding.target,
                    message,
                )?;
                actions.push(CheckedAction::Send {
                    target: binding.id,
                    message: message_id,
                });
            }
        }
    }

    let (step_result, state_arg) = match &block.returns {
        ReturnExpr::Call { name, arg } if name.as_str() == "Stop" => (CheckedStepResult::Stop, arg),
        ReturnExpr::Call { name, arg } if name.as_str() == "Continue" => {
            (CheckedStepResult::Continue, arg)
        }
        _ => {
            return Err(Error::new(
                "step body must return Stop(<state value>) or Continue(<state value>)",
            ))
        }
    };
    let next_state = if matches!(state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        CheckedNextState::Current
    } else {
        CheckedNextState::Value(state_space.resolve_state_value(context.semantic_index, state_arg)?)
    };

    Ok(CheckedTransition::new(CheckedTransitionParts {
        message,
        step_result,
        next_state,
        actions,
    }))
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
    let TypeRef::Applied { constructor, args } = ty else {
        return Err(Error::new(format!(
            "process {} process reference {} must be typed as {PROCESS_REF_TYPE}<ProcessName>",
            process.name, process_ref
        )));
    };
    if constructor.as_str() != PROCESS_REF_TYPE || args.len() != 1 {
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

impl CheckedAction {
    fn effect(&self) -> Effect {
        match self {
            Self::Emit { .. } => Effect::Emit,
            Self::Spawn { .. } => Effect::Spawn,
            Self::Send { .. } => Effect::Send,
        }
    }
}

fn total_action_count(transitions: &[CheckedTransition]) -> Result<usize> {
    transitions.iter().try_fold(0usize, |total, transition| {
        total
            .checked_add(transition.actions().len())
            .ok_or_else(|| Error::new("process action_count overflowed"))
    })
}

fn validate_count(field: &str, value: usize, min: usize, max: usize) -> Result<()> {
    if value < min {
        if min == 1 {
            return Err(Error::new(format!("{field} must be greater than zero")));
        }
        return Err(Error::new(format!("{field} must be at least {min}")));
    }
    if value > max {
        return Err(Error::new(format!("{field} must be no greater than {max}")));
    }
    Ok(())
}

fn validate_effects(
    function_name: &str,
    declared_effects: &[Effect],
    used_effects: BTreeSet<Effect>,
) -> Result<()> {
    let mut declared = BTreeSet::new();
    for &effect in declared_effects {
        if !declared.insert(effect) {
            return Err(Error::new(format!(
                "{function_name} declares duplicate effect {effect}"
            )));
        }
    }

    for used in &used_effects {
        if !declared.contains(used) {
            return Err(Error::new(format!(
                "{function_name} uses effect {used} but does not declare it"
            )));
        }
    }
    for declared_effect in &declared {
        if !used_effects.contains(declared_effect) {
            return Err(Error::new(format!(
                "{function_name} declares effect {declared_effect} but does not use it"
            )));
        }
    }
    Ok(())
}
