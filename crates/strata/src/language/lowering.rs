use mantle_artifact::{
    source_hash_fnv1a64, ArtifactAction, ArtifactProcess, ArtifactTransition, MantleArtifact,
    MessageId, NextState, OutputId, ProcessId, StateId, StepResult, ARTIFACT_FORMAT,
    ARTIFACT_SCHEMA_VERSION, STRATA_SOURCE_LANGUAGE,
};

use super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedOutputId, CheckedProcess,
    CheckedProcessId, CheckedProgram, CheckedStateId, CheckedStepResult, CheckedTransition,
};

pub fn lower_to_artifact(
    checked: &CheckedProgram,
    source: &str,
) -> mantle_artifact::Result<MantleArtifact> {
    let artifact = MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: STRATA_SOURCE_LANGUAGE.to_string(),
        module: checked.module().name.to_string(),
        entry_process: lower_process_id(checked.entry_process()),
        entry_message: lower_message_id(checked.entry_message()),
        outputs: checked.outputs().to_vec(),
        processes: checked.processes().iter().map(lower_process).collect(),
        source_hash_fnv1a64: source_hash_fnv1a64(source),
    };
    artifact.validate()?;
    Ok(artifact)
}

fn lower_process(process: &CheckedProcess) -> ArtifactProcess {
    ArtifactProcess {
        debug_name: process.debug_name().to_string(),
        state_type: process.state_type().to_string(),
        state_values: process.state_values().to_vec(),
        message_type: process.message_type().to_string(),
        message_variants: process
            .message_variants()
            .iter()
            .map(ToString::to_string)
            .collect(),
        mailbox_bound: process.mailbox_bound(),
        init_state: lower_state_id(process.init_state()),
        transitions: process.transitions().iter().map(lower_transition).collect(),
    }
}

fn lower_transition(transition: &CheckedTransition) -> ArtifactTransition {
    ArtifactTransition {
        message: lower_message_id(transition.message()),
        step_result: lower_step_result(transition.step_result()),
        next_state: lower_next_state(transition.next_state()),
        actions: transition.actions().iter().map(lower_action).collect(),
    }
}

fn lower_action(action: &CheckedAction) -> ArtifactAction {
    match action {
        CheckedAction::Emit { output } => ArtifactAction::Emit {
            output: lower_output_id(*output),
        },
        CheckedAction::Spawn { target } => ArtifactAction::Spawn {
            target: lower_process_id(*target),
        },
        CheckedAction::Send { target, message } => ArtifactAction::Send {
            target: lower_process_id(*target),
            message: lower_message_id(*message),
        },
    }
}

fn lower_next_state(next_state: CheckedNextState) -> NextState {
    match next_state {
        CheckedNextState::Current => NextState::Current,
        CheckedNextState::Value(state) => NextState::Value(lower_state_id(state)),
    }
}

fn lower_step_result(step_result: CheckedStepResult) -> StepResult {
    match step_result {
        CheckedStepResult::Continue => StepResult::Continue,
        CheckedStepResult::Stop => StepResult::Stop,
    }
}

fn lower_process_id(id: CheckedProcessId) -> ProcessId {
    ProcessId::new(id.as_u32())
}

fn lower_state_id(id: CheckedStateId) -> StateId {
    StateId::new(id.as_u32())
}

fn lower_message_id(id: CheckedMessageId) -> MessageId {
    MessageId::new(id.as_u32())
}

fn lower_output_id(id: CheckedOutputId) -> OutputId {
    OutputId::new(id.as_u32())
}
