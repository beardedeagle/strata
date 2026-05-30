use mantle_artifact::{ArtifactTargetRequirements, RuntimeFeature};

use super::checked::{
    CheckedAction, CheckedCapabilityDescriptor, CheckedNextState, CheckedProgram,
    CheckedSendTarget, CheckedTypeKind, CheckedValueShape, CheckedValueTemplate,
};

pub(super) const STRATA_SOURCE_LANGUAGE: &str = "strata";

pub(super) fn target_requirements_for_checked_program(
    checked: &CheckedProgram,
) -> ArtifactTargetRequirements {
    let mut features = FeatureAccumulator::new();
    features.push(RuntimeFeature::BoundedMailbox);
    features.push(RuntimeFeature::JsonlTrace);
    features.push(RuntimeFeature::LocalExecution);

    if !checked.protocols().is_empty()
        || !checked.ports().is_empty()
        || !checked.components().is_empty()
    {
        features.push(RuntimeFeature::TypedBoundaryTables);
    }
    if !checked.compositions().is_empty() {
        features.push(RuntimeFeature::ComponentCompositionMetadata);
        features.push(RuntimeFeature::TypedBoundaryTables);
    }

    for ty in checked.types() {
        match ty.kind() {
            CheckedTypeKind::Value { shape } => collect_shape_requirements(&mut features, shape),
            CheckedTypeKind::ProcessRef { .. } => {
                features.push(RuntimeFeature::LocalSend);
            }
        }
    }

    for process in checked.processes() {
        if !process.supervisor_plans().is_empty() {
            features.push(RuntimeFeature::LocalSupervision);
        }
        for authority in process.authorities() {
            match authority.descriptor() {
                CheckedCapabilityDescriptor::Spawn { .. } => {
                    features.push(RuntimeFeature::LocalSpawn);
                }
                CheckedCapabilityDescriptor::ProtocolBoundary { .. }
                | CheckedCapabilityDescriptor::PortConnect { .. }
                | CheckedCapabilityDescriptor::ComponentExport { .. } => {
                    features.push(RuntimeFeature::TypedBoundaryTables);
                }
            }
        }
        for transition in process.transitions() {
            if transition.payload_guard().is_some() {
                features.push(RuntimeFeature::TypedValueTemplates);
            }
            collect_next_state_requirements(&mut features, transition.next_state_ref());
            for action in transition.actions() {
                collect_action_requirements(&mut features, action);
            }
        }
    }

    ArtifactTargetRequirements::new(STRATA_SOURCE_LANGUAGE, features.into_vec())
}

struct FeatureAccumulator {
    features: [RuntimeFeature; RuntimeFeature::COUNT],
    len: usize,
}

impl FeatureAccumulator {
    fn new() -> Self {
        Self {
            features: [RuntimeFeature::BoundedMailbox; RuntimeFeature::COUNT],
            len: 0,
        }
    }

    fn push(&mut self, feature: RuntimeFeature) {
        if self.features[..self.len].contains(&feature) {
            return;
        }
        debug_assert!(self.len < RuntimeFeature::COUNT);
        self.features[self.len] = feature;
        self.len += 1;
    }

    fn into_vec(self) -> Vec<RuntimeFeature> {
        self.features[..self.len].to_vec()
    }
}

fn collect_shape_requirements(features: &mut FeatureAccumulator, shape: &CheckedValueShape) {
    match shape {
        CheckedValueShape::Atom | CheckedValueShape::Record { .. } => {}
        CheckedValueShape::Scalar(_) => {
            features.push(RuntimeFeature::ScalarValueTemplates);
        }
        CheckedValueShape::Enum { variants } => {
            if variants
                .iter()
                .any(|variant| variant.payload_type.is_some())
            {
                features.push(RuntimeFeature::TypedValueTemplates);
            }
        }
        CheckedValueShape::List { .. } | CheckedValueShape::Map { .. } => {
            features.push(RuntimeFeature::TypedValueTemplates);
        }
    }
}

fn collect_next_state_requirements(
    features: &mut FeatureAccumulator,
    next_state: &CheckedNextState,
) {
    match next_state {
        CheckedNextState::Current | CheckedNextState::Value(_) => {}
        CheckedNextState::Template(template) => collect_template_requirements(features, template),
        CheckedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            features.push(RuntimeFeature::RuntimeBranching);
            collect_template_requirements(features, condition);
            collect_next_state_requirements(features, then_state);
            collect_next_state_requirements(features, else_state);
        }
    }
}

