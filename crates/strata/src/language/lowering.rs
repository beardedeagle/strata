use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactAuthority, ArtifactEffect,
    ArtifactLoopElement, ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef,
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactSendTarget,
    ArtifactStateValue, ArtifactTransition, ArtifactType, ArtifactTypeKind,
    ArtifactValueBooleanOperator, ArtifactValueEqualityOperator, ArtifactValueTemplate,
    ArtifactValueTemplateField, ArtifactValueTemplateMapEntry, AuthorityId, ComponentId,
    EffectOutcomeId, EnumVariantId, LoopElementId, MantleArtifact, MessageId, NextState, OutputId,
    PortId, ProcessId, ProcessRefId, ProtocolId, SpawnSiteId, StateId, StepResult,
    SupervisorChildId, SupervisorId, TypeId, source_hash_fnv1a64,
};

use super::Effect;
use super::checked::{
    CheckedAction, CheckedAuthorityId, CheckedComponentId, CheckedEffectOutcomeId,
    CheckedEnumVariantId, CheckedLoopElementId, CheckedMessageCase, CheckedMessageId,
    CheckedNextState, CheckedOutputId, CheckedPayloadValue, CheckedPortId, CheckedProcess,
    CheckedProcessId, CheckedProcessRefId, CheckedProgram, CheckedProtocolId,
    CheckedScalarArithmeticOperator, CheckedScalarOrderingOperator, CheckedSendTarget,
    CheckedSpawnSiteId, CheckedStateId, CheckedStateValue, CheckedStepResult, CheckedSupervisorId,
    CheckedTransition, CheckedTypeId, CheckedTypeKind, CheckedTypeRef, CheckedValueBooleanOperator,
    CheckedValueEqualityOperator, CheckedValueTemplate,
};
use super::source_program::SourceProvenanceHash;

mod boundaries;
mod capabilities;
mod record_fields;
mod supervision;
mod value_shapes;

use boundaries::{lower_components, lower_ports, lower_protocols};
use capabilities::lower_capability_descriptor;
use supervision::{lower_spawn_site, lower_supervisor_plans};
use value_shapes::lower_value_shape;

const STRATA_SOURCE_LANGUAGE: &str = "strata";

pub(super) struct ArtifactTypeMap {
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
                let (kind, shape) = match ty.kind() {
                    CheckedTypeKind::Value { shape } => {
                        (ArtifactTypeKind::Value, Some(lower_value_shape(shape)))
                    }
                    CheckedTypeKind::ProcessRef { target } => (
                        ArtifactTypeKind::ProcessRef {
                            target: lower_process_id(*target),
                        },
                        None,
                    ),
                };
                Ok(ArtifactType {
                    label: ty.label().to_string(),
                    kind,
                    shape,
                })
            })
            .collect::<mantle_artifact::Result<Vec<_>>>()?;
        Ok(Self { artifacts })
    }

    pub(super) fn artifact_id(&self, ty: &CheckedTypeRef) -> mantle_artifact::Result<TypeId> {
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
    lower_to_artifact_with_source_hash_fnv1a64(checked, source_hash_fnv1a64(source))
}

pub fn lower_to_artifact_with_source_hash(
    checked: &CheckedProgram,
    source_hash: SourceProvenanceHash,
) -> mantle_artifact::Result<MantleArtifact> {
    lower_to_artifact_with_source_hash_fnv1a64(checked, source_hash.into_fnv1a64())
}

