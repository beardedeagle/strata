use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::artifact::{read_artifact, MantleArtifact};
use crate::language::StepResult;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub artifact_path: PathBuf,
    pub trace_path: PathBuf,
    pub process: String,
    pub message: String,
    pub status: ProcessStatus,
    pub emitted_outputs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Stopped,
}

pub fn run_artifact_path(path: &Path) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    run_artifact(path, &artifact)
}

pub fn run_artifact(path: &Path, artifact: &MantleArtifact) -> Result<RunReport> {
    artifact.validate()?;

    let trace_path = path.with_extension("observability.jsonl");
    let mut trace_file = prepare_trace_file(&trace_path)?;
    let mut trace = RuntimeTrace::new();
    trace.push(format!(
        "{{\"event\":\"artifact_loaded\",\"format\":\"{}\",\"format_version\":\"{}\",\"source_language\":\"{}\",\"module\":\"{}\",\"entry_process\":\"{}\"}}",
        json_escape(&artifact.format),
        json_escape(&artifact.format_version),
        json_escape(&artifact.source_language),
        json_escape(&artifact.module),
        json_escape(&artifact.entry_process)
    ));

    let mut process = ProcessInstance {
        pid: 1,
        name: artifact.entry_process.clone(),
        state: artifact.init_state.clone(),
        status: ProcessStatus::Running,
        mailbox_bound: artifact.mailbox_bound,
        mailbox: VecDeque::new(),
    };
    trace.push(format!(
        "{{\"event\":\"process_spawned\",\"pid\":{},\"process\":\"{}\",\"state\":\"{}\",\"mailbox_bound\":{}}}",
        process.pid,
        json_escape(&process.name),
        json_escape(&process.state),
        process.mailbox_bound
    ));

    process.send(artifact.message_variant.clone(), &mut trace)?;
    let msg = process.dequeue(&mut trace)?;
    process.step(
        &msg,
        artifact.step_result,
        &artifact.emitted_outputs,
        &mut trace,
    )?;

    trace_file.write_all(trace.finish().as_bytes())?;
    trace_file.flush()?;

    Ok(RunReport {
        artifact_path: path.to_path_buf(),
        trace_path,
        process: artifact.entry_process.clone(),
        message: msg,
        status: process.status,
        emitted_outputs: artifact.emitted_outputs.clone(),
    })
}

fn prepare_trace_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?)
}

struct ProcessInstance {
    pid: u64,
    name: String,
    state: String,
    status: ProcessStatus,
    mailbox_bound: usize,
    mailbox: VecDeque<String>,
}

impl ProcessInstance {
    fn send(&mut self, msg: String, trace: &mut RuntimeTrace) -> Result<()> {
        if self.status != ProcessStatus::Running {
            return Err(Error::new(format!(
                "send to process {} failed because it is not running",
                self.name
            )));
        }
        if self.mailbox.len() >= self.mailbox_bound {
            return Err(Error::new(format!(
                "mailbox for process {} is full; message was not accepted",
                self.name
            )));
        }
        self.mailbox.push_back(msg.clone());
        trace.push(format!(
            "{{\"event\":\"message_accepted\",\"pid\":{},\"message\":\"{}\",\"queue_depth\":{}}}",
            self.pid,
            json_escape(&msg),
            self.mailbox.len()
        ));
        Ok(())
    }

    fn dequeue(&mut self, trace: &mut RuntimeTrace) -> Result<String> {
        let msg = self
            .mailbox
            .pop_front()
            .ok_or_else(|| Error::new(format!("process {} mailbox is empty", self.name)))?;
        trace.push(format!(
            "{{\"event\":\"message_dequeued\",\"pid\":{},\"message\":\"{}\",\"queue_depth\":{}}}",
            self.pid,
            json_escape(&msg),
            self.mailbox.len()
        ));
        Ok(msg)
    }

    fn step(
        &mut self,
        msg: &str,
        result: StepResult,
        emitted_outputs: &[String],
        trace: &mut RuntimeTrace,
    ) -> Result<()> {
        if self.status != ProcessStatus::Running {
            return Err(Error::new(format!(
                "process {} cannot step because it is not running",
                self.name
            )));
        }

        for output in emitted_outputs {
            trace.push(format!(
                "{{\"event\":\"program_output\",\"pid\":{},\"stream\":\"stdout\",\"text\":\"{}\"}}",
                self.pid,
                json_escape(output)
            ));
        }

        let result_name = match result {
            StepResult::Continue => "Continue",
            StepResult::Stop => {
                self.status = ProcessStatus::Stopped;
                "Stop"
            }
        };
        trace.push(format!(
            "{{\"event\":\"process_stepped\",\"pid\":{},\"message\":\"{}\",\"result\":\"{}\",\"state\":\"{}\"}}",
            self.pid,
            json_escape(msg),
            result_name,
            json_escape(&self.state)
        ));
        if self.status == ProcessStatus::Stopped {
            trace.push(format!(
                "{{\"event\":\"process_stopped\",\"pid\":{},\"reason\":\"normal\"}}",
                self.pid
            ));
        }
        Ok(())
    }
}

struct RuntimeTrace {
    lines: Vec<String>,
}

impl RuntimeTrace {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    fn push(&mut self, line: String) {
        self.lines.push(line);
    }

    fn finish(self) -> String {
        let mut output = self.lines.join("\n");
        output.push('\n');
        output
    }
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{
        MantleArtifact, ARTIFACT_FORMAT, ARTIFACT_VERSION, STRATA_SOURCE_LANGUAGE,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_rejects_invalid_artifact_identity() {
        let artifact = MantleArtifact {
            format: "other".to_string(),
            format_version: "0".to_string(),
            source_language: "strata".to_string(),
            module: "hello".to_string(),
            entry_process: "Main".to_string(),
            state_type: "MainState".to_string(),
            message_type: "MainMsg".to_string(),
            message_variant: "Start".to_string(),
            mailbox_bound: 1,
            init_state: "MainState".to_string(),
            step_result: StepResult::Stop,
            emitted_outputs: Vec::new(),
            source_hash_fnv1a64: "0000000000000000".to_string(),
        };

        let err = run_artifact(Path::new("target/test/bad.mta"), &artifact)
            .expect_err("invalid artifact must fail closed");
        assert!(err.to_string().contains("unsupported artifact format"));
    }

    #[test]
    fn runtime_rejects_blocked_trace_sink_before_returning_run_report() {
        let dir = unique_test_dir("blocked-trace-sink");
        fs::create_dir_all(&dir).expect("test dir should be created");
        let blocked_parent = dir.join("blocked");
        fs::write(&blocked_parent, "not a directory").expect("blocking file should be written");

        let artifact_path = blocked_parent.join("hello.mta");
        let trace_path = artifact_path.with_extension("observability.jsonl");
        let artifact = valid_artifact();

        let err = run_artifact(&artifact_path, &artifact)
            .expect_err("blocked trace sink should fail before a run report is returned");

        assert!(!err.to_string().is_empty());
        assert!(!trace_path.exists(), "trace path must not be created");

        let _ = fs::remove_file(blocked_parent);
        let _ = fs::remove_dir(dir);
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
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("strata-{name}-{}-{nanos}", std::process::id()))
    }
}
