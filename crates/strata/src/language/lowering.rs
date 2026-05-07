use mantle_artifact::{
    source_hash_fnv1a64, ArtifactAction, ArtifactEffect, ArtifactMessageVariant, ArtifactProcess,
    ArtifactProcessRef, ArtifactSendTarget, ArtifactStateValue, ArtifactTransition,
    ArtifactValueTemplate, ArtifactValueTemplateField, MantleArtifact, MessageId, NextState,
    OutputId, ProcessId, ProcessRefId, StateId, StepResult, ARTIFACT_FORMAT,
    ARTIFACT_SCHEMA_VERSION, STRATA_SOURCE_LANGUAGE,
};

use super::checked::{
    CheckedAction, CheckedMessageCase, CheckedMessageId, CheckedNextState, CheckedOutputId,
    CheckedProcess, CheckedProcessId, CheckedProcessRefId, CheckedProgram, CheckedSendTarget,
    CheckedStateId, CheckedStateValue, CheckedStepResult, CheckedTransition, CheckedValueTemplate,
};
use super::Effect;

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
        state_values: process
            .state_values()
            .iter()
            .map(lower_state_value)
            .collect(),
        message_type: process.message_type().to_string(),
        message_variants: process
            .message_cases()
            .iter()
            .map(lower_message_variant)
            .collect(),
        process_refs: process
            .process_refs()
            .iter()
            .map(|process_ref| ArtifactProcessRef {
                debug_name: process_ref.debug_name().to_string(),
                target: lower_process_id(process_ref.target()),
            })
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
        effects: transition
            .effects()
            .iter()
            .copied()
            .map(lower_effect)
            .collect(),
        actions: transition.actions().iter().map(lower_action).collect(),
    }
}

fn lower_effect(effect: Effect) -> ArtifactEffect {
    match effect {
        Effect::Emit => ArtifactEffect::Emit,
        Effect::Spawn => ArtifactEffect::Spawn,
        Effect::Send => ArtifactEffect::Send,
    }
}

fn lower_action(action: &CheckedAction) -> ArtifactAction {
    match action {
        CheckedAction::Emit { output } => ArtifactAction::Emit {
            output: lower_output_id(*output),
        },
        CheckedAction::Spawn {
            target,
            process_ref,
        } => ArtifactAction::Spawn {
            target: lower_process_id(*target),
            process_ref: lower_process_ref_id(*process_ref),
        },
        CheckedAction::Send {
            target,
            message,
            payload,
        } => ArtifactAction::Send {
            target: lower_send_target(target),
            message: lower_message_id(*message),
            payload: payload.as_ref().map(lower_value_template),
        },
    }
}

fn lower_next_state(next_state: CheckedNextState) -> NextState {
    match next_state {
        CheckedNextState::Current => NextState::Current,
        CheckedNextState::Value(state) => NextState::Value(lower_state_id(state)),
        CheckedNextState::Template(template) => {
            NextState::Template(lower_value_template(&template))
        }
    }
}

fn lower_message_variant(message: &CheckedMessageCase) -> ArtifactMessageVariant {
    ArtifactMessageVariant {
        label: message.label().to_string(),
        payload_type: message.payload_type().map(ToString::to_string),
    }
}

fn lower_state_value(value: &CheckedStateValue) -> ArtifactStateValue {
    ArtifactStateValue::with_label(
        value.ty().to_string(),
        value.value().to_string(),
        value.label().to_string(),
    )
}

fn lower_value_template(template: &CheckedValueTemplate) -> ArtifactValueTemplate {
    match template {
        CheckedValueTemplate::Literal(value) => ArtifactValueTemplate::Literal {
            ty: value.ty().to_string(),
            value: value.label().to_string(),
        },
        CheckedValueTemplate::ReceivedPayload { ty } => {
            ArtifactValueTemplate::ReceivedPayload { ty: ty.to_string() }
        }
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
        } => ArtifactValueTemplate::ProcessRef {
            ty: ty.to_string(),
            target_process: lower_process_id(*target),
            process_ref: lower_process_ref_id(*process_ref),
        },
        CheckedValueTemplate::Record { ty, fields } => ArtifactValueTemplate::Record {
            ty: ty.to_string(),
            fields: fields
                .iter()
                .map(|field| ArtifactValueTemplateField {
                    name: field.name().to_string(),
                    value: lower_value_template(field.value()),
                })
                .collect(),
        },
    }
}

fn lower_send_target(target: &CheckedSendTarget) -> ArtifactSendTarget {
    match target {
        CheckedSendTarget::ProcessRef(process_ref) => {
            ArtifactSendTarget::ProcessRef(lower_process_ref_id(*process_ref))
        }
        CheckedSendTarget::ReceivedPayload { ty, target } => ArtifactSendTarget::ReceivedPayload {
            ty: ty.to_string(),
            target_process: lower_process_id(*target),
        },
    }
}

fn lower_step_result(step_result: CheckedStepResult) -> StepResult {
    match step_result {
        CheckedStepResult::Continue => StepResult::Continue,
        CheckedStepResult::Stop => StepResult::Stop,
        CheckedStepResult::Panic => StepResult::Panic,
    }
}

fn lower_process_id(id: CheckedProcessId) -> ProcessId {
    ProcessId::new(id.as_u32())
}

fn lower_process_ref_id(id: CheckedProcessRefId) -> ProcessRefId {
    ProcessRefId::new(id.as_u32())
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
