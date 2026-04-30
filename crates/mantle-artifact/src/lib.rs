use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const ARTIFACT_MAGIC: &str = "MTA0";
pub const ARTIFACT_FORMAT: &str = "mantle-target-artifact";
pub const ARTIFACT_VERSION: &str = "2";
pub const STRATA_SOURCE_LANGUAGE: &str = "strata";
pub const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
pub const MAX_ARTIFACT_FIELDS: usize = 16_384;
pub const MAX_FIELD_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_IDENTIFIER_BYTES: usize = 128;
pub const MAX_PROCESS_COUNT: usize = 256;
pub const MAX_STATE_VALUES_PER_PROCESS: usize = 1024;
pub const MAX_MESSAGE_VARIANTS_PER_PROCESS: usize = 1024;
pub const MAX_OUTPUT_LITERALS: usize = 4096;
pub const MAX_ACTIONS_PER_PROCESS: usize = 4096;
pub const MAX_MAILBOX_BOUND: usize = 65_536;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Stop,
}

impl StepResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::Stop => "Stop",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "Continue" => Ok(Self::Continue),
            "Stop" => Ok(Self::Stop),
            _ => Err(Error::new(format!("invalid step_result value {value:?}"))),
        }
    }
}

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            pub fn from_index(index: usize) -> Result<Self> {
                let value = u32::try_from(index).map_err(|_| {
                    Error::new(format!("{} index {index} is too large", stringify!($name)))
                })?;
                Ok(Self(value))
            }

            pub const fn as_u32(self) -> u32 {
                self.0
            }

            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

