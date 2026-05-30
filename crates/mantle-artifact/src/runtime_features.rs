use std::borrow::Cow;

use crate::artifact::{
    ArtifactAction, ArtifactCapabilityDescriptor, ArtifactEffect, ArtifactSendTarget,
    ArtifactTypeKind, ArtifactValueShape, ArtifactValueTemplate, MantleArtifact, NextState,
};
use crate::validation::validate_ident_field;
use crate::{Error, MAX_RUNTIME_FEATURE_REQUIREMENTS, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeFeature {
    BoundedMailbox,
    ComponentCompositionMetadata,
    DistributedTransport,
    EmitEffect,
    JsonlTrace,
    LocalExecution,
    LocalSend,
    LocalSpawn,
    LocalSupervision,
    RemoteSend,
    RemoteSpawn,
    RuntimeBranching,
    RuntimeForEach,
    ScalarValueTemplates,
    TypedBoundaryTables,
    TypedEffectOutcomes,
    TypedValueTemplates,
}

impl RuntimeFeature {
    pub const COUNT: usize = 17;

    pub const ALL: [Self; Self::COUNT] = [
        Self::BoundedMailbox,
        Self::ComponentCompositionMetadata,
        Self::DistributedTransport,
        Self::EmitEffect,
        Self::JsonlTrace,
        Self::LocalExecution,
        Self::LocalSend,
        Self::LocalSpawn,
        Self::LocalSupervision,
        Self::RemoteSend,
        Self::RemoteSpawn,
        Self::RuntimeBranching,
        Self::RuntimeForEach,
        Self::ScalarValueTemplates,
        Self::TypedBoundaryTables,
        Self::TypedEffectOutcomes,
        Self::TypedValueTemplates,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BoundedMailbox => "bounded_mailbox",
            Self::ComponentCompositionMetadata => "component_composition_metadata",
            Self::DistributedTransport => "distributed_transport",
            Self::EmitEffect => "emit_effect",
            Self::JsonlTrace => "jsonl_trace",
            Self::LocalExecution => "local_execution",
            Self::LocalSend => "local_send",
            Self::LocalSpawn => "local_spawn",
            Self::LocalSupervision => "local_supervision",
            Self::RemoteSend => "remote_send",
            Self::RemoteSpawn => "remote_spawn",
            Self::RuntimeBranching => "runtime_branching",
            Self::RuntimeForEach => "runtime_for_each",
            Self::ScalarValueTemplates => "scalar_value_templates",
            Self::TypedBoundaryTables => "typed_boundary_tables",
            Self::TypedEffectOutcomes => "typed_effect_outcomes",
            Self::TypedValueTemplates => "typed_value_templates",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "bounded_mailbox" => Ok(Self::BoundedMailbox),
            "component_composition_metadata" => Ok(Self::ComponentCompositionMetadata),
            "distributed_transport" => Ok(Self::DistributedTransport),
            "emit_effect" => Ok(Self::EmitEffect),
            "jsonl_trace" => Ok(Self::JsonlTrace),
            "local_execution" => Ok(Self::LocalExecution),
            "local_send" => Ok(Self::LocalSend),
            "local_spawn" => Ok(Self::LocalSpawn),
            "local_supervision" => Ok(Self::LocalSupervision),
            "remote_send" => Ok(Self::RemoteSend),
            "remote_spawn" => Ok(Self::RemoteSpawn),
            "runtime_branching" => Ok(Self::RuntimeBranching),
            "runtime_for_each" => Ok(Self::RuntimeForEach),
            "scalar_value_templates" => Ok(Self::ScalarValueTemplates),
            "typed_boundary_tables" => Ok(Self::TypedBoundaryTables),
            "typed_effect_outcomes" => Ok(Self::TypedEffectOutcomes),
            "typed_value_templates" => Ok(Self::TypedValueTemplates),
            _ => Err(Error::new(format!("invalid runtime feature {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTargetRequirements {
    pub source_language: Cow<'static, str>,
    pub features: Vec<RuntimeFeature>,
}

impl ArtifactTargetRequirements {
    pub fn new(
        source_language: impl Into<Cow<'static, str>>,
        mut features: Vec<RuntimeFeature>,
    ) -> Self {
        features.sort_unstable();
        features.dedup();
        Self {
            source_language: source_language.into(),
            features,
        }
    }

    pub fn validate(&self, artifact_source_language: &str) -> Result<()> {
        validate_ident_field(
            "target_requirements.source_language",
            self.source_language.as_ref(),
        )?;
        if self.source_language.as_ref() != artifact_source_language {
            return Err(Error::new(format!(
                "target requirements source_language {:?} does not match artifact source_language {:?}",
                self.source_language, artifact_source_language
            )));
        }
        if self.features.is_empty() {
            return Err(Error::new(
                "target requirements must declare at least one runtime feature",
            ));
        }
        if self.features.len() > MAX_RUNTIME_FEATURE_REQUIREMENTS {
            return Err(Error::new(format!(
                "target requirements feature_count must be no greater than {MAX_RUNTIME_FEATURE_REQUIREMENTS}"
            )));
        }
        for pair in self.features.windows(2) {
            match pair {
                [left, right] if left == right => {
                    return Err(Error::new(format!(
                        "duplicate target requirement runtime feature {}",
                        left.as_str()
                    )));
                }
                [left, right] if left > right => {
                    return Err(Error::new(
                        "target requirement runtime features must be sorted by canonical feature id",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn validate_covers_artifact(&self, artifact: &MantleArtifact) -> Result<()> {
        let required = required_runtime_feature_set_for_artifact(artifact);
        for feature in required.as_slice() {
            if !self.features.contains(feature) {
                return Err(Error::new(format!(
                    "target requirements do not declare required runtime feature {} for artifact contents",
                    feature.as_str()
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRequirementsFormat {
    Text,
    Json,
}

pub fn render_artifact_target_requirements(
    artifact: &crate::MantleArtifact,
    subject: &str,
    format: TargetRequirementsFormat,
) -> Result<String> {
    artifact.validate()?;
    Ok(match format {
        TargetRequirementsFormat::Text => render_text(artifact, subject),
        TargetRequirementsFormat::Json => render_json(artifact, subject),
    })
}

fn render_text(artifact: &crate::MantleArtifact, subject: &str) -> String {
    let mut out = String::new();
    out.push_str("mantle target requirements ");
    out.push_str(subject);
    out.push('\n');
    out.push_str("format: ");
    out.push_str(artifact.format.as_ref());
    out.push('\n');
    out.push_str("schema_version: ");
    out.push_str(artifact.schema_version.as_ref());
    out.push('\n');
    out.push_str("source_language: ");
    out.push_str(artifact.target_requirements.source_language.as_ref());
    out.push('\n');
    out.push_str("module: ");
    out.push_str(&artifact.module);
    out.push('\n');
    out.push_str("features:\n");
    for feature in &artifact.target_requirements.features {
        out.push_str("  - ");
        out.push_str(feature.as_str());
        out.push('\n');
    }
    out
}

fn render_json(artifact: &crate::MantleArtifact, subject: &str) -> String {
    let mut out = String::new();
    out.push('{');
    push_json_field(&mut out, "target", subject);
    out.push(',');
    push_json_field(&mut out, "format", artifact.format.as_ref());
    out.push(',');
    push_json_field(&mut out, "schema_version", artifact.schema_version.as_ref());
    out.push(',');
    push_json_field(
        &mut out,
        "source_language",
        artifact.target_requirements.source_language.as_ref(),
    );
    out.push(',');
    push_json_field(&mut out, "module", &artifact.module);
    out.push_str(",\"features\":[");
    for (index, feature) in artifact.target_requirements.features.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(&mut out, feature.as_str());
    }
    out.push_str("]}");
    out
}

fn push_json_field(out: &mut String, key: &str, value: &str) {
    push_json_string(out, key);
    out.push(':');
    push_json_string(out, value);
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => push_json_control_escape(out, c),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_json_control_escape(out: &mut String, ch: char) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let value = ch as usize;
    out.push_str("\\u00");
    out.push(char::from(HEX[(value >> 4) & 0x0f]));
    out.push(char::from(HEX[value & 0x0f]));
}

fn required_runtime_feature_set_for_artifact(artifact: &MantleArtifact) -> FeatureAccumulator {
    let mut features = FeatureAccumulator::new();
    features.push(RuntimeFeature::BoundedMailbox);
    features.push(RuntimeFeature::JsonlTrace);
    features.push(RuntimeFeature::LocalExecution);

    if !artifact.protocols.is_empty()
        || !artifact.ports.is_empty()
        || !artifact.components.is_empty()
    {
        features.push(RuntimeFeature::TypedBoundaryTables);
    }
    if !artifact.compositions.is_empty() {
        features.push(RuntimeFeature::ComponentCompositionMetadata);
        features.push(RuntimeFeature::TypedBoundaryTables);
    }

    for ty in &artifact.types {
        match ty.kind {
            ArtifactTypeKind::Value => {
                if let Some(shape) = &ty.shape {
                    collect_shape_requirements(&mut features, shape);
                }
            }
            ArtifactTypeKind::ProcessRef { .. } => {
                features.push(RuntimeFeature::LocalSend);
            }
        }
    }

    for process in &artifact.processes {
        if !process.supervisor_plans.is_empty() {
            features.push(RuntimeFeature::LocalSupervision);
        }
        for authority in &process.authorities {
            collect_authority_requirements(&mut features, authority.descriptor);
        }
        for transition in &process.transitions {
            if transition.payload_guard.is_some() {
                features.push(RuntimeFeature::TypedValueTemplates);
            }
            for effect in &transition.effects {
                collect_effect_requirements(&mut features, *effect);
            }
            collect_next_state_requirements(&mut features, &transition.next_state);
            for action in &transition.actions {
                collect_action_requirements(&mut features, action);
            }
        }
    }

    features
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

    fn as_slice(&self) -> &[RuntimeFeature] {
        &self.features[..self.len]
    }
}

fn collect_shape_requirements(features: &mut FeatureAccumulator, shape: &ArtifactValueShape) {
    match shape {
        ArtifactValueShape::Atom | ArtifactValueShape::Record { .. } => {}
        ArtifactValueShape::Scalar { .. } => {
            features.push(RuntimeFeature::ScalarValueTemplates);
        }
        ArtifactValueShape::Enum { variants } => {
            if variants
                .iter()
                .any(|variant| variant.payload_type.is_some())
            {
                features.push(RuntimeFeature::TypedValueTemplates);
            }
        }
        ArtifactValueShape::List { .. } | ArtifactValueShape::Map { .. } => {
            features.push(RuntimeFeature::TypedValueTemplates);
        }
    }
}

fn collect_authority_requirements(
    features: &mut FeatureAccumulator,
    descriptor: ArtifactCapabilityDescriptor,
) {
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { .. } => {
            features.push(RuntimeFeature::LocalSpawn);
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { .. }
        | ArtifactCapabilityDescriptor::PortConnect { .. }
        | ArtifactCapabilityDescriptor::ComponentExport { .. } => {
            features.push(RuntimeFeature::TypedBoundaryTables);
        }
    }
}

fn collect_effect_requirements(features: &mut FeatureAccumulator, effect: ArtifactEffect) {
    match effect {
        ArtifactEffect::Emit => {
            features.push(RuntimeFeature::EmitEffect);
        }
        ArtifactEffect::Spawn => {
            features.push(RuntimeFeature::LocalSpawn);
        }
        ArtifactEffect::Send => {
            features.push(RuntimeFeature::LocalSend);
        }
    }
}

fn collect_next_state_requirements(features: &mut FeatureAccumulator, next_state: &NextState) {
    match next_state {
        NextState::Current | NextState::Value(_) => {}
        NextState::Template(template) => collect_template_requirements(features, template),
        NextState::IfElse {
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

fn collect_action_requirements(features: &mut FeatureAccumulator, action: &ArtifactAction) {
    match action {
        ArtifactAction::Emit { .. } => {
            features.push(RuntimeFeature::EmitEffect);
        }
        ArtifactAction::Spawn { .. } => {
            features.push(RuntimeFeature::LocalSpawn);
        }
        ArtifactAction::SpawnOutcome { .. } => {
            features.push(RuntimeFeature::LocalSpawn);
            features.push(RuntimeFeature::TypedEffectOutcomes);
        }
        ArtifactAction::Send {
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
        ArtifactAction::SendOutcome {
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
        ArtifactAction::IfElse {
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
        ArtifactAction::ForEach {
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

fn collect_send_target_requirements(
    features: &mut FeatureAccumulator,
    target: &ArtifactSendTarget,
) {
    match target {
        ArtifactSendTarget::ProcessRef(_) => {}
        ArtifactSendTarget::SupervisorChild { .. } => {
            features.push(RuntimeFeature::LocalSupervision);
        }
        ArtifactSendTarget::ReceivedPayload { .. } => {
            features.push(RuntimeFeature::TypedValueTemplates);
        }
    }
}

fn collect_port_requirements(features: &mut FeatureAccumulator, port: Option<crate::PortId>) {
    if port.is_some() {
        features.push(RuntimeFeature::TypedBoundaryTables);
    }
}

fn collect_template_requirements(
    features: &mut FeatureAccumulator,
    template: &ArtifactValueTemplate,
) {
    features.push(RuntimeFeature::TypedValueTemplates);
    match template {
        ArtifactValueTemplate::Literal { .. }
        | ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. } => {}
        ArtifactValueTemplate::EnumPayload { value, .. }
        | ArtifactValueTemplate::RecordField { record: value, .. }
        | ArtifactValueTemplate::EnumVariant { payload: value, .. } => {
            collect_template_requirements(features, value);
        }
        ArtifactValueTemplate::ListElement { list, .. }
        | ArtifactValueTemplate::ListPrefixElement { list, .. }
        | ArtifactValueTemplate::ListRest { list, .. } => {
            collect_template_requirements(features, list);
        }
        ArtifactValueTemplate::MapValue { map, .. }
        | ArtifactValueTemplate::MapRest { map, .. } => {
            collect_template_requirements(features, map);
        }
        ArtifactValueTemplate::ProcessRef { .. } => {
            features.push(RuntimeFeature::LocalSend);
        }
        ArtifactValueTemplate::LoopElement { .. } => {
            features.push(RuntimeFeature::RuntimeForEach);
        }
        ArtifactValueTemplate::EffectOutcome { .. } => {
            features.push(RuntimeFeature::TypedEffectOutcomes);
        }
        ArtifactValueTemplate::Record { fields, .. } => {
            for field in fields {
                collect_template_requirements(features, &field.value);
            }
        }
        ArtifactValueTemplate::List { items, .. } => {
            for item in items {
                collect_template_requirements(features, item);
            }
        }
        ArtifactValueTemplate::Map { entries, .. } => {
            for entry in entries {
                collect_template_requirements(features, &entry.key);
                collect_template_requirements(features, &entry.value);
            }
        }
        ArtifactValueTemplate::IfElse {
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
        ArtifactValueTemplate::Equality { left, right, .. } => {
            collect_template_requirements(features, left);
            collect_template_requirements(features, right);
        }
        ArtifactValueTemplate::ScalarArithmetic { left, right, .. }
        | ArtifactValueTemplate::ScalarOrdering { left, right, .. } => {
            features.push(RuntimeFeature::ScalarValueTemplates);
            collect_template_requirements(features, left);
            collect_template_requirements(features, right);
        }
        ArtifactValueTemplate::BooleanNot { operand, .. } => {
            collect_template_requirements(features, operand);
        }
        ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
            collect_template_requirements(features, left);
            collect_template_requirements(features, right);
        }
    }
}
