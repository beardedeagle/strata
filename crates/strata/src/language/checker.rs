mod outputs;
mod state_space;
mod static_validation;
mod symbols;

use std::collections::BTreeSet;

use mantle_artifact::{
    MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
};

use super::ast::{
    Determinism, Effect, FunctionBlock, FunctionBody, MessageMatch, Module, Process, ReturnExpr,
    Statement, ValueExpr,
};
use super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedProcess, CheckedProcessId,
    CheckedProcessParts, CheckedProgram, CheckedProgramParts, CheckedStateId, CheckedStepResult,
    CheckedTransition, CheckedTransitionParts,
};
use super::diagnostic::{Error, Result};
use super::PROC_RESULT_TYPE;
use outputs::OutputPool;
use state_space::StateSpace;
use static_validation::validate_action_references;
use symbols::SemanticIndex;

const STEP_STATE_PARAMETER_NAME: &str = "state";

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
    let transitions = check_step(
        module,
        process,
        process_id,
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
    let FunctionBody::Block(body) = body else {
        return Err(Error::new("init body must not use message matching"));
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
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
) -> Result<Vec<CheckedTransition>> {
    let step = &process.step;
    if step.params.len() != 2 {
        return Err(Error::new("step must declare state and msg parameters"));
    }
    let state_param = &step.params[0];
    let msg_param = &step.params[1];
    if state_param.name.as_str() != STEP_STATE_PARAMETER_NAME
        || !semantic_index.same_type(&state_param.ty, &process.state_type)
    {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    }
    if msg_param.name.as_str() != "msg"
        || !semantic_index.same_type(&msg_param.ty, &process.msg_type)
    {
        return Err(Error::new(format!(
            "step second parameter must be msg: {}",
            process.msg_type
        )));
    }

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

    let Some(body) = &step.body else {
        return Err(Error::new("step must have a body for buildable source"));
    };

    let transitions = match body {
        FunctionBody::Block(block) => {
            check_simple_step_block(module, process, semantic_index, state_space, outputs, block)?
        }
        FunctionBody::MatchMessage(message_match) => check_message_match_step(
            module,
            process,
            process_id,
            semantic_index,
            state_space,
            outputs,
            message_match,
        )?,
    };

    let action_count = total_action_count(&transitions)?;
    validate_count(
        &format!("process {} action_count", process.name),
        action_count,
        0,
        MAX_ACTIONS_PER_PROCESS,
    )?;
    let used_effects = transitions
        .iter()
        .flat_map(|transition| transition.actions())
        .fold(BTreeSet::new(), |mut effects, action| {
            effects.insert(action.effect());
            effects
        });
    validate_effects("step", &step.effects, used_effects)?;

    Ok(transitions)
}

fn check_simple_step_block(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    block: &FunctionBlock,
) -> Result<Vec<CheckedTransition>> {
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    if msg_enum.variants.len() != 1 {
        return Err(Error::new(format!(
            "process {} step with multiple messages must use match msg",
            process.name
        )));
    }
    let transition = check_step_transition(
        module,
        process,
        semantic_index,
        state_space,
        outputs,
        CheckedMessageId::from_index(0)?,
        block,
    )?;
    Ok(vec![transition])
}

fn check_message_match_step(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    message_match: &MessageMatch,
) -> Result<Vec<CheckedTransition>> {
    if message_match.scrutinee.as_str() != "msg" {
        return Err(Error::new(format!(
            "process {} step must match msg, got {}",
            process.name, message_match.scrutinee
        )));
    }

    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    let mut seen = vec![false; msg_enum.variants.len()];
    let mut transitions = Vec::with_capacity(message_match.arms.len());
    for arm in &message_match.arms {
        let message = semantic_index.message_id_for_match_arm(module, process_id, &arm.message)?;
        if std::mem::replace(&mut seen[message.index()], true) {
            return Err(Error::new(format!(
                "process {} step has duplicate match arm for message {}",
                process.name, arm.message
            )));
        }
        transitions.push(check_step_transition(
            module,
            process,
            semantic_index,
            state_space,
            outputs,
            message,
            &arm.body,
        )?);
    }
    for (index, covered) in seen.iter().enumerate() {
        if !covered {
            return Err(Error::new(format!(
                "process {} step match must cover message {}",
                process.name, msg_enum.variants[index]
            )));
        }
    }
    Ok(transitions)
}

fn check_step_transition(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
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
            Statement::Spawn(target) => {
                actions.push(CheckedAction::Spawn {
                    target: semantic_index.process_id(target)?,
                });
            }
            Statement::Send { target, message } => {
                let target_id = semantic_index.process_id(target)?;
                let message_id = semantic_index.message_id_for_process(
                    module,
                    process.name.as_str(),
                    target_id,
                    message,
                )?;
                actions.push(CheckedAction::Send {
                    target: target_id,
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
        CheckedNextState::Value(state_space.resolve_state_value(semantic_index, state_arg)?)
    };

    Ok(CheckedTransition::new(CheckedTransitionParts {
        message,
        step_result,
        next_state,
        actions,
    }))
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