define_id!(ProcessId);
define_id!(StateId);
define_id!(MessageId);
define_id!(OutputId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub format_version: String,
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
            "{ARTIFACT_MAGIC}\nformat={}\nformat_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\noutput_count={}\nprocess_count={}\n",
            self.format,
            self.format_version,
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
                encoded.push_str(&format!("{prefix}.state_value.{value_index}={value}\n"));
            }
            encoded.push_str(&format!(
                "{prefix}.message_type={}\n{prefix}.message_count={}\n",
                process.message_type,
                process.message_variants.len()
            ));
            for (message_index, message) in process.message_variants.iter().enumerate() {
                encoded.push_str(&format!("{prefix}.message.{message_index}={message}\n"));
            }
            encoded.push_str(&format!(
                "{prefix}.mailbox_bound={}\n{prefix}.init_state={}\n{prefix}.step_result={}\n{prefix}.final_state={}\n{prefix}.action_count={}\n",
                process.mailbox_bound,
                process.init_state.as_u32(),
                process.step_result.as_str(),
                process.final_state.as_u32(),
                process.actions.len()
            ));
            for (action_index, action) in process.actions.iter().enumerate() {
                let action_prefix = format!("{prefix}.action.{action_index}");
                match action {
                    ArtifactAction::Emit { output } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=emit\n{action_prefix}.output={}\n",
                            output.as_u32()
                        ));
                    }
                    ArtifactAction::Spawn { target } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=spawn\n{action_prefix}.target_process={}\n",
                            target.as_u32()
                        ));
                    }
                    ArtifactAction::Send { target, message } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=send\n{action_prefix}.target_process={}\n{action_prefix}.message={}\n",
                            target.as_u32(),
                            message.as_u32()
                        ));
                    }
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
                state_values
                    .push(fields.take_required(&format!("{prefix}.state_value.{value_index}"))?);
            }

            let message_count = fields.take_bounded_usize(
                &format!("{prefix}.message_count"),
                1,
                MAX_MESSAGE_VARIANTS_PER_PROCESS,
            )?;
            let mut message_variants = Vec::with_capacity(message_count);
            for message_index in 0..message_count {
                message_variants
                    .push(fields.take_required(&format!("{prefix}.message.{message_index}"))?);
            }

            let action_count = fields.take_bounded_usize(
                &format!("{prefix}.action_count"),
                0,
                MAX_ACTIONS_PER_PROCESS,
            )?;
            let mut actions = Vec::with_capacity(action_count);
            for action_index in 0..action_count {
                let action_prefix = format!("{prefix}.action.{action_index}");
                let kind = fields.take_required(&format!("{action_prefix}.kind"))?;
                let action = match kind.as_str() {
                    "emit" => ArtifactAction::Emit {
                        output: fields.take_output_id(&format!("{action_prefix}.output"))?,
                    },
                    "spawn" => ArtifactAction::Spawn {
                        target: fields
                            .take_process_id(&format!("{action_prefix}.target_process"))?,
                    },
                    "send" => ArtifactAction::Send {
                        target: fields
                            .take_process_id(&format!("{action_prefix}.target_process"))?,
                        message: fields.take_message_id(&format!("{action_prefix}.message"))?,
                    },
                    _ => return Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
                };
                actions.push(action);
            }

            processes.push(ArtifactProcess {
                debug_name: fields.take_required(&format!("{prefix}.debug_name"))?,
                state_type: fields.take_required(&format!("{prefix}.state_type"))?,
                state_values,
                message_type: fields.take_required(&format!("{prefix}.message_type"))?,
                message_variants,
                mailbox_bound: fields.take_bounded_usize(
                    &format!("{prefix}.mailbox_bound"),
                    1,
                    MAX_MAILBOX_BOUND,
                )?,
                init_state: fields.take_state_id(&format!("{prefix}.init_state"))?,
                step_result: fields.take_step_result(&format!("{prefix}.step_result"))?,
                final_state: fields.take_state_id(&format!("{prefix}.final_state"))?,
                actions,
            });
        }

        let artifact = Self {
            format: fields.take_required("format")?,
            format_version: fields.take_required("format_version")?,
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
        if self.format != ARTIFACT_FORMAT {
            return Err(Error::new(format!(
                "unsupported artifact format {}; expected {}",
                self.format, ARTIFACT_FORMAT
            )));
        }
        if self.format_version != ARTIFACT_VERSION {
            return Err(Error::new(format!(
                "unsupported artifact version {}; expected {}",
                self.format_version, ARTIFACT_VERSION
            )));
        }
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

        for process in &self.processes {
            process.validate_references(self)?;
        }
        validate_encoded_artifact_size(self)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcess {
    pub debug_name: String,
    pub state_type: String,
    pub state_values: Vec<String>,
    pub message_type: String,
    pub message_variants: Vec<String>,
    pub mailbox_bound: usize,
    pub init_state: StateId,
    pub step_result: StepResult,
    pub final_state: StateId,
    pub actions: Vec<ArtifactAction>,
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
            "action_count",
            self.actions.len(),
            0,
            MAX_ACTIONS_PER_PROCESS,
        )?;
        validate_unique_ident_list("state value", &self.state_values)?;
        validate_unique_ident_list("message variant", &self.message_variants)?;
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a valid state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.final_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} final_state id {} is not a valid state value",
                self.debug_name,
                self.final_state.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_references(&self, artifact: &MantleArtifact) -> Result<()> {
        for action in &self.actions {
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
                ArtifactAction::Spawn { target } => {
                    if target.index() >= artifact.processes.len() {
                        return Err(Error::new(format!(
                            "process {} spawns undefined process id {}",
                            self.debug_name,
                            target.as_u32()
                        )));
                    }
                }
                ArtifactAction::Send { target, message } => {
                    let Some(target_process) = artifact.processes.get(target.index()) else {
                        return Err(Error::new(format!(
                            "process {} sends to undefined process id {}",
                            self.debug_name,
                            target.as_u32()
                        )));
                    };
                    if message.index() >= target_process.message_variants.len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by process id {}",
                            self.debug_name,
                            message.as_u32(),
                            target.as_u32()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
    },
    Send {
        target: ProcessId,
        message: MessageId,
    },
}

struct ArtifactFields {
    fields: BTreeMap<String, String>,
}

impl ArtifactFields {
    fn parse(contents: &str) -> Result<Self> {
        if contents.len() > MAX_ARTIFACT_BYTES {
            return Err(Error::new(format!(
                "artifact is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes"
            )));
        }

        let mut lines = contents.lines();
        match lines.next() {
            Some(ARTIFACT_MAGIC) => {}
            Some(other) => {
                return Err(Error::new(format!(
                    "invalid Mantle artifact magic {other:?}; expected {ARTIFACT_MAGIC:?}"
                )));
            }
            None => return Err(Error::new("empty Mantle artifact")),
        }

        let mut fields = BTreeMap::new();
        for line in lines {
            if fields.len() >= MAX_ARTIFACT_FIELDS {
                return Err(Error::new(format!(
                    "artifact declares too many fields; maximum supported count is {MAX_ARTIFACT_FIELDS}"
                )));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(Error::new(format!("invalid artifact field line {line:?}")));
            };
            if value.len() > MAX_FIELD_VALUE_BYTES {
                return Err(Error::new(format!(
                    "artifact field {key:?} exceeds maximum value length of {MAX_FIELD_VALUE_BYTES} bytes"
                )));
            }
            if fields.insert(key.to_string(), value.to_string()).is_some() {
                return Err(Error::new(format!("duplicate artifact field {key:?}")));
            }
        }
        Ok(Self { fields })
    }

    fn take_required(&mut self, key: &str) -> Result<String> {
        self.fields
            .remove(key)
            .ok_or_else(|| Error::new(format!("missing artifact field {key}")))
    }

    fn take_usize(&mut self, key: &str) -> Result<usize> {
        let value = self.take_required(key)?;
        value
            .parse::<usize>()
            .map_err(|_| Error::new(format!("invalid {key} value {value:?}")))
    }

    fn take_bounded_usize(&mut self, key: &str, min: usize, max: usize) -> Result<usize> {
        let value = self.take_usize(key)?;
        validate_count(key, value, min, max)?;
        Ok(value)
    }

    fn take_process_id(&mut self, key: &str) -> Result<ProcessId> {
        ProcessId::from_index(self.take_bounded_usize(key, 0, MAX_PROCESS_COUNT - 1)?)
    }

    fn take_state_id(&mut self, key: &str) -> Result<StateId> {
        StateId::from_index(self.take_bounded_usize(key, 0, MAX_STATE_VALUES_PER_PROCESS - 1)?)
    }

    fn take_message_id(&mut self, key: &str) -> Result<MessageId> {
        MessageId::from_index(self.take_bounded_usize(
            key,
            0,
            MAX_MESSAGE_VARIANTS_PER_PROCESS - 1,
        )?)
    }

    fn take_output_id(&mut self, key: &str) -> Result<OutputId> {
        OutputId::from_index(self.take_bounded_usize(key, 0, MAX_OUTPUT_LITERALS - 1)?)
    }

    fn take_step_result(&mut self, key: &str) -> Result<StepResult> {
        StepResult::parse(&self.take_required(key)?)
    }

    fn finish(self) -> Result<()> {
        if let Some(key) = self.fields.keys().next() {
            return Err(Error::new(format!("unknown artifact field {key:?}")));
        }
        Ok(())
    }
}

pub fn default_artifact_path(source_path: &Path) -> Result<PathBuf> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Error::new(format!(
                "source path {} has no UTF-8 file stem",
                source_path.display()
            ))
        })?;
    Ok(Path::new("target")
        .join("strata")
        .join(format!("{stem}.mta")))
}

