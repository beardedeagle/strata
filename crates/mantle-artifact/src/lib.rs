use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const ARTIFACT_MAGIC: &str = "MTA0";
pub const ARTIFACT_FORMAT: &str = "mantle-target-artifact";
pub const ARTIFACT_VERSION: &str = "1";
pub const STRATA_SOURCE_LANGUAGE: &str = "strata";
pub const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
pub const MAX_ARTIFACT_FIELDS: usize = 16_384;
pub const MAX_FIELD_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_IDENTIFIER_BYTES: usize = 128;
pub const MAX_PROCESS_COUNT: usize = 256;
pub const MAX_STATE_VALUES_PER_PROCESS: usize = 1024;
pub const MAX_MESSAGE_VARIANTS_PER_PROCESS: usize = 1024;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub format_version: String,
    pub source_language: String,
    pub module: String,
    pub entry_process: String,
    pub entry_message: String,
    pub processes: Vec<ArtifactProcess>,
    pub source_hash_fnv1a64: String,
}

impl MantleArtifact {
    pub fn encode(&self) -> String {
        let mut encoded = format!(
            "{ARTIFACT_MAGIC}\nformat={}\nformat_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\nprocess_count={}\n",
            self.format,
            self.format_version,
            self.source_language,
            self.module,
            self.entry_process,
            self.entry_message,
            self.processes.len()
        );

        for (process_index, process) in self.processes.iter().enumerate() {
            let prefix = format!("process.{process_index}");
            encoded.push_str(&format!(
                "{prefix}.name={}\n{prefix}.state_type={}\n{prefix}.state_value_count={}\n",
                process.name,
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
                process.init_state,
                process.step_result.as_str(),
                process.final_state,
                process.actions.len()
            ));
            for (action_index, action) in process.actions.iter().enumerate() {
                let action_prefix = format!("{prefix}.action.{action_index}");
                match action {
                    ArtifactAction::Emit { text } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=emit\n{action_prefix}.text={text}\n"
                        ));
                    }
                    ArtifactAction::Spawn { target } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=spawn\n{action_prefix}.target={target}\n"
                        ));
                    }
                    ArtifactAction::Send { target, message } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=send\n{action_prefix}.target={target}\n{action_prefix}.message={message}\n"
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
                        text: fields.take_required(&format!("{action_prefix}.text"))?,
                    },
                    "spawn" => ArtifactAction::Spawn {
                        target: fields.take_required(&format!("{action_prefix}.target"))?,
                    },
                    "send" => ArtifactAction::Send {
                        target: fields.take_required(&format!("{action_prefix}.target"))?,
                        message: fields.take_required(&format!("{action_prefix}.message"))?,
                    },
                    _ => return Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
                };
                actions.push(action);
            }

            processes.push(ArtifactProcess {
                name: fields.take_required(&format!("{prefix}.name"))?,
                state_type: fields.take_required(&format!("{prefix}.state_type"))?,
                state_values,
                message_type: fields.take_required(&format!("{prefix}.message_type"))?,
                message_variants,
                mailbox_bound: fields.take_bounded_usize(
                    &format!("{prefix}.mailbox_bound"),
                    1,
                    MAX_MAILBOX_BOUND,
                )?,
                init_state: fields.take_required(&format!("{prefix}.init_state"))?,
                step_result: fields.take_step_result(&format!("{prefix}.step_result"))?,
                final_state: fields.take_required(&format!("{prefix}.final_state"))?,
                actions,
            });
        }

        let artifact = Self {
            format: fields.take_required("format")?,
            format_version: fields.take_required("format_version")?,
            source_language: fields.take_required("source_language")?,
            module: fields.take_required("module")?,
            entry_process: fields.take_required("entry_process")?,
            entry_message: fields.take_required("entry_message")?,
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
        validate_ident_field("entry_process", &self.entry_process)?;
        validate_ident_field("entry_message", &self.entry_message)?;
        validate_source_hash(&self.source_hash_fnv1a64)?;
        validate_count("process_count", self.processes.len(), 1, MAX_PROCESS_COUNT)?;

        let mut process_names = BTreeSet::new();
        let mut processes_by_name = BTreeMap::new();
        for process in &self.processes {
            process.validate_identity()?;
            if !process_names.insert(process.name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate process definition {}",
                    process.name
                )));
            }
            processes_by_name.insert(process.name.as_str(), process);
        }

        let Some(entry_process) = processes_by_name.get(self.entry_process.as_str()) else {
            return Err(Error::new(format!(
                "entry process {} is not defined",
                self.entry_process
            )));
        };
        if !entry_process
            .message_variants
            .iter()
            .any(|message| message == &self.entry_message)
        {
            return Err(Error::new(format!(
                "entry message {} is not accepted by {}",
                self.entry_message, self.entry_process
            )));
        }

        for process in &self.processes {
            process.validate_references(&processes_by_name)?;
        }
        validate_encoded_artifact_size(self)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcess {
    pub name: String,
    pub state_type: String,
    pub state_values: Vec<String>,
    pub message_type: String,
    pub message_variants: Vec<String>,
    pub mailbox_bound: usize,
    pub init_state: String,
    pub step_result: StepResult,
    pub final_state: String,
    pub actions: Vec<ArtifactAction>,
}

