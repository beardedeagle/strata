use std::fs;
use std::path::{Path, PathBuf};

use crate::language::{CheckedProgram, StepResult};
use crate::{Error, Result};

pub const ARTIFACT_MAGIC: &str = "MTA0";
pub const ARTIFACT_FORMAT: &str = "mantle-target-artifact";
pub const ARTIFACT_VERSION: &str = "0";
pub const STRATA_SOURCE_LANGUAGE: &str = "strata";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub format_version: String,
    pub source_language: String,
    pub module: String,
    pub entry_process: String,
    pub state_type: String,
    pub message_type: String,
    pub message_variant: String,
    pub mailbox_bound: usize,
    pub init_state: String,
    pub step_result: StepResult,
    pub emitted_outputs: Vec<String>,
    pub source_hash_fnv1a64: String,
}

impl MantleArtifact {
    pub fn from_checked(checked: &CheckedProgram, source: &str) -> Result<Self> {
        let process = checked
            .module
            .processes
            .iter()
            .find(|process| process.name == checked.entry_process)
            .ok_or_else(|| Error::new("checked entry process is missing from module"))?;

        Ok(Self {
            format: ARTIFACT_FORMAT.to_string(),
            format_version: ARTIFACT_VERSION.to_string(),
            source_language: STRATA_SOURCE_LANGUAGE.to_string(),
            module: checked.module.name.clone(),
            entry_process: checked.entry_process.clone(),
            state_type: process.state_type.clone(),
            message_type: process.msg_type.clone(),
            message_variant: checked.message_variant.clone(),
            mailbox_bound: process.mailbox_bound,
            init_state: checked.init_state.clone(),
            step_result: checked.step_result,
            emitted_outputs: checked.emitted_outputs.clone(),
            source_hash_fnv1a64: format!("{:016x}", fnv1a64(source.as_bytes())),
        })
    }

    pub fn encode(&self) -> String {
        let step_result = match self.step_result {
            StepResult::Continue => "Continue",
            StepResult::Stop => "Stop",
        };

        let mut encoded = format!(
            "{ARTIFACT_MAGIC}\nformat={}\nformat_version={}\nsource_language={}\nmodule={}\nentry_process={}\nstate_type={}\nmessage_type={}\nmessage_variant={}\nmailbox_bound={}\ninit_state={}\nstep_result={}\nemitted_output_count={}\n",
            self.format,
            self.format_version,
            self.source_language,
            self.module,
            self.entry_process,
            self.state_type,
            self.message_type,
            self.message_variant,
            self.mailbox_bound,
            self.init_state,
            step_result,
            self.emitted_outputs.len()
        );
        for (index, output) in self.emitted_outputs.iter().enumerate() {
            encoded.push_str(&format!("emitted_output_{index}={output}\n"));
        }
        encoded.push_str(&format!(
            "source_hash_fnv1a64={}\n",
            self.source_hash_fnv1a64
        ));
        encoded
    }