pub fn write_artifact(path: &Path, artifact: &MantleArtifact) -> Result<()> {
    artifact.validate()?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, artifact.encode())?;
    Ok(())
}

pub fn read_artifact(path: &Path) -> Result<MantleArtifact> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_ARTIFACT_BYTES as u64 {
        return Err(Error::new(format!(
            "artifact {} is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes",
            path.display()
        )));
    }
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_ARTIFACT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "artifact {} is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes",
            path.display()
        )));
    }
    let contents = String::from_utf8(bytes).map_err(|err| {
        Error::new(format!(
            "artifact {} is not valid UTF-8: {err}",
            path.display()
        ))
    })?;
    MantleArtifact::decode(&contents)
}

pub fn source_hash_fnv1a64(source: &str) -> String {
    format!("{:016x}", fnv1a64(source.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn validate_ident_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "artifact field {field} exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if is_artifact_ident(value) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "artifact field {field} must be an identifier, got {value:?}"
        )))
    }
}

fn validate_unique_ident_list(label: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        return Err(Error::new(format!("{label} list must not be empty")));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_ident_field(label, value)?;
        if !seen.insert(value.as_str()) {
            return Err(Error::new(format!("duplicate {label} {value}")));
        }
    }
    Ok(())
}

fn validate_output_text(output: &str) -> Result<()> {
    if output.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "emitted output exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if output.is_empty() || output.chars().any(char::is_control) {
        return Err(Error::new(
            "emitted outputs must be non-empty and contain no control characters",
        ));
    }
    Ok(())
}