impl ArtifactProcess {
    fn validate_identity(&self) -> Result<()> {
        validate_ident_field("process name", &self.name)?;
        validate_ident_field("state_type", &self.state_type)?;
        validate_ident_field("message_type", &self.message_type)?;
        validate_ident_field("init_state", &self.init_state)?;
        validate_ident_field("final_state", &self.final_state)?;
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
        if !self
            .state_values
            .iter()
            .any(|value| value == &self.init_state)
        {
            return Err(Error::new(format!(
                "process {} init_state {} is not a valid state value",
                self.name, self.init_state
            )));
        }
        if !self
            .state_values
            .iter()
            .any(|value| value == &self.final_state)
        {
            return Err(Error::new(format!(
                "process {} final_state {} is not a valid state value",
                self.name, self.final_state
            )));
        }
        Ok(())
    }

    fn validate_references(
        &self,
        processes_by_name: &BTreeMap<&str, &ArtifactProcess>,
    ) -> Result<()> {
        for action in &self.actions {
            match action {
                ArtifactAction::Emit { text } => validate_output_text(text)?,
                ArtifactAction::Spawn { target } => {
                    validate_ident_field("spawn target", target)?;
                    if !processes_by_name.contains_key(target.as_str()) {
                        return Err(Error::new(format!(
                            "process {} spawns undefined process {}",
                            self.name, target
                        )));
                    }
                }
                ArtifactAction::Send { target, message } => {
                    validate_ident_field("send target", target)?;
                    validate_ident_field("send message", message)?;
                    let Some(target_process) = processes_by_name.get(target.as_str()) else {
                        return Err(Error::new(format!(
                            "process {} sends to undefined process {}",
                            self.name, target
                        )));
                    };
                    if !target_process
                        .message_variants
                        .iter()
                        .any(|variant| variant == message)
                    {
                        return Err(Error::new(format!(
                            "process {} sends message {} not accepted by {}",
                            self.name, message, target
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
    Emit { text: String },
    Spawn { target: String },
    Send { target: String, message: String },
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
    add_field_bytes(&mut encoded_len, "entry_process", &artifact.entry_process)?;
    add_field_bytes(&mut encoded_len, "entry_message", &artifact.entry_message)?;
    add_field_bytes(
        &mut encoded_len,
        "process_count",
        &artifact.processes.len().to_string(),
    )?;

    for (process_index, process) in artifact.processes.iter().enumerate() {
        let prefix = format!("process.{process_index}");
        add_field_bytes(&mut encoded_len, &format!("{prefix}.name"), &process.name)?;
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
            &process.init_state,
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.step_result"),
            process.step_result.as_str(),
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.final_state"),
            &process.final_state,
        )?;
        add_field_bytes(
            &mut encoded_len,
            &format!("{prefix}.action_count"),
            &process.actions.len().to_string(),
        )?;
        for (action_index, action) in process.actions.iter().enumerate() {
            let action_prefix = format!("{prefix}.action.{action_index}");
            match action {
                ArtifactAction::Emit { text } => {
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.kind"), "emit")?;
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.text"), text)?;
                }
                ArtifactAction::Spawn { target } => {
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.kind"), "spawn")?;
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.target"), target)?;
                }
                ArtifactAction::Send { target, message } => {
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.kind"), "send")?;
                    add_field_bytes(&mut encoded_len, &format!("{action_prefix}.target"), target)?;
                    add_field_bytes(
                        &mut encoded_len,
                        &format!("{action_prefix}.message"),
                        message,
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

        let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
        assert!(err.to_string().contains("invalid Mantle artifact magic"));
    }

    #[test]
    fn decode_reports_duplicate_fields() {
        let encoded = valid_artifact().encode().replace(
            "process.0.name=Main",
            "process.0.name=Main\nprocess.0.name=Other",
        );

        let err = MantleArtifact::decode(&encoded).expect_err("duplicate field should fail");

        assert!(err
            .to_string()
            .contains("duplicate artifact field \"process.0.name\""));
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
        artifact.processes[1].actions = (0..70)
            .map(|_| ArtifactAction::Emit { text: text.clone() })
            .collect();

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
            target: "Worker".to_string(),
            message: "Unknown".to_string(),
        });

        let err = artifact
            .validate()
            .expect_err("unknown send message should fail");

        assert!(err
            .to_string()
            .contains("sends message Unknown not accepted by Worker"));
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
            entry_process: "Main".to_string(),
            entry_message: "Start".to_string(),
            processes: vec![
                ArtifactProcess {
                    name: "Main".to_string(),
                    state_type: "MainState".to_string(),
                    state_values: vec!["MainState".to_string()],
                    message_type: "MainMsg".to_string(),
                    message_variants: vec!["Start".to_string()],
                    mailbox_bound: 1,
                    init_state: "MainState".to_string(),
                    step_result: StepResult::Stop,
                    final_state: "MainState".to_string(),
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: "Worker".to_string(),
                        },
                        ArtifactAction::Send {
                            target: "Worker".to_string(),
                            message: "Ping".to_string(),
                        },
                    ],
                },
                ArtifactProcess {
                    name: "Worker".to_string(),
                    state_type: "WorkerState".to_string(),
                    state_values: vec!["Idle".to_string(), "Handled".to_string()],
                    message_type: "WorkerMsg".to_string(),
                    message_variants: vec!["Ping".to_string()],
                    mailbox_bound: 1,
                    init_state: "Idle".to_string(),
                    step_result: StepResult::Stop,
                    final_state: "Handled".to_string(),
                    actions: vec![ArtifactAction::Emit {
                        text: "worker handled Ping".to_string(),
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
