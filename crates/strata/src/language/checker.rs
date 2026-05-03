mod outputs;
mod state_space;
mod static_validation;
mod symbols;

use std::collections::BTreeSet;

use mantle_artifact::{
    MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
};

use super::ast::{Determinism, Effect, Module, Process, ReturnExpr, Statement, ValueExpr};
use super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedProcess, CheckedProcessId,
    CheckedProcessParts, CheckedProgram, CheckedProgramParts, CheckedStateId, CheckedStepResult,
};
use super::diagnostic::{Error, Result};
use super::PROC_RESULT_TYPE;
use outputs::OutputPool;
use state_space::StateSpace;
use static_validation::{reject_unsupported_self_send, validate_action_references};
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

    validate_action_references(&checked_processes, &entry_process)?;

    let entry_message = CheckedMessageId::from_index(0)?;
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
    let (step_result, next_state, actions) = check_step(
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
        step_result,
        next_state,
        actions,
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
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
) -> Result<(CheckedStepResult, CheckedNextState, Vec<CheckedAction>)> {
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

    validate_count(
        &format!("process {} action_count", process.name),
        body.statements.len(),
        0,
        MAX_ACTIONS_PER_PROCESS,
    )?;
    let mut used_effects = BTreeSet::new();
    let mut actions = Vec::with_capacity(body.statements.len());
    for statement in &body.statements {
        match statement {
            Statement::Emit(text) => {
                used_effects.insert(Effect::Emit);
                actions.push(CheckedAction::Emit {
                    output: outputs.intern(text.as_str())?,
                });
            }
            Statement::Spawn(target) => {
                used_effects.insert(Effect::Spawn);
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
                used_effects.insert(Effect::Send);
                actions.push(CheckedAction::Send {
                    target: target_id,
                    message: message_id,
                });
            }
        }
    }
    validate_effects("step", &step.effects, used_effects)?;

    let (step_result, state_arg) = match &body.returns {
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

    reject_unsupported_self_send(process, process_id, &actions)?;

    Ok((step_result, next_state, actions))
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