fn lower_to_artifact_with_source_hash_fnv1a64(
    checked: &CheckedProgram,
    source_hash_fnv1a64: String,
) -> mantle_artifact::Result<MantleArtifact> {
    let type_map = ArtifactTypeMap::new(checked)?;
    let processes = checked
        .processes()
        .iter()
        .map(|process| lower_process(process, &type_map))
        .collect::<mantle_artifact::Result<Vec<_>>>()?;
    let protocols = lower_protocols(checked, &type_map)?;
    let ports = lower_ports(checked)?;
    let components = lower_components(checked)?;
    let artifact = MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: STRATA_SOURCE_LANGUAGE.to_string(),
        module: checked.module().name.to_string(),
        entry_process: lower_process_id(checked.entry_process()),
        entry_message: lower_message_id(checked.entry_message()),
        types: type_map.into_artifact_types(),
        outputs: checked.outputs().to_vec(),
        protocols,
        ports,
        components,
        processes,
        source_hash_fnv1a64,
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
        authorities: process
            .authorities()
            .iter()
            .map(|authority| ArtifactAuthority {
                debug_name: authority.debug_name().to_string(),
                descriptor: lower_capability_descriptor(authority.descriptor()),
            })
            .collect(),
        spawn_sites: process.spawn_sites().iter().map(lower_spawn_site).collect(),
        supervisor_plans: lower_supervisor_plans(process),
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
        payload_guard: transition
            .payload_guard()
            .map(|payload| lower_payload_guard(payload, types))
            .transpose()?,
        step_result: lower_step_result(transition.step_result()),
        next_state: lower_next_state(transition.next_state_ref(), types)?,
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

fn lower_payload_guard(
    payload: &CheckedPayloadValue,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<mantle_artifact::ArtifactPayload> {
    let value = payload.value().cloned().ok_or_else(|| {
        mantle_artifact::Error::new(
            "checked transition payload guard cannot be a process reference payload",
        )
    })?;
    mantle_artifact::ArtifactPayload::value(types.artifact_id(payload.ty())?, value)
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
            spawn_site,
        } => Ok(ArtifactAction::Spawn {
            target: lower_process_id(*target),
            process_ref: lower_process_ref_id(*process_ref),
            spawn_site: lower_spawn_site_id(*spawn_site),
        }),
        CheckedAction::SpawnOutcome {
            outcome,
            outcome_ty,
            target,
            spawn_site,
        } => Ok(ArtifactAction::SpawnOutcome {
            outcome: lower_effect_outcome_id(*outcome),
            outcome_ty: types.artifact_id(outcome_ty)?,
            target: lower_process_id(*target),
            spawn_site: lower_spawn_site_id(*spawn_site),
        }),
        CheckedAction::Send {
            target,
            port,
            message,
            payload,
        } => Ok(ArtifactAction::Send {
            target: lower_send_target(target, types)?,
            port: port.map(lower_port_id),
            message: lower_message_id(*message),
            payload: payload
                .as_ref()
                .map(|payload| lower_value_template(payload, types))
                .transpose()?,
        }),
        CheckedAction::SendOutcome {
            outcome,
            outcome_ty,
            target,
            port,
            message,
            payload,
        } => Ok(ArtifactAction::SendOutcome {
            outcome: lower_effect_outcome_id(*outcome),
            outcome_ty: types.artifact_id(outcome_ty)?,
            target: lower_send_target(target, types)?,
            port: port.map(lower_port_id),
            message: lower_message_id(*message),
            payload: payload
                .as_ref()
                .map(|payload| lower_value_template(payload, types))
                .transpose()?,
        }),
        CheckedAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => Ok(ArtifactAction::IfElse {
            condition: lower_value_template(condition, types)?,
            then_actions: then_actions
                .iter()
                .map(|action| lower_action(action, types))
                .collect::<mantle_artifact::Result<Vec<_>>>()?,
            else_actions: else_actions
                .iter()
                .map(|action| lower_action(action, types))
                .collect::<mantle_artifact::Result<Vec<_>>>()?,
        }),
        CheckedAction::ForEach {
            element,
            collection,
            max_items,
            body,
        } => Ok(ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: lower_loop_element_id(element.id()),
                ty: types.artifact_id(element.ty())?,
            },
            collection: lower_value_template(collection, types)?,
            max_items: *max_items,
            body: body
                .iter()
                .map(|action| lower_action(action, types))
                .collect::<mantle_artifact::Result<Vec<_>>>()?,
        }),
    }
}

