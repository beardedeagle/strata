use std::collections::BTreeSet;
use std::fmt;

use crate::fields::ArtifactFields;
use crate::validation::{
    process_ref_type_target, validate_count, validate_encoded_artifact_size, validate_ident_field,
    validate_output_text, validate_source_hash, validate_type_field,
    validate_unique_message_variant_list, validate_unique_state_value_list, validate_value_label,
};
use crate::{
    ARTIFACT_FORMAT, ARTIFACT_MAGIC, ARTIFACT_SCHEMA_VERSION, Error, MAX_ACTIONS_PER_PROCESS,
    MAX_EFFECTS_PER_TRANSITION, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS,
    MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT, MAX_PROCESS_REFS_PER_PROCESS,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS, MAX_VALUE_TEMPLATE_DEPTH,
    MAX_VALUE_TEMPLATE_FIELDS, MessageId, OutputId, ProcessId, ProcessRefId, Result, StateId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Stop,
    Panic,
}

impl StepResult {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::Stop => "Stop",
            Self::Panic => "Panic",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "Continue" => Ok(Self::Continue),
            "Stop" => Ok(Self::Stop),
            "Panic" => Ok(Self::Panic),
            _ => Err(Error::new(format!("invalid step_result value {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactEffect {
    Emit,
    Spawn,
    Send,
}

impl ArtifactEffect {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Emit => "emit",
            Self::Spawn => "spawn",
            Self::Send => "send",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "emit" => Ok(Self::Emit),
            "spawn" => Ok(Self::Spawn),
            "send" => Ok(Self::Send),
            _ => Err(Error::new(format!("invalid effect value {value:?}"))),
        }
    }
}

impl fmt::Display for ArtifactEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextState {
    Current,
    Value(StateId),
    Template(ArtifactValueTemplate),
}

impl NextState {
    pub(crate) fn kind_str(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Value(_) => "value",
            Self::Template(_) => "template",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub schema_version: String,
    pub source_language: String,
    pub module: String,
    pub entry_process: ProcessId,
    pub entry_message: MessageId,
    pub outputs: Vec<String>,
    pub processes: Vec<ArtifactProcess>,
    pub source_hash_fnv1a64: String,
}

impl MantleArtifact {
    pub fn encode(&self) -> String {
        let mut encoded = format!(
            "{ARTIFACT_MAGIC}\nformat={}\nschema_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\noutput_count={}\nprocess_count={}\n",
            self.format,
            self.schema_version,
            self.source_language,
            self.module,
            self.entry_process.as_u32(),
            self.entry_message.as_u32(),
            self.outputs.len(),
            self.processes.len()
        );
        for (output_index, output) in self.outputs.iter().enumerate() {
            encoded.push_str(&format!("output.{output_index}={output}\n"));
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            let prefix = format!("process.{process_index}");
            encoded.push_str(&format!(
                "{prefix}.debug_name={}\n{prefix}.state_type={}\n{prefix}.state_value_count={}\n",
                process.debug_name,
                process.state_type,
                process.state_values.len()
            ));
            for (value_index, value) in process.state_values.iter().enumerate() {
                encode_state_value(
                    &mut encoded,
                    &format!("{prefix}.state_value.{value_index}"),
                    value,
                );
            }
            encoded.push_str(&format!(
                "{prefix}.message_type={}\n{prefix}.message_count={}\n",
                process.message_type,
                process.message_variants.len()
            ));
            for (message_index, message) in process.message_variants.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.message.{message_index}={}\n",
                    message.label
                ));
                if let Some(payload_type) = &message.payload_type {
                    encoded.push_str(&format!(
                        "{prefix}.message.{message_index}.payload_type={payload_type}\n"
                    ));
                }
            }
            encoded.push_str(&format!(
                "{prefix}.process_ref_count={}\n",
                process.process_refs.len()
            ));
            for (process_ref_index, process_ref) in process.process_refs.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.process_ref.{process_ref_index}.debug_name={}\n{prefix}.process_ref.{process_ref_index}.target_process={}\n",
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                ));
            }
            encoded.push_str(&format!(
                "{prefix}.mailbox_bound={}\n{prefix}.init_state={}\n{prefix}.transition_count={}\n",
                process.mailbox_bound,
                process.init_state.as_u32(),
                process.transitions.len()
            ));
            for (transition_index, transition) in process.transitions.iter().enumerate() {
                let transition_prefix = format!("{prefix}.transition.{transition_index}");
                encoded.push_str(&format!(
                    "{transition_prefix}.message={}\n{transition_prefix}.step_result={}\n{transition_prefix}.next_state={}\n",
                    transition.message.as_u32(),
                    transition.step_result.as_str(),
                    transition.next_state.kind_str()
                ));
                if let NextState::Value(state) = &transition.next_state {
                    encoded.push_str(&format!(
                        "{transition_prefix}.next_state_value={}\n",
                        state.as_u32()
                    ));
                }
                if let NextState::Template(template) = &transition.next_state {
                    encode_value_template(
                        &mut encoded,
                        &format!("{transition_prefix}.next_state_template"),
                        template,
                    );
                }
                encoded.push_str(&format!(
                    "{transition_prefix}.effect_count={}\n",
                    transition.effects.len()
                ));
                for (effect_index, effect) in transition.effects.iter().enumerate() {
                    encoded.push_str(&format!(
                        "{transition_prefix}.effect.{effect_index}={}\n",
                        effect.as_str()
                    ));
                }
                encoded.push_str(&format!(
                    "{transition_prefix}.action_count={}\n",
                    transition.actions.len()
                ));
                for (action_index, action) in transition.actions.iter().enumerate() {
                    let action_prefix = format!("{transition_prefix}.action.{action_index}");
                    encode_action(&mut encoded, &action_prefix, action);
                }
            }
        }

        encoded.push_str(&format!(
            "source_hash_fnv1a64={}\n",
            self.source_hash_fnv1a64
        ));
        encoded
    }

    pub fn decode(contents: &str) -> Result<Self> {
        let mut fields = ArtifactFields::parse(contents)?;
        let format = fields.take_required("format")?;
        let schema_version = fields.take_required("schema_version")?;
        validate_artifact_identity(&format, &schema_version)?;

        let process_count = fields.take_bounded_usize("process_count", 1, MAX_PROCESS_COUNT)?;
        let output_count = fields.take_bounded_usize("output_count", 0, MAX_OUTPUT_LITERALS)?;
        let mut outputs = Vec::with_capacity(output_count);
        for output_index in 0..output_count {
            outputs.push(fields.take_required(&format!("output.{output_index}"))?);
        }

        let mut processes = Vec::with_capacity(process_count);
        for process_index in 0..process_count {
            let prefix = format!("process.{process_index}");
            let state_value_count = fields.take_bounded_usize(
                &format!("{prefix}.state_value_count"),
                1,
                MAX_STATE_VALUES_PER_PROCESS,
            )?;
            let mut state_values = Vec::with_capacity(state_value_count);
            for value_index in 0..state_value_count {
                state_values.push(decode_state_value(
                    &mut fields,
                    &format!("{prefix}.state_value.{value_index}"),
                )?);
            }

            let message_count = fields.take_bounded_usize(
                &format!("{prefix}.message_count"),
                1,
                MAX_MESSAGE_VARIANTS_PER_PROCESS,
            )?;
            let mut message_variants = Vec::with_capacity(message_count);
            for message_index in 0..message_count {
                message_variants.push(ArtifactMessageVariant {
                    label: fields.take_required(&format!("{prefix}.message.{message_index}"))?,
                    payload_type: fields
                        .take_optional(&format!("{prefix}.message.{message_index}.payload_type")),
                });
            }

            let process_ref_count = fields.take_bounded_usize(
                &format!("{prefix}.process_ref_count"),
                0,
                MAX_PROCESS_REFS_PER_PROCESS,
            )?;
            let mut process_refs = Vec::with_capacity(process_ref_count);
            for process_ref_index in 0..process_ref_count {
                let process_ref_prefix = format!("{prefix}.process_ref.{process_ref_index}");
                process_refs.push(ArtifactProcessRef {
                    debug_name: fields
                        .take_required(&format!("{process_ref_prefix}.debug_name"))?,
                    target: fields
                        .take_process_id(&format!("{process_ref_prefix}.target_process"))?,
                });
            }

            let transition_count = fields.take_bounded_usize(
                &format!("{prefix}.transition_count"),
                1,
                MAX_TRANSITIONS_PER_PROCESS,
            )?;
            let mut transitions = Vec::with_capacity(transition_count);
            for transition_index in 0..transition_count {
                let transition_prefix = format!("{prefix}.transition.{transition_index}");
                let effect_count = fields.take_bounded_usize(
                    &format!("{transition_prefix}.effect_count"),
                    0,
                    MAX_EFFECTS_PER_TRANSITION,
                )?;
                let mut effects = Vec::with_capacity(effect_count);
                for effect_index in 0..effect_count {
                    let key = format!("{transition_prefix}.effect.{effect_index}");
                    effects.push(ArtifactEffect::parse(&fields.take_required(&key)?)?);
                }
                let action_count = fields.take_bounded_usize(
                    &format!("{transition_prefix}.action_count"),
                    0,
                    MAX_ACTIONS_PER_PROCESS,
                )?;
                let mut actions = Vec::with_capacity(action_count);
                for action_index in 0..action_count {
                    let action_prefix = format!("{transition_prefix}.action.{action_index}");
                    actions.push(decode_action(&mut fields, &action_prefix)?);
                }

                transitions.push(ArtifactTransition {
                    message: fields.take_message_id(&format!("{transition_prefix}.message"))?,
                    step_result: fields
                        .take_step_result(&format!("{transition_prefix}.step_result"))?,
                    next_state: decode_next_state(&mut fields, &transition_prefix)?,
                    effects,
                    actions,
                });
            }

            processes.push(ArtifactProcess {
                debug_name: fields.take_required(&format!("{prefix}.debug_name"))?,
                state_type: fields.take_required(&format!("{prefix}.state_type"))?,
                state_values,
                message_type: fields.take_required(&format!("{prefix}.message_type"))?,
                message_variants,
                process_refs,
                mailbox_bound: fields.take_bounded_usize(
                    &format!("{prefix}.mailbox_bound"),
                    1,
                    MAX_MAILBOX_BOUND,
                )?,
                init_state: fields.take_state_id(&format!("{prefix}.init_state"))?,
                transitions,
            });
        }

        let artifact = Self {
            format,
            schema_version,
            source_language: fields.take_required("source_language")?,
            module: fields.take_required("module")?,
            entry_process: fields.take_process_id("entry_process")?,
            entry_message: fields.take_message_id("entry_message")?,
            outputs,
            processes,
            source_hash_fnv1a64: fields.take_required("source_hash_fnv1a64")?,
        };

        fields.finish()?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<()> {
        validate_artifact_identity(&self.format, &self.schema_version)?;
        validate_ident_field("source_language", &self.source_language)?;
        validate_ident_field("module", &self.module)?;
        validate_source_hash(&self.source_hash_fnv1a64)?;
        validate_count("process_count", self.processes.len(), 1, MAX_PROCESS_COUNT)?;
        validate_count("output_count", self.outputs.len(), 0, MAX_OUTPUT_LITERALS)?;
        for output in &self.outputs {
            validate_output_text(output)?;
        }

        let mut process_debug_names = BTreeSet::new();
        for process in &self.processes {
            process.validate_identity()?;
            if !process_debug_names.insert(process.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate process debug_name {}",
                    process.debug_name
                )));
            }
        }

        let Some(entry_process) = self.processes.get(self.entry_process.index()) else {
            return Err(Error::new(format!(
                "entry process id {} is not defined",
                self.entry_process.as_u32()
            )));
        };
        if self.entry_message.index() >= entry_process.message_variants.len() {
            return Err(Error::new(format!(
                "entry message id {} is not accepted by process id {}",
                self.entry_message.as_u32(),
                self.entry_process.as_u32()
            )));
        }
        if entry_process.message_variants[self.entry_message.index()]
            .payload_type
            .is_some()
        {
            return Err(Error::new(format!(
                "entry message id {} must not require a payload",
                self.entry_message.as_u32()
            )));
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            process.validate_references(self, ProcessId::from_index(process_index)?)?;
        }
        validate_encoded_artifact_size(self)?;

        Ok(())
    }
}

