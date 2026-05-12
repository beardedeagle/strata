use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactEffect,
    ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef, ArtifactSendTarget,
    ArtifactStateValue, ArtifactTransition, ArtifactType, ArtifactTypeKind, ArtifactValueTemplate,
    ArtifactValueTemplateField, ArtifactValueTemplateMapEntry, MantleArtifact, MessageId,
    NextState, OutputId, ProcessId, ProcessRefId, StateId, StepResult, TypeId, source_hash_fnv1a64,
};

use super::Effect;
use super::checked::{
    CheckedAction, CheckedMessageCase, CheckedMessageId, CheckedNextState, CheckedOutputId,
    CheckedProcess, CheckedProcessId, CheckedProcessRefId, CheckedProgram, CheckedSendTarget,
    CheckedStateId, CheckedStateValue, CheckedStepResult, CheckedTransition, CheckedTypeId,
    CheckedTypeKind, CheckedTypeRef, CheckedValueTemplate,
};

const STRATA_SOURCE_LANGUAGE: &str = "strata";

struct ArtifactTypeMap {
    artifacts: Vec<ArtifactType>,
}

impl ArtifactTypeMap {
    fn new(checked: &CheckedProgram) -> mantle_artifact::Result<Self> {
        let artifacts = checked
            .types()
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                let id = CheckedTypeId::from_index(index).map_err(|err| {
                    mantle_artifact::Error::new(format!(
                        "checked type index {index} cannot lower: {err}"
                    ))
                })?;
                if ty.id() != id {
                    return Err(mantle_artifact::Error::new(format!(
                        "checked type table id {} does not match index {index}",
                        ty.id().as_u32()
                    )));
                }
                Ok(ArtifactType {
                    label: ty.label().to_string(),
                    kind: match ty.kind() {
                        CheckedTypeKind::Value => ArtifactTypeKind::Value,
                        CheckedTypeKind::ProcessRef { target } => ArtifactTypeKind::ProcessRef {
                            target: lower_process_id(target),
                        },
                    },
                })
            })
            .collect::<mantle_artifact::Result<Vec<_>>>()?;
        Ok(Self { artifacts })
    }

    fn artifact_id(&self, ty: &CheckedTypeRef) -> mantle_artifact::Result<TypeId> {
        self.artifacts.get(ty.id().index()).ok_or_else(|| {
            mantle_artifact::Error::new(format!(
                "checked type id {} is not in the checked type table",
                ty.id().as_u32()
            ))
        })?;
        TypeId::from_index(ty.id().index())
    }

    fn into_artifact_types(self) -> Vec<ArtifactType> {
        self.artifacts
    }
}

pub fn lower_to_artifact(
    checked: &CheckedProgram,
    source: &str,
) -> mantle_artifact::Result<MantleArtifact> {
    let type_map = ArtifactTypeMap::new(checked)?;
    let processes = checked
        .processes()
        .iter()
        .map(|process| lower_process(process, &type_map))
        .collect::<mantle_artifact::Result<Vec<_>>>()?;
    let artifact = MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: STRATA_SOURCE_LANGUAGE.to_string(),
        module: checked.module().name.to_string(),
        entry_process: lower_process_id(checked.entry_process()),
        entry_message: lower_message_id(checked.entry_message()),
        types: type_map.into_artifact_types(),
        outputs: checked.outputs().to_vec(),
        processes,
        source_hash_fnv1a64: source_hash_fnv1a64(source),
    };
    artifact.validate()?;
    Ok(artifact)
}