fn validate_encoded_artifact_size(artifact: &MantleArtifact) -> Result<()> {
    let mut encoded_len = 0usize;
    add_encoded_bytes(&mut encoded_len, ARTIFACT_MAGIC.len() + 1)?;
    add_field_bytes(&mut encoded_len, "format", &artifact.format)?;
    add_field_bytes(&mut encoded_len, "format_version", &artifact.format_version)?;
    add_field_bytes(
        &mut encoded_len,
        "source_language",
        &artifact.source_language,
    )?;
    add_field_bytes(&mut encoded_len, "module", &artifact.module)?;
    add_field_bytes(
        &mut encoded_len,
        "entry_process",
        &artifact.entry_process.as_u32().to_string(),
    )?;
    add_field_bytes(
        &mut encoded_len,
        "entry_message",
        &artifact.entry_message.as_u32().to_string(),
    )?;
    add_field_bytes(
        &mut encoded_len,
        "output_count",
        &artifact.outputs.len().to_string(),
    )?;
    for (output_index, output) in artifact.outputs.iter().enumerate() {
        add_field_bytes(&mut encoded_len, &format!("output.{output_index}"), output)?;
    }
    add_field_bytes(
        &mut encoded_len,
        "process_count",
        &artifact.processes.len().to_string(),
    )?;

    for (process_index, process) in artifact.processes.iter().enumerate() {
        let prefix = format!("process.{process_index}");
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.debug_name"),
            &process.debug_name,
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.state_type"),
            &process.state_type,
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.state_value_count"),
            &process.state_values.len().to_string(),
        )?;
        for (value_index, value) in process.state_values.iter().enumerate() {
            add_field_bytes(
                &mut encoded_len,
                &format!("{prefix}.state_value.{value_index}"),
                value,
            )?;
        }
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.message_type"),
            &process.message_type,
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.message_count"),
            &process.message_variants.len().to_string(),
        )?;
        for (message_index, message) in process.message_variants.iter().enumerate() {
            add_field_bytes(
                &mut encoded_len,
                &format!("{prefix}.message.{message_index}"),
                message,
            )?;
        }
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.mailbox_bound"),
            &process.mailbox_bound.to_string(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.init_state"),
            &process.init_state.as_u32().to_string(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.step_result"),
            process.step_result.as_str(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.final_state"),
            &process.final_state.as_u32().to_string(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.action_count"),
            &process.actions.len().to_string(),
        )?;
        for (action_index, action) in process.actions.iter().enumerate() {
            let action_prefix = format!("{prefix}.action.{action_index}");
            match action {
                ArtifactAction::Emit { output } => {
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.kind"), "emit")?;
                    add_field_bytes(
                        &mut encoded_len,
                        &format!("{action_prefix}.output"),
                        &output.as_u32().to_string(),
                    )?;
                }
                ArtifactAction::Spawn { target } => {
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.kind"), "spawn")?;
                    add_field_bytes(
                        &mut encoded_len,
                        &format!("{action_prefix}.target_process"),
                        &target.as_u32().to_string(),
                    )?;
                }
                ArtifactAction::Send { target, message } => {
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.kind"), "send")?;
                    add_field_bytes(
                        &mut encoded_len,
                        &format!("{action_prefix}.target_process"),
                        &target.as_u32().to_string(),
                    )?;
                    add_field_bytes(
                        &mut encoded_len,
                        &format!("{action_prefix}.message"),
                        &message.as_u32().to_string(),
                    )?;
                }
            }
        }
    }

    add_field_bytes(
        &mut encoded_len,
        "source_hash_fnv1a64",
        &artifact.source_hash_fnv1a64,
    )?;
    Ok(())
}

fn add_field_bytes(total: &mut usize, key: &str, value: &str) -> Result<()> {
    add_encoded_bytes(total, key.len())?;
    add_encoded_bytes(total, 1)?;
    add_encoded_bytes(total, value.len())?;
    add_encoded_bytes(total, 1)
}

fn add_encoded_bytes(total: &mut usize, count: usize) -> Result<()> {
    *total = total
        .checked_add(count)
        .ok_or_else(|| Error::new("encoded artifact size overflowed"))?;
    if *total > MAX_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "encoded artifact exceeds maximum size of {MAX_ARTIFACT_BYTES} bytes"
        )));
    }
    Ok(())
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

fn validate_source_hash(value: &str) -> Result<()> {
    if value.len() != 16 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(Error::new(
            "source_hash_fnv1a64 must be 16 hexadecimal digits",
        ));
    }
    Ok(())
}