fn validate_artifact_identity(format: &str, schema_version: &str) -> Result<()> {
    if format != ARTIFACT_FORMAT {
        return Err(Error::new(format!(
            "unsupported artifact format {format}; expected {ARTIFACT_FORMAT}"
        )));
    }
    if schema_version != ARTIFACT_SCHEMA_VERSION {
        return Err(Error::new(format!(
            "unsupported artifact schema version {schema_version}; expected {ARTIFACT_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn validate_unique_process_ref_list(process_refs: &[ArtifactProcessRef]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for process_ref in process_refs {
        validate_ident_field("process reference", &process_ref.debug_name)?;
        if !seen.insert(process_ref.debug_name.as_str()) {
            return Err(Error::new(format!(
                "duplicate process reference {}",
                process_ref.debug_name
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMessageVariant {
    pub label: String,
    pub payload_type: Option<String>,
}

impl ArtifactMessageVariant {
    pub fn unit(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            payload_type: None,
        }
    }

    pub fn payload(label: impl Into<String>, payload_type: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            payload_type: Some(payload_type.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactStateValue {
    pub ty: String,
    pub value: String,
    pub label: String,
}

impl ArtifactStateValue {
    pub fn new(ty: impl Into<String>, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            ty: ty.into(),
            label: value.clone(),
            value,
        }
    }

    pub fn with_label(
        ty: impl Into<String>,
        value: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            ty: ty.into(),
            value: value.into(),
            label: label.into(),
        }
    }

    fn has_same_identity(&self, other: &Self) -> bool {
        self.ty == other.ty && self.value == other.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcess {
    pub debug_name: String,
    pub state_type: String,
    pub state_values: Vec<ArtifactStateValue>,
    pub message_type: String,
    pub message_variants: Vec<ArtifactMessageVariant>,
    pub process_refs: Vec<ArtifactProcessRef>,
    pub mailbox_bound: usize,
    pub init_state: StateId,
    pub transitions: Vec<ArtifactTransition>,
}

impl ArtifactProcess {
    fn validate_identity(&self) -> Result<()> {
        validate_ident_field("process debug_name", &self.debug_name)?;
        validate_ident_field("state_type", &self.state_type)?;
        validate_ident_field("message_type", &self.message_type)?;
        validate_count("mailbox_bound", self.mailbox_bound, 1, MAX_MAILBOX_BOUND)?;
        validate_count(
            "state_value_count",
            self.state_values.len(),
            1,
            MAX_STATE_VALUES_PER_PROCESS,
        )?;
        validate_count(
            "message_count",
            self.message_variants.len(),
            1,
            MAX_MESSAGE_VARIANTS_PER_PROCESS,
        )?;
        validate_count(
            "process_ref_count",
            self.process_refs.len(),
            0,
            MAX_PROCESS_REFS_PER_PROCESS,
        )?;
        validate_count(
            "transition_count",
            self.transitions.len(),
            1,
            MAX_TRANSITIONS_PER_PROCESS,
        )?;
        validate_unique_state_value_list(&self.state_values)?;
        for state_value in &self.state_values {
            if state_value.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} state value {} (label {}) has type {}, expected {}",
                    self.debug_name,
                    state_value.value,
                    state_value.label,
                    state_value.ty,
                    self.state_type
                )));
            }
        }
        validate_unique_message_variant_list(&self.message_variants)?;
        validate_unique_process_ref_list(&self.process_refs)?;
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a valid state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.transitions.len() != self.message_variants.len() {
            return Err(Error::new(format!(
                "process {} transition_count must equal message_count",
                self.debug_name
            )));
        }
        let mut transition_messages = BTreeSet::new();
        let mut action_count = 0usize;
        for transition in &self.transitions {
            if !transition_messages.insert(transition.message.as_u32()) {
                return Err(Error::new(format!(
                    "process {} declares duplicate transition for message id {}",
                    self.debug_name,
                    transition.message.as_u32()
                )));
            }
            if transition.message.index() >= self.message_variants.len() {
                return Err(Error::new(format!(
                    "process {} transition message id {} is not accepted",
                    self.debug_name,
                    transition.message.as_u32()
                )));
            }
            match &transition.next_state {
                NextState::Current => {}
                NextState::Value(state) => {
                    if state.index() >= self.state_values.len() {
                        return Err(Error::new(format!(
                            "process {} transition next_state id {} is not a valid state value",
                            self.debug_name,
                            state.as_u32()
                        )));
                    }
                }
                NextState::Template(template) => {
                    let received_payload_type = self
                        .message_variants
                        .get(transition.message.index())
                        .and_then(|message| message.payload_type.as_deref());
                    template.validate_for_received_payload(
                        &format!(
                            "process {} transition {} next_state_template",
                            self.debug_name,
                            transition.message.as_u32()
                        ),
                        Some(&self.state_type),
                        received_payload_type,
                        0,
                    )?;
                    self.validate_static_next_state_template_value(transition, template)?;
                }
            }
            action_count = action_count
                .checked_add(transition.actions.len())
                .ok_or_else(|| Error::new("process action_count overflowed"))?;
        }
        validate_count("action_count", action_count, 0, MAX_ACTIONS_PER_PROCESS)?;
        for message_index in 0..self.message_variants.len() {
            if !transition_messages.contains(&(message_index as u32)) {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {}",
                    self.debug_name, message_index
                )));
            }
        }
        Ok(())
    }

    fn validate_static_next_state_template_value(
        &self,
        transition: &ArtifactTransition,
        template: &ArtifactValueTemplate,
    ) -> Result<()> {
        if template.depends_on_received_payload() {
            return Ok(());
        }
        let value = template.evaluate_state_value(None)?;
        if self
            .state_values
            .iter()
            .any(|state_value| state_value.has_same_identity(&value))
        {
            return Ok(());
        }
        Err(Error::new(format!(
            "process {} transition {} next_state_template produced value {} not admitted by state table",
            self.debug_name,
            transition.message.as_u32(),
            value.label
        )))
    }

    fn validate_references(&self, artifact: &MantleArtifact, process_id: ProcessId) -> Result<()> {
        for process_ref in &self.process_refs {
            if process_ref.target.index() >= artifact.processes.len() {
                return Err(Error::new(format!(
                    "process {} process reference {} targets undefined process id {}",
                    self.debug_name,
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == artifact.entry_process {
                return Err(Error::new(format!(
                    "process {} process reference {} targets entry process id {}",
                    self.debug_name,
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == process_id {
                return Err(Error::new(format!(
                    "process {} process reference {} targets itself, which is not supported",
                    self.debug_name, process_ref.debug_name
                )));
            }
        }
        for transition in &self.transitions {
            let declared_effects = transition.validate_effects(&self.debug_name)?;
            let mut spawned_refs = BTreeSet::new();
            let mut used_effects = BTreeSet::new();
            for action in &transition.actions {
                let action_effect = action.effect();
                if !declared_effects.contains(&action_effect) {
                    return Err(Error::new(format!(
                        "process {} transition {} uses effect {action_effect} but does not declare it",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                used_effects.insert(action_effect);
                self.validate_action_reference(artifact, transition, &mut spawned_refs, action)?;
            }
            for declared_effect in &declared_effects {
                if !used_effects.contains(declared_effect) {
                    return Err(Error::new(format!(
                        "process {} transition {} declares effect {declared_effect} but no action uses it",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_action_reference(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        spawned_refs: &mut BTreeSet<ProcessRefId>,
        action: &ArtifactAction,
    ) -> Result<()> {
        match action {
            ArtifactAction::Emit { output } => {
                if output.index() >= artifact.outputs.len() {
                    return Err(Error::new(format!(
                        "process {} emits undefined output id {}",
                        self.debug_name,
                        output.as_u32()
                    )));
                }
            }
            ArtifactAction::Spawn {
                target,
                process_ref,
            } => {
                let declared_target = self.process_ref_target(*process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn process reference id {} targets process id {}, expected {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                if !spawned_refs.insert(*process_ref) {
                    return Err(Error::new(format!(
                        "process {} duplicates process reference id {} within message transition {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        transition.message.as_u32()
                    )));
                }
            }
            ArtifactAction::Send {
                target,
                message,
                payload,
            } => {
                let target_process_id = self.validate_send_target_reference(
                    artifact,
                    target,
                    transition,
                    spawned_refs,
                )?;
                let target_process = artifact
                    .processes
                    .get(target_process_id.index())
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} sends to undefined process id {}",
                            self.debug_name,
                            target_process_id.as_u32()
                        ))
                    })?;
                if message.index() >= target_process.message_variants.len() {
                    return Err(Error::new(format!(
                        "process {} sends message id {} not accepted by process id {}",
                        self.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32()
                    )));
                }
                let target_message = &target_process.message_variants[message.index()];
                match (&target_message.payload_type, payload) {
                    (None, None) => {}
                    (None, Some(_)) => {
                        return Err(Error::new(format!(
                            "process {} sends payload to process id {} message id {}, which does not accept one",
                            self.debug_name,
                            target_process_id.as_u32(),
                            message.as_u32()
                        )));
                    }
                    (Some(_), None) => {
                        return Err(Error::new(format!(
                            "process {} sends process id {} message id {} without required payload",
                            self.debug_name,
                            target_process_id.as_u32(),
                            message.as_u32()
                        )));
                    }
                    (Some(payload_type), Some(payload)) => {
                        self.validate_template_process_refs(artifact, payload, spawned_refs)?;
                        let received_payload_type = self
                            .message_variants
                            .get(transition.message.index())
                            .and_then(|message| message.payload_type.as_deref());
                        payload.validate_for_received_payload(
                            &format!(
                                "process {} transition {} send payload",
                                self.debug_name,
                                transition.message.as_u32()
                            ),
                            Some(payload_type),
                            received_payload_type,
                            0,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_send_target_reference(
        &self,
        artifact: &MantleArtifact,
        target: &ArtifactSendTarget,
        transition: &ArtifactTransition,
        spawned_refs: &BTreeSet<ProcessRefId>,
    ) -> Result<ProcessId> {
        match target {
            ArtifactSendTarget::ProcessRef(process_ref) => {
                let target_process_id = self.process_ref_target(*process_ref)?;
                if !spawned_refs.contains(process_ref) {
                    return Err(Error::new(format!(
                        "process {} sends through unbound process reference id {} within message transition {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        transition.message.as_u32()
                    )));
                }
                Ok(target_process_id)
            }
            ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
                validate_process_ref_type_target(
                    artifact,
                    "send target payload type",
                    ty,
                    *target_process,
                )?;
                let received_payload_type = self
                    .message_variants
                    .get(transition.message.index())
                    .and_then(|message| message.payload_type.as_deref())
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} transition {} send target requires a payload-bearing message",
                            self.debug_name,
                            transition.message.as_u32()
                        ))
                    })?;
                if ty != received_payload_type {
                    return Err(Error::new(format!(
                        "process {} transition {} send target has received payload type {ty}, expected {received_payload_type}",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                Ok(*target_process)
            }
        }
    }

    fn validate_template_process_refs(
        &self,
        artifact: &MantleArtifact,
        template: &ArtifactValueTemplate,
        spawned_refs: &BTreeSet<ProcessRefId>,
    ) -> Result<()> {
        match template {
            ArtifactValueTemplate::Literal { .. }
            | ArtifactValueTemplate::ReceivedPayload { .. } => Ok(()),
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => {
                validate_process_ref_type_target(
                    artifact,
                    "process reference payload type",
                    ty,
                    *target_process,
                )?;
                let declared_target = self.process_ref_target(*process_ref)?;
                if declared_target != *target_process {
                    return Err(Error::new(format!(
                        "process {} process reference payload id {} targets process id {}, expected {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        declared_target.as_u32(),
                        target_process.as_u32()
                    )));
                }
                if !spawned_refs.contains(process_ref) {
                    return Err(Error::new(format!(
                        "process {} sends unbound process reference id {} as payload",
                        self.debug_name,
                        process_ref.as_u32()
                    )));
                }
                Ok(())
            }
            ArtifactValueTemplate::Record { fields, .. } => {
                for field in fields {
                    self.validate_template_process_refs(artifact, &field.value, spawned_refs)?;
                }
                Ok(())
            }
        }
    }

    fn process_ref_target(&self, process_ref: ProcessRefId) -> Result<ProcessId> {
        self.process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references undefined process reference id {}",
                    self.debug_name,
                    process_ref.as_u32()
                ))
            })
    }
}

fn validate_process_ref_type_field(field: &str, value: &str) -> Result<()> {
    validate_type_field(field, value)?;
    if process_ref_type_target(value).is_none() {
        return Err(Error::new(format!(
            "artifact field {field} must be a process reference type, got {value:?}"
        )));
    }
    Ok(())
}

fn validate_process_ref_type_target(
    artifact: &MantleArtifact,
    field: &str,
    value: &str,
    target_process: ProcessId,
) -> Result<()> {
    validate_process_ref_type_field(field, value)?;
    let target_name = process_ref_type_target(value)
        .expect("validate_process_ref_type_field ensures process reference type shape");
    let process = artifact
        .processes
        .get(target_process.index())
        .ok_or_else(|| {
            Error::new(format!(
                "artifact field {field} targets undefined process id {}",
                target_process.as_u32()
            ))
        })?;
    if process.debug_name != target_name {
        return Err(Error::new(format!(
            "artifact field {field} {value} targets {target_name}, expected {}",
            process.debug_name
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcessRef {
    pub debug_name: String,
    pub target: ProcessId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValueTemplate {
    Literal {
        ty: String,
        value: String,
    },
    ReceivedPayload {
        ty: String,
    },
    ProcessRef {
        ty: String,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    },
    Record {
        ty: String,
        fields: Vec<ArtifactValueTemplateField>,
    },
}

impl ArtifactValueTemplate {
    pub fn result_type(&self) -> &str {
        match self {
            Self::Literal { ty, .. }
            | Self::ReceivedPayload { ty }
            | Self::ProcessRef { ty, .. }
            | Self::Record { ty, .. } => ty,
        }
    }

    pub fn evaluate_state_value(
        &self,
        received_payload: Option<&ArtifactPayload>,
    ) -> Result<ArtifactStateValue> {
        match self {
            Self::Literal { ty, value } => Ok(ArtifactStateValue::new(ty.clone(), value.clone())),
            Self::ReceivedPayload { ty } => {
                let payload = received_payload.ok_or_else(|| {
                    Error::new("received payload template requires a payload-bearing message")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "received payload has type {}, expected {}",
                        payload.ty, ty
                    )));
                }
                if payload.process_ref.is_some() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                Ok(ArtifactStateValue::new(
                    payload.ty.clone(),
                    payload.value.clone(),
                ))
            }
            Self::ProcessRef { .. } => Err(Error::new(
                "process reference template requires runtime process reference bindings",
            )),
            Self::Record { ty, fields } => {
                let mut parts = Vec::with_capacity(fields.len());
                let mut labels = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = field.value.evaluate_state_value(received_payload)?;
                    parts.push(format!("{}:{}", field.name, value.value));
                    labels.push(format!("{}:{}", field.name, value.label));
                }
                let value = format!("{ty}{{{}}}", parts.join(","));
                let label = format!("{ty}{{{}}}", labels.join(","));
                validate_value_label("record template value", &value)?;
                validate_value_label("record template label", &label)?;
                Ok(ArtifactStateValue::with_label(ty.clone(), value, label))
            }
        }
    }

    fn depends_on_received_payload(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => true,
            Self::ProcessRef { .. } => false,
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.depends_on_received_payload()),
        }
    }

    fn validate_for_received_payload(
        &self,
        field: &str,
        expected_type: Option<&str>,
        received_payload_type: Option<&str>,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        validate_type_field(&format!("{field}.type"), self.result_type())?;
        if let Some(expected_type) = expected_type {
            if self.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type {}, expected {}",
                    self.result_type(),
                    expected_type
                )));
            }
        }
        match self {
            Self::Literal { value, .. } => validate_value_label(field, value),
            Self::ReceivedPayload { ty } => {
                let Some(received_payload_type) = received_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing transition message"
                    )));
                };
                if ty != received_payload_type {
                    return Err(Error::new(format!(
                        "{field} has received payload type {ty}, expected {received_payload_type}"
                    )));
                }
                Ok(())
            }
            Self::ProcessRef { ty, .. } => {
                if expected_type.is_none() {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                validate_process_ref_type_field(&format!("{field}.type"), ty)
            }
            Self::Record { fields, .. } => {
                validate_count(
                    &format!("{field}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                let mut seen = BTreeSet::new();
                for record_field in fields {
                    validate_ident_field(&format!("{field}.field"), &record_field.name)?;
                    if !seen.insert(record_field.name.as_str()) {
                        return Err(Error::new(format!(
                            "{field} duplicates field {}",
                            record_field.name
                        )));
                    }
                    record_field.value.validate_for_received_payload(
                        &format!("{field}.field.{}", record_field.name),
                        None,
                        received_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValueTemplateField {
    pub name: String,
    pub value: ArtifactValueTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPayload {
    pub ty: String,
    pub value: String,
    pub process_ref: Option<ArtifactProcessRefPayload>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactProcessRefPayload {
    pub target_process: ProcessId,
    pub pid: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTransition {
    pub message: MessageId,
    pub step_result: StepResult,
    pub next_state: NextState,
    pub effects: Vec<ArtifactEffect>,
    pub actions: Vec<ArtifactAction>,
}

impl ArtifactTransition {
    fn validate_effects(&self, process_debug_name: &str) -> Result<BTreeSet<ArtifactEffect>> {
        validate_count(
            "effect_count",
            self.effects.len(),
            0,
            MAX_EFFECTS_PER_TRANSITION,
        )?;
        let mut effects = BTreeSet::new();
        for &effect in &self.effects {
            if !effects.insert(effect) {
                return Err(Error::new(format!(
                    "process {process_debug_name} transition {} declares duplicate effect {effect}",
                    self.message.as_u32()
                )));
            }
        }
        Ok(effects)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
        process_ref: ProcessRefId,
    },
    Send {
        target: ArtifactSendTarget,
        message: MessageId,
        payload: Option<ArtifactValueTemplate>,
    },
}

impl ArtifactAction {
    fn effect(&self) -> ArtifactEffect {
        match self {
            Self::Emit { .. } => ArtifactEffect::Emit,
            Self::Spawn { .. } => ArtifactEffect::Spawn,
            Self::Send { .. } => ArtifactEffect::Send,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSendTarget {
    ProcessRef(ProcessRefId),
    ReceivedPayload {
        ty: String,
        target_process: ProcessId,
    },
}

fn encode_state_value(encoded: &mut String, prefix: &str, state_value: &ArtifactStateValue) {
    encoded.push_str(&format!(
        "{prefix}.type={}\n{prefix}.value={}\n{prefix}.label={}\n",
        state_value.ty, state_value.value, state_value.label
    ));
}

fn decode_state_value(fields: &mut ArtifactFields, prefix: &str) -> Result<ArtifactStateValue> {
    Ok(ArtifactStateValue {
        ty: fields.take_required(&format!("{prefix}.type"))?,
        value: fields.take_required(&format!("{prefix}.value"))?,
        label: fields.take_required(&format!("{prefix}.label"))?,
    })
}

fn encode_value_template(encoded: &mut String, prefix: &str, template: &ArtifactValueTemplate) {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => {
            encoded.push_str(&format!(
                "{prefix}.kind=literal\n{prefix}.type={ty}\n{prefix}.value={value}\n"
            ));
        }
        ArtifactValueTemplate::ReceivedPayload { ty } => {
            encoded.push_str(&format!(
                "{prefix}.kind=received_payload\n{prefix}.type={ty}\n"
            ));
        }
        ArtifactValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=process_ref\n{prefix}.type={ty}\n{prefix}.target_process={}\n{prefix}.process_ref={}\n",
                target_process.as_u32(),
                process_ref.as_u32()
            ));
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            encoded.push_str(&format!(
                "{prefix}.kind=record\n{prefix}.type={ty}\n{prefix}.field_count={}\n",
                fields.len()
            ));
            for (field_index, field) in fields.iter().enumerate() {
                let field_prefix = format!("{prefix}.field.{field_index}");
                encoded.push_str(&format!("{field_prefix}.name={}\n", field.name));
                encode_value_template(encoded, &format!("{field_prefix}.value"), &field.value);
            }
        }
    }
}

fn decode_next_state(fields: &mut ArtifactFields, prefix: &str) -> Result<NextState> {
    let key = format!("{prefix}.next_state");
    match fields.take_required(&key)?.as_str() {
        "current" => Ok(NextState::Current),
        "value" => Ok(NextState::Value(
            fields.take_state_id(&format!("{prefix}.next_state_value"))?,
        )),
        "template" => Ok(NextState::Template(decode_value_template(
            fields,
            &format!("{prefix}.next_state_template"),
            0,
        )?)),
        value => Err(Error::new(format!(
            "invalid {key} value {value:?}; expected \"current\", \"value\", or \"template\""
        ))),
    }
}

fn decode_value_template(
    fields: &mut ArtifactFields,
    prefix: &str,
    depth: usize,
) -> Result<ArtifactValueTemplate> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "{prefix} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
        )));
    }
    let kind_key = format!("{prefix}.kind");
    match fields.take_required(&kind_key)?.as_str() {
        "literal" => Ok(ArtifactValueTemplate::Literal {
            ty: fields.take_required(&format!("{prefix}.type"))?,
            value: fields.take_required(&format!("{prefix}.value"))?,
        }),
        "received_payload" => Ok(ArtifactValueTemplate::ReceivedPayload {
            ty: fields.take_required(&format!("{prefix}.type"))?,
        }),
        "process_ref" => Ok(ArtifactValueTemplate::ProcessRef {
            ty: fields.take_required(&format!("{prefix}.type"))?,
            target_process: fields.take_process_id(&format!("{prefix}.target_process"))?,
            process_ref: fields.take_process_ref_id(&format!("{prefix}.process_ref"))?,
        }),
        "record" => {
            let ty = fields.take_required(&format!("{prefix}.type"))?;
            let field_count = fields.take_bounded_usize(
                &format!("{prefix}.field_count"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut record_fields = Vec::with_capacity(field_count);
            for field_index in 0..field_count {
                let field_prefix = format!("{prefix}.field.{field_index}");
                record_fields.push(ArtifactValueTemplateField {
                    name: fields.take_required(&format!("{field_prefix}.name"))?,
                    value: decode_value_template(
                        fields,
                        &format!("{field_prefix}.value"),
                        depth + 1,
                    )?,
                });
            }
            Ok(ArtifactValueTemplate::Record {
                ty,
                fields: record_fields,
            })
        }
        value => Err(Error::new(format!("invalid {kind_key} value {value:?}"))),
    }
}

fn encode_action(encoded: &mut String, action_prefix: &str, action: &ArtifactAction) {
    match action {
        ArtifactAction::Emit { output } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=emit\n{action_prefix}.output={}\n",
                output.as_u32()
            ));
        }
        ArtifactAction::Spawn {
            target,
            process_ref,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=spawn\n{action_prefix}.target_process={}\n{action_prefix}.process_ref={}\n",
                target.as_u32(),
                process_ref.as_u32()
            ));
        }
        ArtifactAction::Send {
            target,
            message,
            payload,
        } => {
            encoded.push_str(&format!("{action_prefix}.kind=send\n"));
            encode_send_target(encoded, action_prefix, target);
            encoded.push_str(&format!(
                "{action_prefix}.message={}\n{action_prefix}.payload={}\n",
                message.as_u32(),
                if payload.is_some() {
                    "template"
                } else {
                    "none"
                }
            ));
            if let Some(payload) = payload {
                encode_value_template(
                    encoded,
                    &format!("{action_prefix}.payload_template"),
                    payload,
                );
            }
        }
    }
}

fn encode_send_target(encoded: &mut String, action_prefix: &str, target: &ArtifactSendTarget) {
    match target {
        ArtifactSendTarget::ProcessRef(process_ref) => {
            encoded.push_str(&format!(
                "{action_prefix}.target=process_ref\n{action_prefix}.target_process_ref={}\n",
                process_ref.as_u32()
            ));
        }
        ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
            encoded.push_str(&format!(
                "{action_prefix}.target=received_payload\n{action_prefix}.target_payload_type={ty}\n{action_prefix}.target_process={}\n",
                target_process.as_u32()
            ));
        }
    }
}

fn decode_send_target(
    fields: &mut ArtifactFields,
    action_prefix: &str,
) -> Result<ArtifactSendTarget> {
    let key = format!("{action_prefix}.target");
    match fields.take_required(&key)?.as_str() {
        "process_ref" => Ok(ArtifactSendTarget::ProcessRef(
            fields.take_process_ref_id(&format!("{action_prefix}.target_process_ref"))?,
        )),
        "received_payload" => Ok(ArtifactSendTarget::ReceivedPayload {
            ty: fields.take_required(&format!("{action_prefix}.target_payload_type"))?,
            target_process: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
        }),
        value => Err(Error::new(format!("invalid {key} value {value:?}"))),
    }
}

fn decode_action(fields: &mut ArtifactFields, action_prefix: &str) -> Result<ArtifactAction> {
    let kind = fields.take_required(&format!("{action_prefix}.kind"))?;
    match kind.as_str() {
        "emit" => Ok(ArtifactAction::Emit {
            output: fields.take_output_id(&format!("{action_prefix}.output"))?,
        }),
        "spawn" => Ok(ArtifactAction::Spawn {
            target: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
            process_ref: fields.take_process_ref_id(&format!("{action_prefix}.process_ref"))?,
        }),
        "send" => {
            let target = decode_send_target(fields, action_prefix)?;
            let message = fields.take_message_id(&format!("{action_prefix}.message"))?;
            let payload_key = format!("{action_prefix}.payload");
            let payload = match fields.take_required(&payload_key)?.as_str() {
                "none" => None,
                "template" => Some(decode_value_template(
                    fields,
                    &format!("{action_prefix}.payload_template"),
                    0,
                )?),
                value => {
                    return Err(Error::new(format!(
                        "invalid {payload_key} value {value:?}; expected \"none\" or \"template\""
                    )));
                }
            };
            Ok(ArtifactAction::Send {
                target,
                message,
                payload,
            })
        }
        _ => Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
    }
}