fn lower_next_state(
    next_state: &CheckedNextState,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<NextState> {
    match next_state {
        CheckedNextState::Current => Ok(NextState::Current),
        CheckedNextState::Value(state) => Ok(NextState::Value(lower_state_id(*state))),
        CheckedNextState::Template(template) => {
            Ok(NextState::Template(lower_value_template(template, types)?))
        }
        CheckedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => Ok(NextState::IfElse {
            condition: lower_value_template(condition, types)?,
            then_state: Box::new(lower_next_state(then_state, types)?),
            else_state: Box::new(lower_next_state(else_state, types)?),
        }),
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
        CheckedValueTemplate::EffectOutcome { ty, outcome } => {
            Ok(ArtifactValueTemplate::EffectOutcome {
                ty: types.artifact_id(ty)?,
                outcome: lower_effect_outcome_id(*outcome),
            })
        }
        CheckedValueTemplate::EnumPayload { ty, value, variant } => {
            Ok(ArtifactValueTemplate::EnumPayload {
                ty: types.artifact_id(ty)?,
                value: Box::new(lower_value_template(value, types)?),
                variant: lower_enum_variant_id(*variant),
            })
        }
        CheckedValueTemplate::RecordField { ty, record, field } => {
            Ok(ArtifactValueTemplate::RecordField {
                ty: types.artifact_id(ty)?,
                record: Box::new(lower_value_template(record, types)?),
                field: types.record_field_id(record.result_type(), field)?,
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
        CheckedValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => Ok(ArtifactValueTemplate::ListPrefixElement {
            ty: types.artifact_id(ty)?,
            list: Box::new(lower_value_template(list, types)?),
            index: *index,
            prefix_len: *prefix_len,
        }),
        CheckedValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => Ok(ArtifactValueTemplate::ListRest {
            ty: types.artifact_id(ty)?,
            list: Box::new(lower_value_template(list, types)?),
            prefix_len: *prefix_len,
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
            keys: keys.to_vec(),
            projection: *projection,
        }),
        CheckedValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => Ok(ArtifactValueTemplate::MapRest {
            ty: types.artifact_id(ty)?,
            map: Box::new(lower_value_template(map, types)?),
            excluded_keys: excluded_keys.to_vec(),
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
        CheckedValueTemplate::LoopElement { ty, element } => {
            Ok(ArtifactValueTemplate::LoopElement {
                ty: types.artifact_id(ty)?,
                element: lower_loop_element_id(*element),
            })
        }
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => Ok(ArtifactValueTemplate::EnumVariant {
            ty: types.artifact_id(ty)?,
            variant: lower_enum_variant_id(*variant),
            payload: Box::new(lower_value_template(payload, types)?),
        }),
        CheckedValueTemplate::Record { ty, fields } => Ok(ArtifactValueTemplate::Record {
            ty: types.artifact_id(ty)?,
            fields: fields
                .iter()
                .map(|field| {
                    Ok(ArtifactValueTemplateField {
                        field: types.record_field_id(ty, field.name())?,
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
        CheckedValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => Ok(ArtifactValueTemplate::IfElse {
            ty: types.artifact_id(ty)?,
            condition: Box::new(lower_value_template(condition, types)?),
            then_value: Box::new(lower_value_template(then_value, types)?),
            else_value: Box::new(lower_value_template(else_value, types)?),
        }),
        CheckedValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => Ok(ArtifactValueTemplate::Equality {
            ty: types.artifact_id(ty)?,
            operand_ty: types.artifact_id(operand_ty)?,
            operator: lower_value_equality_operator(*operator),
            left: Box::new(lower_value_template(left, types)?),
            right: Box::new(lower_value_template(right, types)?),
        }),
        CheckedValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => Ok(ArtifactValueTemplate::ScalarArithmetic {
            ty: types.artifact_id(ty)?,
            operator: lower_scalar_arithmetic_operator(*operator),
            left: Box::new(lower_value_template(left, types)?),
            right: Box::new(lower_value_template(right, types)?),
        }),
        CheckedValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => Ok(ArtifactValueTemplate::ScalarOrdering {
            ty: types.artifact_id(ty)?,
            operand_ty: types.artifact_id(operand_ty)?,
            operator: lower_scalar_ordering_operator(*operator),
            left: Box::new(lower_value_template(left, types)?),
            right: Box::new(lower_value_template(right, types)?),
        }),
        CheckedValueTemplate::BooleanNot { ty, operand } => Ok(ArtifactValueTemplate::BooleanNot {
            ty: types.artifact_id(ty)?,
            operand: Box::new(lower_value_template(operand, types)?),
        }),
        CheckedValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => Ok(ArtifactValueTemplate::BooleanBinary {
            ty: types.artifact_id(ty)?,
            operator: lower_value_boolean_operator(*operator),
            left: Box::new(lower_value_template(left, types)?),
            right: Box::new(lower_value_template(right, types)?),
        }),
    }
}

fn lower_value_equality_operator(
    operator: CheckedValueEqualityOperator,
) -> ArtifactValueEqualityOperator {
    match operator {
        CheckedValueEqualityOperator::Equal => ArtifactValueEqualityOperator::Equal,
        CheckedValueEqualityOperator::NotEqual => ArtifactValueEqualityOperator::NotEqual,
    }
}

fn lower_value_boolean_operator(
    operator: CheckedValueBooleanOperator,
) -> ArtifactValueBooleanOperator {
    match operator {
        CheckedValueBooleanOperator::And => ArtifactValueBooleanOperator::And,
        CheckedValueBooleanOperator::Or => ArtifactValueBooleanOperator::Or,
    }
}

fn lower_scalar_arithmetic_operator(
    operator: CheckedScalarArithmeticOperator,
) -> ArtifactScalarArithmeticOperator {
    match operator {
        CheckedScalarArithmeticOperator::Add => ArtifactScalarArithmeticOperator::Add,
        CheckedScalarArithmeticOperator::Subtract => ArtifactScalarArithmeticOperator::Subtract,
        CheckedScalarArithmeticOperator::Multiply => ArtifactScalarArithmeticOperator::Multiply,
        CheckedScalarArithmeticOperator::Divide => ArtifactScalarArithmeticOperator::Divide,
        CheckedScalarArithmeticOperator::Modulo => ArtifactScalarArithmeticOperator::Modulo,
    }
}

fn lower_scalar_ordering_operator(
    operator: CheckedScalarOrderingOperator,
) -> ArtifactScalarOrderingOperator {
    match operator {
        CheckedScalarOrderingOperator::Less => ArtifactScalarOrderingOperator::Less,
        CheckedScalarOrderingOperator::LessEqual => ArtifactScalarOrderingOperator::LessEqual,
        CheckedScalarOrderingOperator::Greater => ArtifactScalarOrderingOperator::Greater,
        CheckedScalarOrderingOperator::GreaterEqual => ArtifactScalarOrderingOperator::GreaterEqual,
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
        CheckedSendTarget::SupervisorChild {
            supervisor,
            child,
            target,
        } => Ok(ArtifactSendTarget::SupervisorChild {
            supervisor: lower_supervisor_id(*supervisor),
            child: lower_supervisor_child_id(*child),
            target_process: lower_process_id(*target),
        }),
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

pub(super) fn lower_process_id(id: CheckedProcessId) -> ProcessId {
    ProcessId::new(id.as_u32())
}

pub(super) fn lower_protocol_id(id: CheckedProtocolId) -> ProtocolId {
    ProtocolId::new(id.as_u32())
}

pub(super) fn lower_port_id(id: CheckedPortId) -> PortId {
    PortId::new(id.as_u32())
}

pub(super) fn lower_component_id(id: CheckedComponentId) -> ComponentId {
    ComponentId::new(id.as_u32())
}

fn lower_type_id(id: CheckedTypeId) -> TypeId {
    TypeId::new(id.as_u32())
}

fn lower_authority_id(id: CheckedAuthorityId) -> AuthorityId {
    AuthorityId::new(id.as_u32())
}

fn lower_spawn_site_id(id: CheckedSpawnSiteId) -> SpawnSiteId {
    SpawnSiteId::new(id.as_u32())
}

pub(super) fn lower_supervisor_id(id: CheckedSupervisorId) -> SupervisorId {
    SupervisorId::new(id.as_u32())
}

pub(super) fn lower_supervisor_child_id(
    id: super::checked::CheckedSupervisorChildId,
) -> SupervisorChildId {
    SupervisorChildId::new(id.as_u32())
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

fn lower_enum_variant_id(id: CheckedEnumVariantId) -> EnumVariantId {
    EnumVariantId::new(id.as_u32())
}

fn lower_loop_element_id(id: CheckedLoopElementId) -> LoopElementId {
    LoopElementId::new(id.as_u32())
}

fn lower_effect_outcome_id(id: CheckedEffectOutcomeId) -> EffectOutcomeId {
    EffectOutcomeId::new(id.as_u32())
}