fn is_artifact_ident(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn artifact_round_trips_and_validates_magic() {
        let artifact = valid_artifact();
        let encoded = artifact.encode();
        let decoded = MantleArtifact::decode(&encoded).expect("artifact should decode");

        assert_eq!(decoded, artifact);
        assert!(encoded.contains("entry_process=0"));
        assert!(encoded.contains("process.0.action.0.target_process=1"));

        let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
        assert!(err.to_string().contains("invalid Mantle artifact magic"));
    }

    #[test]
    fn decode_reports_duplicate_fields() {
        let encoded = valid_artifact().encode().replace(
            "process.0.debug_name=Main",
            "process.0.debug_name=Main\nprocess.0.debug_name=Other",
        );

        let err = MantleArtifact::decode(&encoded).expect_err("duplicate field should fail");

        assert!(err
            .to_string()
            .contains("duplicate artifact field \"process.0.debug_name\""));
    }

    #[test]
    fn decode_reports_unknown_fields() {
        let mut encoded = valid_artifact().encode();
        encoded.push_str("process.0.action.0.extra=value\n");

        let err = MantleArtifact::decode(&encoded).expect_err("unknown field should fail");

        assert!(err
            .to_string()
            .contains("unknown artifact field \"process.0.action.0.extra\""));
    }

    #[test]
    fn decode_rejects_unbounded_process_count_before_allocation() {
        let encoded = format!("MTA0\nprocess_count={}\n", MAX_PROCESS_COUNT + 1);

        let err = MantleArtifact::decode(&encoded).expect_err("process count should be bounded");

        assert!(err
            .to_string()
            .contains("process_count must be no greater than"));
    }

    #[test]
    fn decode_rejects_unbounded_nested_counts_before_allocation() {
        let encoded = valid_artifact().encode().replace(
            "process.0.state_value_count=1",
            &format!(
                "process.0.state_value_count={}",
                MAX_STATE_VALUES_PER_PROCESS + 1
            ),
        );

        let err =
            MantleArtifact::decode(&encoded).expect_err("state value count should be bounded");

        assert!(err
            .to_string()
            .contains("process.0.state_value_count must be no greater than"));
    }

    #[test]
    fn validate_accepts_non_strata_source_language() {
        let mut artifact = valid_artifact();
        artifact.source_language = "lattice".to_string();

        artifact
            .validate()
            .expect("artifact source language should be language-neutral");

        let decoded = MantleArtifact::decode(&artifact.encode())
            .expect("language-neutral artifact should decode");
        assert_eq!(decoded.source_language, "lattice");
    }

    #[test]
    fn validate_rejects_invalid_source_language_identifier() {
        let mut artifact = valid_artifact();
        artifact.source_language = "not-valid".to_string();

        let err = artifact
            .validate()
            .expect_err("invalid source language should fail");

        assert!(err
            .to_string()
            .contains("artifact field source_language must be an identifier"));
    }

    #[test]
    fn validate_rejects_encoded_artifacts_above_size_limit() {
        let mut artifact = valid_artifact();
        let text = "a".repeat(MAX_FIELD_VALUE_BYTES);
        artifact.outputs = (0..70).map(|_| text.clone()).collect();

        let err = artifact
            .validate()
            .expect_err("encoded artifact size should be bounded");

        assert!(err
            .to_string()
            .contains("encoded artifact exceeds maximum size"));
    }

    #[test]
    fn validate_rejects_unknown_send_message() {
        let mut artifact = valid_artifact();
        artifact.processes[0].actions.push(ArtifactAction::Send {
            target: ProcessId::new(1),
            message: MessageId::new(1),
        });

        let err = artifact
            .validate()
            .expect_err("unknown send message should fail");

        assert!(err
            .to_string()
            .contains("sends message id 1 not accepted by process id 1"));
    }

    #[test]
    fn validate_rejects_unknown_spawn_target() {
        let mut artifact = valid_artifact();
        artifact.processes[0].actions.push(ArtifactAction::Spawn {
            target: ProcessId::new(99),
        });

        let err = artifact
            .validate()
            .expect_err("unknown spawn target should fail");

        assert!(err.to_string().contains("spawns undefined process id 99"));
    }

    #[test]
    fn validate_rejects_unknown_output_id() {
        let mut artifact = valid_artifact();
        artifact.processes[1].actions = vec![ArtifactAction::Emit {
            output: OutputId::new(99),
        }];

        let err = artifact
            .validate()
            .expect_err("unknown output id should fail");

        assert!(err.to_string().contains("emits undefined output id 99"));
    }

    #[test]
    fn validate_rejects_unknown_entry_process_id() {
        let mut artifact = valid_artifact();
        artifact.entry_process = ProcessId::new(99);

        let err = artifact
            .validate()
            .expect_err("unknown entry process id should fail");

        assert!(err
            .to_string()
            .contains("entry process id 99 is not defined"));
    }

    #[test]
    fn validate_rejects_duplicate_process_debug_names() {
        let mut artifact = valid_artifact();
        artifact.processes[1].debug_name = "Main".to_string();

        let err = artifact
            .validate()
            .expect_err("duplicate debug labels should fail");

        assert!(err
            .to_string()
            .contains("duplicate process debug_name Main"));
    }

    #[test]
    fn validate_treats_debug_names_as_metadata_not_targets() {
        let mut artifact = valid_artifact();
        artifact.processes[1].debug_name = "RenamedWorker".to_string();

        artifact
            .validate()
            .expect("renaming debug metadata should not affect indexed references");
    }

    #[test]
    fn write_artifact_rejects_invalid_artifacts_before_writing() {
        let dir = unique_test_dir("invalid-artifact-write");
        let path = dir.join("bad.mta");
        let mut artifact = valid_artifact();
        artifact.format = "invalid-format".to_string();

        let err = write_artifact(&path, &artifact).expect_err("invalid artifact should fail");

        assert!(err.to_string().contains("unsupported artifact format"));
        assert!(!path.exists(), "invalid artifact must not be written");
        assert!(
            !dir.exists(),
            "invalid artifact must not create parent dirs"
        );
    }

    #[test]
    fn write_artifact_accepts_current_directory_output_path() {
        let path = unique_current_dir_artifact_path("artifact-current-dir");
        let artifact = valid_artifact();

        write_artifact(&path, &artifact).expect("current-directory artifact write should succeed");

        let decoded = read_artifact(&path).expect("written artifact should decode");
        assert_eq!(decoded, artifact);

        fs::remove_file(path).expect("test artifact should be removed");
    }

    #[test]
    fn read_artifact_rejects_oversized_file() {
        let path = unique_current_dir_artifact_path("artifact-too-large");
        fs::write(&path, vec![b'a'; MAX_ARTIFACT_BYTES + 1])
            .expect("oversized test file should be written");

        let err = read_artifact(&path).expect_err("oversized artifact file should fail");

        assert!(err.to_string().contains("is too large"));

        fs::remove_file(path).expect("test artifact should be removed");
    }

    fn valid_artifact() -> MantleArtifact {
        MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            format_version: ARTIFACT_VERSION.to_string(),
            source_language: STRATA_SOURCE_LANGUAGE.to_string(),
            module: "actor_ping".to_string(),
            entry_process: ProcessId::new(0),
            entry_message: MessageId::new(0),
            outputs: vec!["worker handled Ping".to_string()],
            processes: vec![
                ArtifactProcess {
                    debug_name: "Main".to_string(),
                    state_type: "MainState".to_string(),
                    state_values: vec!["MainState".to_string()],
                    message_type: "MainMsg".to_string(),
                    message_variants: vec!["Start".to_string()],
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    step_result: StepResult::Stop,
                    final_state: StateId::new(0),
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                        },
                        ArtifactAction::Send {
                            target: ProcessId::new(1),
                            message: MessageId::new(0),
                        },
                    ],
                },
                ArtifactProcess {
                    debug_name: "Worker".to_string(),
                    state_type: "WorkerState".to_string(),
                    state_values: vec!["Idle".to_string(), "Handled".to_string()],
                    message_type: "WorkerMsg".to_string(),
                    message_variants: vec!["Ping".to_string()],
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    step_result: StepResult::Stop,
                    final_state: StateId::new(1),
                    actions: vec![ArtifactAction::Emit {
                        output: OutputId::new(0),
                    }],
                },
            ],
            source_hash_fnv1a64: "0000000000000000".to_string(),
        }
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(unique_artifact_name(name))
    }

    fn unique_current_dir_artifact_path(name: &str) -> PathBuf {
        PathBuf::from(unique_artifact_name(name))
    }

    fn unique_artifact_name(name: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        format!("strata-{name}-{}-{nanos}.mta", std::process::id())
    }
}