fn lower_process(
    process: &CheckedProcess,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactProcess> {
    Ok(ArtifactProcess {
        debug_name: process.debug_name().to_string(),
        state_type: types.artifact_id(process.state_type())?,
        state_values: process
            .state_values()
            .iter()
            .map(|value| lower_state_value(value, types))
            .collect::<mantle_artifact::Result<Vec<_>>>()?,
        message_type: types.artifact_id(process.message_type())?,
        message_variants: process
            .message_cases()
            .iter()
            .map(|message| lower_message_variant(message, types))
            .collect::<mantle_artifact::Result<Vec<_>>>()?,
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
        transitions: process
            .transitions()
            .iter()
            .map(|transition| lower_transition(transition, types))
            .collect::<mantle_artifact::Result<Vec<_>>>()?,
    })
}

fn lower_transition(
    transition: &CheckedTransition,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactTransition> {
    Ok(ArtifactTransition {
        current_state: transition.current_state().map(lower_state_id),
        message: lower_message_id(transition.message()),
        step_result: lower_step_result(transition.step_result()),
        next_state: lower_next_state(transition.next_state(), types)?,
        effects: transition
            .effects()
            .iter()
            .copied()
            .map(lower_effect)
            .collect(),
        actions: transition
            .actions()
            .iter()
            .map(|action| lower_action(action, types))
            .collect::<mantle_artifact::Result<Vec<_>>>()?,
    })
}

fn lower_effect(effect: Effect) -> ArtifactEffect {
    match effect {
        Effect::Emit => ArtifactEffect::Emit,
        Effect::Spawn => ArtifactEffect::Spawn,
        Effect::Send => ArtifactEffect::Send,
    }
}

fn lower_action(
    action: &CheckedAction,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactAction> {
    match action {
        CheckedAction::Emit { output } => Ok(ArtifactAction::Emit {
            output: lower_output_id(*output),
        }),
        CheckedAction::Spawn {
            target,
            process_ref,
        } => Ok(ArtifactAction::Spawn {
            target: lower_process_id(*target),
            process_ref: lower_process_ref_id(*process_ref),
        }),
        CheckedAction::Send {
            target,
            message,
            payload,
        } => Ok(ArtifactAction::Send {
            target: lower_send_target(target, types)?,
            message: lower_message_id(*message),
            payload: payload
                .as_ref()
                .map(|payload| lower_value_template(payload, types))
                .transpose()?,
        }),
    }
}

fn lower_next_state(
    next_state: CheckedNextState,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<NextState> {
    match next_state {
        CheckedNextState::Current => Ok(NextState::Current),
        CheckedNextState::Value(state) => Ok(NextState::Value(lower_state_id(state))),
        CheckedNextState::Template(template) => {
            Ok(NextState::Template(lower_value_template(&template, types)?))
        }
    }
}

fn lower_message_variant(
    message: &CheckedMessageCase,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactMessageVariant> {
    Ok(ArtifactMessageVariant {
        label: message.label().to_string(),
        payload_type: message
            .payload_type()
            .map(|ty| types.artifact_id(ty))
            .transpose()?,
    })
}

fn lower_state_value(
    value: &CheckedStateValue,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactStateValue> {
    let mut state = ArtifactStateValue::with_label(
        types.artifact_id(value.ty())?,
        value.value().clone(),
        value.label(),
    )?;
    if let Some(payload) = value.payload() {
        let payload_value = payload.value().cloned().ok_or_else(|| {
            mantle_artifact::Error::new("checked state payload cannot be a process reference")
        })?;
        state.payload = Some(mantle_artifact::ArtifactPayload::value(
            types.artifact_id(payload.ty())?,
            payload_value,
        )?);
    }
    Ok(state)
}

fn lower_value_template(
    template: &CheckedValueTemplate,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactValueTemplate> {
    match template {
        CheckedValueTemplate::Literal(value) => Ok(ArtifactValueTemplate::Literal {
            ty: types.artifact_id(value.ty())?,
            value: value.value().cloned().ok_or_else(|| {
                mantle_artifact::Error::new("literal process reference template must be explicit")
            })?,
        }),
        CheckedValueTemplate::ReceivedPayload { ty } => {
            Ok(ArtifactValueTemplate::ReceivedPayload {
                ty: types.artifact_id(ty)?,
            })
        }
        CheckedValueTemplate::CurrentStatePayload { ty } => {
            Ok(ArtifactValueTemplate::CurrentStatePayload {
                ty: types.artifact_id(ty)?,
            })
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            Ok(ArtifactValueTemplate::RecordField {
                ty: types.artifact_id(ty)?,
                record: Box::new(lower_value_template(record, types)?),
                field: field.to_string(),
            })
        }
        CheckedValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => Ok(ArtifactValueTemplate::ListElement {
            ty: types.artifact_id(ty)?,
            list: Box::new(lower_value_template(list, types)?),
            index: *index,
            len: *len,
        }),
        CheckedValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => Ok(ArtifactValueTemplate::MapValue {
            ty: types.artifact_id(ty)?,
            map: Box::new(lower_value_template(map, types)?),
            key: key.clone(),
            keys: keys.clone(),
            projection: *projection,
        }),
        CheckedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => Ok(ArtifactValueTemplate::MapRest {
            ty: types.artifact_id(ty)?,
            map: Box::new(lower_value_template(map, types)?),
            excluded_keys: excluded_keys.clone(),
        }),
        CheckedValueTemplate::ProcessRef {
            ty,
            target,
            process_ref,
        } => Ok(ArtifactValueTemplate::ProcessRef {
            ty: types.artifact_id(ty)?,
            target_process: lower_process_id(*target),
            process_ref: lower_process_ref_id(*process_ref),
        }),
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => Ok(ArtifactValueTemplate::EnumVariant {
            ty: types.artifact_id(ty)?,
            variant: variant.to_string(),
            payload: Box::new(lower_value_template(payload, types)?),
        }),
        CheckedValueTemplate::Record { ty, fields } => Ok(ArtifactValueTemplate::Record {
            ty: types.artifact_id(ty)?,
            fields: fields
                .iter()
                .map(|field| {
                    Ok(ArtifactValueTemplateField {
                        name: field.name().to_string(),
                        value: lower_value_template(field.value(), types)?,
                    })
                })
                .collect::<mantle_artifact::Result<Vec<_>>>()?,
        }),
        CheckedValueTemplate::List { ty, items } => Ok(ArtifactValueTemplate::List {
            ty: types.artifact_id(ty)?,
            items: items
                .iter()
                .map(|item| lower_value_template(item, types))
                .collect::<mantle_artifact::Result<Vec<_>>>()?,
        }),
        CheckedValueTemplate::Map { ty, entries } => Ok(ArtifactValueTemplate::Map {
            ty: types.artifact_id(ty)?,
            entries: entries
                .iter()
                .map(|entry| {
                    Ok(ArtifactValueTemplateMapEntry {
                        key: lower_value_template(entry.key(), types)?,
                        value: lower_value_template(entry.value(), types)?,
                    })
                })
                .collect::<mantle_artifact::Result<Vec<_>>>()?,
        }),
    }
}

fn lower_send_target(
    target: &CheckedSendTarget,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<ArtifactSendTarget> {
    match target {
        CheckedSendTarget::ProcessRef(process_ref) => Ok(ArtifactSendTarget::ProcessRef(
            lower_process_ref_id(*process_ref),
        )),
        CheckedSendTarget::ReceivedPayload { ty, target } => {
            Ok(ArtifactSendTarget::ReceivedPayload {
                ty: types.artifact_id(ty)?,
                target_process: lower_process_id(*target),
            })
        }
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