fn collect_action_requirements(features: &mut FeatureAccumulator, action: &CheckedAction) {
    match action {
        CheckedAction::Emit { .. } => {
            features.push(RuntimeFeature::EmitEffect);
        }
        CheckedAction::Spawn { .. } => {
            features.push(RuntimeFeature::LocalSpawn);
        }
        CheckedAction::SpawnOutcome { .. } => {
            features.push(RuntimeFeature::LocalSpawn);
            features.push(RuntimeFeature::TypedEffectOutcomes);
        }
        CheckedAction::Send {
            target,
            port,
            payload,
            ..
        } => {
            features.push(RuntimeFeature::LocalSend);
            collect_send_target_requirements(features, target);
            collect_port_requirements(features, *port);
            if let Some(payload) = payload {
                collect_template_requirements(features, payload);
            }
        }
        CheckedAction::SendOutcome {
            target,
            port,
            payload,
            ..
        } => {
            features.push(RuntimeFeature::LocalSend);
            features.push(RuntimeFeature::TypedEffectOutcomes);
            collect_send_target_requirements(features, target);
            collect_port_requirements(features, *port);
            if let Some(payload) = payload {
                collect_template_requirements(features, payload);
            }
        }
        CheckedAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            features.push(RuntimeFeature::RuntimeBranching);
            collect_template_requirements(features, condition);
            for action in then_actions {
                collect_action_requirements(features, action);
            }
            for action in else_actions {
                collect_action_requirements(features, action);
            }
        }
        CheckedAction::ForEach {
            collection, body, ..
        } => {
            features.push(RuntimeFeature::RuntimeForEach);
            features.push(RuntimeFeature::TypedValueTemplates);
            collect_template_requirements(features, collection);
            for action in body {
                collect_action_requirements(features, action);
            }
        }
    }
}

fn collect_send_target_requirements(features: &mut FeatureAccumulator, target: &CheckedSendTarget) {
    match target {
        CheckedSendTarget::ProcessRef(_) => {}
        CheckedSendTarget::SupervisorChild { .. } => {
            features.push(RuntimeFeature::LocalSupervision);
        }
        CheckedSendTarget::ReceivedPayload { .. } => {
            features.push(RuntimeFeature::TypedValueTemplates);
        }
    }
}

fn collect_port_requirements(
    features: &mut FeatureAccumulator,
    port: Option<super::checked::CheckedPortId>,
) {
    if port.is_some() {
        features.push(RuntimeFeature::TypedBoundaryTables);
    }
}

fn collect_template_requirements(
    features: &mut FeatureAccumulator,
    template: &CheckedValueTemplate,
) {
    features.push(RuntimeFeature::TypedValueTemplates);
    match template {
        CheckedValueTemplate::Literal(_) => {}
        CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. } => {}
        CheckedValueTemplate::EnumPayload { value, .. }
        | CheckedValueTemplate::RecordField { record: value, .. } => {
            collect_template_requirements(features, value);
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            collect_template_requirements(features, list);
        }
        CheckedValueTemplate::MapValue { map, .. } | CheckedValueTemplate::MapRest { map, .. } => {
            collect_template_requirements(features, map);
        }
        CheckedValueTemplate::ProcessRef { .. } => {
            features.push(RuntimeFeature::LocalSend);
        }
        CheckedValueTemplate::LoopElement { .. } => {
            features.push(RuntimeFeature::RuntimeForEach);
        }
        CheckedValueTemplate::EffectOutcome { .. } => {
            features.push(RuntimeFeature::TypedEffectOutcomes);
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            collect_template_requirements(features, payload);
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                collect_template_requirements(features, field.value());
            }
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                collect_template_requirements(features, item);
            }
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                collect_template_requirements(features, entry.key());
                collect_template_requirements(features, entry.value());
            }
        }
        CheckedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            features.push(RuntimeFeature::RuntimeBranching);
            collect_template_requirements(features, condition);
            collect_template_requirements(features, then_value);
            collect_template_requirements(features, else_value);
        }
        CheckedValueTemplate::Equality { left, right, .. } => {
            collect_template_requirements(features, left);
            collect_template_requirements(features, right);
        }
        CheckedValueTemplate::ScalarArithmetic { left, right, .. }
        | CheckedValueTemplate::ScalarOrdering { left, right, .. } => {
            features.push(RuntimeFeature::ScalarValueTemplates);
            collect_template_requirements(features, left);
            collect_template_requirements(features, right);
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            collect_template_requirements(features, operand);
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            collect_template_requirements(features, left);
            collect_template_requirements(features, right);
        }
    }
}