    pub fn decode(contents: &str) -> Result<Self> {
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

        let mut fields = ArtifactFields::default();
        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                return Err(Error::new(format!("invalid artifact field line {line:?}")));
            };
            match key {
                "format" => fields.format = Some(value.to_string()),
                "format_version" => fields.format_version = Some(value.to_string()),
                "source_language" => fields.source_language = Some(value.to_string()),
                "module" => fields.module = Some(value.to_string()),
                "entry_process" => fields.entry_process = Some(value.to_string()),
                "state_type" => fields.state_type = Some(value.to_string()),
                "message_type" => fields.message_type = Some(value.to_string()),
                "message_variant" => fields.message_variant = Some(value.to_string()),
                "mailbox_bound" => {
                    fields.mailbox_bound = Some(value.parse::<usize>().map_err(|_| {
                        Error::new(format!("invalid mailbox_bound value {value:?}"))
                    })?)
                }
                "init_state" => fields.init_state = Some(value.to_string()),
                "step_result" => {
                    fields.step_result = Some(match value {
                        "Continue" => StepResult::Continue,
                        "Stop" => StepResult::Stop,
                        _ => {
                            return Err(Error::new(format!("invalid step_result value {value:?}")))
                        }
                    })
                }
                "emitted_output_count" => {
                    fields.emitted_output_count = Some(value.parse::<usize>().map_err(|_| {
                        Error::new(format!("invalid emitted_output_count value {value:?}"))
                    })?)
                }
                key if key.starts_with("emitted_output_") => {
                    let index = key.trim_start_matches("emitted_output_");
                    let index = index.parse::<usize>().map_err(|_| {
                        Error::new(format!("invalid emitted output index {index:?}"))
                    })?;
                    fields.emitted_outputs.push((index, value.to_string()));
                }
                "source_hash_fnv1a64" => fields.source_hash_fnv1a64 = Some(value.to_string()),
                _ => return Err(Error::new(format!("unknown artifact field {key:?}"))),
            }
        }

        let emitted_outputs = fields.emitted_outputs()?;

        let artifact = MantleArtifact {
            format: fields.required("format")?,
            format_version: fields.required("format_version")?,
            source_language: fields.required("source_language")?,
            module: fields.required("module")?,
            entry_process: fields.required("entry_process")?,
            state_type: fields.required("state_type")?,
            message_type: fields.required("message_type")?,
            message_variant: fields.required("message_variant")?,
            mailbox_bound: fields
                .mailbox_bound
                .ok_or_else(|| Error::new("missing artifact field mailbox_bound"))?,
            init_state: fields.required("init_state")?,
            step_result: fields
                .step_result
                .ok_or_else(|| Error::new("missing artifact field step_result"))?,
            emitted_outputs,
            source_hash_fnv1a64: fields.required("source_hash_fnv1a64")?,
        };

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
        if self.source_language != STRATA_SOURCE_LANGUAGE {
            return Err(Error::new(format!(
                "unsupported source language {}; expected {}",
                self.source_language, STRATA_SOURCE_LANGUAGE
            )));
        }
        if self.mailbox_bound == 0 {
            return Err(Error::new("mailbox_bound must be greater than zero"));
        }
        for (field, value) in [
            ("module", &self.module),
            ("entry_process", &self.entry_process),
            ("state_type", &self.state_type),
            ("message_type", &self.message_type),
            ("message_variant", &self.message_variant),
            ("init_state", &self.init_state),
        ] {
            if !is_artifact_ident(value) {
                return Err(Error::new(format!(
                    "artifact field {field} must be an identifier, got {value:?}"
                )));
            }
        }
        for output in &self.emitted_outputs {
            if output.is_empty() || output.chars().any(char::is_control) {
                return Err(Error::new(
                    "emitted outputs must be non-empty and contain no control characters",
                ));
            }
        }
        if self.source_hash_fnv1a64.len() != 16
            || !self
                .source_hash_fnv1a64
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(Error::new(
                "source_hash_fnv1a64 must be 16 hexadecimal digits",
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct ArtifactFields {
    format: Option<String>,
    format_version: Option<String>,
    source_language: Option<String>,
    module: Option<String>,
    entry_process: Option<String>,
    state_type: Option<String>,
    message_type: Option<String>,
    message_variant: Option<String>,
    mailbox_bound: Option<usize>,
    init_state: Option<String>,
    step_result: Option<StepResult>,
    emitted_output_count: Option<usize>,
    emitted_outputs: Vec<(usize, String)>,
    source_hash_fnv1a64: Option<String>,
}

impl ArtifactFields {
    fn emitted_outputs(&mut self) -> Result<Vec<String>> {
        let expected = self
            .emitted_output_count
            .ok_or_else(|| Error::new("missing artifact field emitted_output_count"))?;
        self.emitted_outputs.sort_by_key(|(index, _)| *index);
        let mut previous_index = None;
        for (actual_index, _) in &self.emitted_outputs {
            if previous_index == Some(*actual_index) {
                return Err(Error::new(format!(
                    "duplicate emitted output index {actual_index}"
                )));
            }
            if *actual_index >= expected {
                return Err(Error::new(format!(
                    "unexpected emitted output index {actual_index}"
                )));
            }
            previous_index = Some(*actual_index);
        }
        for (expected_index, (actual_index, _)) in self.emitted_outputs.iter().enumerate() {
            if *actual_index != expected_index {
                return Err(Error::new(format!(
                    "missing emitted output index {expected_index}"
                )));
            }
        }
        if self.emitted_outputs.len() != expected {
            return Err(Error::new(format!(
                "emitted output count mismatch: expected {expected}, found {}",
                self.emitted_outputs.len()
            )));
        }
        Ok(std::mem::take(&mut self.emitted_outputs)
            .into_iter()
            .map(|(_, output)| output)
            .collect())
    }

    fn required(&mut self, name: &str) -> Result<String> {
        let value = match name {
            "format" => self.format.take(),
            "format_version" => self.format_version.take(),
            "source_language" => self.source_language.take(),
            "module" => self.module.take(),
            "entry_process" => self.entry_process.take(),
            "state_type" => self.state_type.take(),
            "message_type" => self.message_type.take(),
            "message_variant" => self.message_variant.take(),
            "init_state" => self.init_state.take(),
            "source_hash_fnv1a64" => self.source_hash_fnv1a64.take(),
            _ => None,
        };
        value.ok_or_else(|| Error::new(format!("missing artifact field {name}")))
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
        fs::create_dir_all(parent)?;
    }
    fs::write(path, artifact.encode())?;
    Ok(())
}

pub fn read_artifact(path: &Path) -> Result<MantleArtifact> {
    let contents = fs::read_to_string(path)?;
    MantleArtifact::decode(&contents)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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
    use crate::language::check_source;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn artifact_round_trips_and_validates_magic() {
        let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det { return Stop(state); }
}
"#;
        let checked = check_source(source).expect("source should check");
        let artifact =
            MantleArtifact::from_checked(&checked, source).expect("artifact should build");
        let encoded = artifact.encode();
        let decoded = MantleArtifact::decode(&encoded).expect("artifact should decode");

        assert_eq!(decoded, artifact);

        let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
        assert!(err.to_string().contains("invalid Mantle artifact magic"));
    }

    #[test]
    fn decode_reports_duplicate_emitted_output_indices() {
        let mut artifact = valid_artifact();
        artifact.emitted_outputs = vec!["first".to_string(), "second".to_string()];
        let encoded = artifact
            .encode()
            .replace("emitted_output_1=second", "emitted_output_0=second");

        let err =
            MantleArtifact::decode(&encoded).expect_err("duplicate emitted output should fail");

        assert!(err.to_string().contains("duplicate emitted output index 0"));
    }

    #[test]
    fn decode_reports_unexpected_emitted_output_indices() {
        let encoded = valid_artifact().encode().replace(
            "emitted_output_0=hello from Strata",
            "emitted_output_2=hello from Strata",
        );

        let err =
            MantleArtifact::decode(&encoded).expect_err("unexpected emitted output should fail");

        assert!(err
            .to_string()
            .contains("unexpected emitted output index 2"));
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

    fn valid_artifact() -> MantleArtifact {
        MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            format_version: ARTIFACT_VERSION.to_string(),
            source_language: STRATA_SOURCE_LANGUAGE.to_string(),
            module: "hello".to_string(),
            entry_process: "Main".to_string(),
            state_type: "MainState".to_string(),
            message_type: "MainMsg".to_string(),
            message_variant: "Start".to_string(),
            mailbox_bound: 1,
            init_state: "MainState".to_string(),
            step_result: StepResult::Stop,
            emitted_outputs: vec!["hello from Strata".to_string()],
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
