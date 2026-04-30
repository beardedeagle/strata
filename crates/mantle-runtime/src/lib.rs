use std::collections::VecDeque;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{LineWriter, Write};
use std::path::{Path, PathBuf};

use mantle_artifact::{
    read_artifact, ArtifactAction, Error, MantleArtifact, MessageId, OutputId, ProcessId, Result,
    StateId, StepResult,
};

pub const DEFAULT_MAX_DISPATCHES: usize = 10_000;
pub const DEFAULT_MAX_TRACE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_EMITTED_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub artifact_path: PathBuf,
    pub trace_path: PathBuf,
    pub entry_process: String,
    pub entry_message: String,
    pub spawned_processes: Vec<SpawnReport>,
    pub delivered_messages: Vec<MessageDelivery>,
    pub processes: Vec<ProcessReport>,
    pub emitted_outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnReport {
    pub pid: u64,
    pub process: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDelivery {
    pub pid: u64,
    pub process: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessReport {
    pub pid: u64,
    pub process: String,
    pub state: String,
    pub status: ProcessStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub max_dispatches: usize,
    pub max_trace_bytes: usize,
    pub max_emitted_output_bytes: usize,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_dispatches: DEFAULT_MAX_DISPATCHES,
            max_trace_bytes: DEFAULT_MAX_TRACE_BYTES,
            max_emitted_output_bytes: DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        }
    }
}

impl RunLimits {
    fn validate(self) -> Result<()> {
        if self.max_dispatches == 0 {
            return Err(Error::new("max_dispatches must be greater than zero"));
        }
        if self.max_trace_bytes == 0 {
            return Err(Error::new("max_trace_bytes must be greater than zero"));
        }
        if self.max_emitted_output_bytes == 0 {
            return Err(Error::new(
                "max_emitted_output_bytes must be greater than zero",
            ));
        }
        Ok(())
    }
}

pub fn run_artifact_path(path: &Path) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    run_artifact(path, &artifact)
}

pub fn run_artifact(path: &Path, artifact: &MantleArtifact) -> Result<RunReport> {
    run_artifact_with_limits(path, artifact, RunLimits::default())
}

pub fn run_artifact_with_limits(
    path: &Path,
    artifact: &MantleArtifact,
    limits: RunLimits,
) -> Result<RunReport> {
    limits.validate()?;
    let program = LoadedProgram::from_artifact(artifact)?;

    let trace_path = path.with_extension("observability.jsonl");
    let trace_file = prepare_trace_file(&trace_path)?;

    let trace = RuntimeTrace::new(trace_file, limits.max_trace_bytes);
    let mut run = RuntimeRun::new(&program, trace, limits.max_emitted_output_bytes);
    run.trace.push(format!(
        "{{\"event\":\"artifact_loaded\",\"format\":\"{}\",\"format_version\":\"{}\",\"source_language\":\"{}\",\"module\":\"{}\",\"entry_process\":\"{}\",\"process_count\":{}}}",
        json_escape(&program.format),
        json_escape(&program.format_version),
        json_escape(&program.source_language),
        json_escape(&program.module),
        json_escape(program.process_label(program.entry_process)?),
        program.processes.len()
    ))?;

    run.spawn_process(program.entry_process, None)?;
    run.send_message(program.entry_process, program.entry_message, None)?;
    run.drain_mailboxes(limits.max_dispatches)?;
    run.reject_unhandled_messages()?;
    run.trace.flush()?;

    let process_reports = run
        .processes
        .into_iter()
        .map(|process| {
            Ok(ProcessReport {
                pid: process.pid,
                process: program.process_label(process.process_id)?.to_string(),
                state: program
                    .state_label(process.process_id, process.state)?
                    .to_string(),
                status: process.status,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(RunReport {
        artifact_path: path.to_path_buf(),
        trace_path,
        entry_process: program.process_label(program.entry_process)?.to_string(),
        entry_message: program
            .message_label(program.entry_process, program.entry_message)?
            .to_string(),
        spawned_processes: run.spawned_processes,
        delivered_messages: run.delivered_messages,
        processes: process_reports,
        emitted_outputs: run.emitted_outputs,
    })
}

fn prepare_trace_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?)
}

#[derive(Debug, Clone)]
struct LoadedProgram {
    format: String,
    format_version: String,
    source_language: String,
    module: String,
    entry_process: ProcessId,
    entry_message: MessageId,
    outputs: Vec<String>,
    processes: Vec<LoadedProcess>,
}

impl LoadedProgram {
    fn from_artifact(artifact: &MantleArtifact) -> Result<Self> {
        artifact.validate()?;
        let processes = artifact
            .processes
            .iter()
            .map(|process| {
                Ok(LoadedProcess {
                    debug_name: process.debug_name.clone(),
                    state_values: process.state_values.clone(),
                    message_variants: process.message_variants.clone(),
                    mailbox_bound: process.mailbox_bound,
                    init_state: process.init_state,
                    step_result: process.step_result,
                    final_state: process.final_state,
                    actions: process
                        .actions
                        .iter()
                        .map(LoadedAction::from_artifact)
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            format: artifact.format.clone(),
            format_version: artifact.format_version.clone(),
            source_language: artifact.source_language.clone(),
            module: artifact.module.clone(),
            entry_process: artifact.entry_process,
            entry_message: artifact.entry_message,
            outputs: artifact.outputs.clone(),
            processes,
        })
    }

    fn process(&self, id: ProcessId) -> Result<&LoadedProcess> {
        self.processes
            .get(id.index())
            .ok_or_else(|| Error::new(format!("process id {} is not loaded", id.as_u32())))
    }

    fn process_label(&self, id: ProcessId) -> Result<&str> {
        Ok(self.process(id)?.debug_name.as_str())
    }

    fn state_label(&self, process_id: ProcessId, state_id: StateId) -> Result<&str> {
        self.process(process_id)?
            .state_values
            .get(state_id.index())
            .map(String::as_str)
            .ok_or_else(|| {
                Error::new(format!(
                    "state id {} is not loaded for process id {}",
                    state_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    fn message_label(&self, process_id: ProcessId, message_id: MessageId) -> Result<&str> {
        self.process(process_id)?
            .message_variants
            .get(message_id.index())
            .map(String::as_str)
            .ok_or_else(|| {
                Error::new(format!(
                    "message id {} is not loaded for process id {}",
                    message_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    fn output(&self, output_id: OutputId) -> Result<&str> {
        self.outputs
            .get(output_id.index())
            .map(String::as_str)
            .ok_or_else(|| Error::new(format!("output id {} is not loaded", output_id.as_u32())))
    }
}

#[derive(Debug, Clone)]
struct LoadedProcess {
    debug_name: String,
    state_values: Vec<String>,
    message_variants: Vec<String>,
    mailbox_bound: usize,
    init_state: StateId,
    step_result: StepResult,
    final_state: StateId,
    actions: Vec<LoadedAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadedAction {
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

impl LoadedAction {
    fn from_artifact(action: &ArtifactAction) -> Self {
        match action {
            ArtifactAction::Emit { output } => Self::Emit { output: *output },
            ArtifactAction::Spawn { target } => Self::Spawn { target: *target },
            ArtifactAction::Send { target, message } => Self::Send {
                target: *target,
                message: *message,
            },
        }
    }
}

struct RuntimeRun<'a> {
    program: &'a LoadedProgram,
    processes: Vec<ProcessInstance>,
    next_pid: u64,
    trace: RuntimeTrace,
    emitted_output_bytes: usize,
    max_emitted_output_bytes: usize,
    spawned_processes: Vec<SpawnReport>,
    delivered_messages: Vec<MessageDelivery>,
    emitted_outputs: Vec<String>,
}

impl<'a> RuntimeRun<'a> {
    fn new(
        program: &'a LoadedProgram,
        trace: RuntimeTrace,
        max_emitted_output_bytes: usize,
    ) -> Self {
        Self {
            program,
            processes: Vec::new(),
            next_pid: 1,
            trace,
            emitted_output_bytes: 0,
            max_emitted_output_bytes,
            spawned_processes: Vec::new(),
            delivered_messages: Vec::new(),
            emitted_outputs: Vec::new(),
        }
    }

    fn spawn_process(&mut self, process_id: ProcessId, spawned_by_pid: Option<u64>) -> Result<()> {
        let definition = self.program.process(process_id)?;
        if self
            .processes
            .iter()
            .any(|process| process.process_id == process_id)
        {
            return Err(Error::new(format!(
                "process {} is already spawned",
                definition.debug_name
            )));
        }

        let pid = self.next_pid;
        self.next_pid += 1;
        let process = ProcessInstance {
            pid,
            process_id,
            state: definition.init_state,
            status: ProcessStatus::Running,
            mailbox_bound: definition.mailbox_bound,
            mailbox: VecDeque::new(),
        };

        match spawned_by_pid {
            Some(parent_pid) => self.trace.push(format!(
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process\":\"{}\",\"state\":\"{}\",\"mailbox_bound\":{},\"spawned_by_pid\":{}}}",
                process.pid,
                json_escape(&definition.debug_name),
                json_escape(self.program.state_label(process_id, process.state)?),
                process.mailbox_bound,
                parent_pid
            ))?,
            None => self.trace.push(format!(
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process\":\"{}\",\"state\":\"{}\",\"mailbox_bound\":{}}}",
                process.pid,
                json_escape(&definition.debug_name),
                json_escape(self.program.state_label(process_id, process.state)?),
                process.mailbox_bound
            ))?,
        };
        self.spawned_processes.push(SpawnReport {
            pid: process.pid,
            process: definition.debug_name.clone(),
        });
        self.processes.push(process);
        Ok(())
    }

    fn send_message(
        &mut self,
        target: ProcessId,
        message: MessageId,
        sender_pid: Option<u64>,
    ) -> Result<()> {
        let target_process = self.program.process(target)?;
        let message_label = self.program.message_label(target, message)?;
        let process_label = target_process.debug_name.as_str();

        let process = self
            .processes
            .iter_mut()
            .find(|process| process.process_id == target)
            .ok_or_else(|| Error::new(format!("process {process_label} is not spawned")))?;
        if process.status != ProcessStatus::Running {
            return Err(Error::new(format!(
                "send to process {} failed because it is not running",
                process_label
            )));
        }
        if process.mailbox.len() >= process.mailbox_bound {
            return Err(Error::new(format!(
                "mailbox for process {} is full; message was not accepted",
                process_label
            )));
        }

        match sender_pid {
            Some(pid) => self.trace.push(format!(
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process\":\"{}\",\"message\":\"{}\",\"queue_depth\":{},\"sender_pid\":{}}}",
                process.pid,
                json_escape(process_label),
                json_escape(message_label),
                process.mailbox.len() + 1,
                pid
            ))?,
            None => self.trace.push(format!(
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process\":\"{}\",\"message\":\"{}\",\"queue_depth\":{}}}",
                process.pid,
                json_escape(process_label),
                json_escape(message_label),
                process.mailbox.len() + 1
            ))?,
        };
        process.mailbox.push_back(message);
        self.delivered_messages.push(MessageDelivery {
            pid: process.pid,
            process: process_label.to_string(),
            message: message_label.to_string(),
        });
        Ok(())
    }

    fn drain_mailboxes(&mut self, max_dispatches: usize) -> Result<()> {
        let mut dispatches = 0usize;
        while let Some(process_index) = self.next_runnable_process() {
            if dispatches >= max_dispatches {
                return Err(Error::new(format!(
                    "runtime dispatch budget exceeded after {max_dispatches} process step(s)"
                )));
            }
            let message = self.processes[process_index].dequeue(self.program, &mut self.trace)?;
            self.step_process(process_index, message)?;
            dispatches += 1;
        }
        Ok(())
    }

    fn next_runnable_process(&self) -> Option<usize> {
        self.processes.iter().position(|process| {
            process.status == ProcessStatus::Running && !process.mailbox.is_empty()
        })
    }

    fn step_process(&mut self, process_index: usize, message: MessageId) -> Result<()> {
        let program = self.program;
        let pid = self.processes[process_index].pid;
        let process_id = self.processes[process_index].process_id;
        let process_name = program.process_label(process_id)?.to_string();
        let message_label = program.message_label(process_id, message)?.to_string();
        if self.processes[process_index].status != ProcessStatus::Running {
            return Err(Error::new(format!(
                "process {process_name} cannot step because it is not running"
            )));
        }

        let definition = program.process(process_id)?;
        let final_state = definition.final_state;
        let step_result = definition.step_result;

        for &action in &definition.actions {
            match action {
                LoadedAction::Emit { output } => {
                    let text = program.output(output)?.to_string();
                    let emitted_output_bytes =
                        checked_output_bytes(self.emitted_output_bytes, text.len())?;
                    if emitted_output_bytes > self.max_emitted_output_bytes {
                        return Err(Error::new(format!(
                            "emitted output exceeded maximum size of {} bytes",
                            self.max_emitted_output_bytes
                        )));
                    }
                    self.trace.push(format!(
                        "{{\"event\":\"program_output\",\"pid\":{},\"process\":\"{}\",\"stream\":\"stdout\",\"text\":\"{}\"}}",
                        pid,
                        json_escape(&process_name),
                        json_escape(&text)
                    ))?;
                    self.emitted_output_bytes = emitted_output_bytes;
                    self.emitted_outputs.push(text);
                }
                LoadedAction::Spawn { target } => self.spawn_process(target, Some(pid))?,
                LoadedAction::Send { target, message } => {
                    self.send_message(target, message, Some(pid))?;
                }
            }
        }

        let previous_state = self.processes[process_index].state;
        if previous_state != final_state {
            self.trace.push(format!(
                "{{\"event\":\"state_updated\",\"pid\":{},\"process\":\"{}\",\"from\":\"{}\",\"to\":\"{}\"}}",
                pid,
                json_escape(&process_name),
                json_escape(program.state_label(process_id, previous_state)?),
                json_escape(program.state_label(process_id, final_state)?)
            ))?;
            self.processes[process_index].state = final_state;
        }

        let result_name = match step_result {
            StepResult::Continue => "Continue",
            StepResult::Stop => "Stop",
        };
        self.trace.push(format!(
            "{{\"event\":\"process_stepped\",\"pid\":{},\"process\":\"{}\",\"message\":\"{}\",\"result\":\"{}\",\"state\":\"{}\"}}",
            pid,
            json_escape(&process_name),
            json_escape(&message_label),
            result_name,
            json_escape(program.state_label(process_id, self.processes[process_index].state)?)
        ))?;
        if step_result == StepResult::Stop {
            self.trace.push(format!(
                "{{\"event\":\"process_stopped\",\"pid\":{},\"process\":\"{}\",\"reason\":\"normal\"}}",
                pid,
                json_escape(&process_name)
            ))?;
            self.processes[process_index].status = ProcessStatus::Stopped;
        }
        Ok(())
    }

    fn reject_unhandled_messages(&self) -> Result<()> {
        for process in &self.processes {
            if !process.mailbox.is_empty() {
                return Err(Error::new(format!(
                    "process {} has {} unhandled message(s)",
                    self.program.process_label(process.process_id)?,
                    process.mailbox.len()
                )));
            }
        }
        Ok(())
    }
}

struct ProcessInstance {
    pid: u64,
    process_id: ProcessId,
    state: StateId,
    status: ProcessStatus,
    mailbox_bound: usize,
    mailbox: VecDeque<MessageId>,
}

impl ProcessInstance {
    fn dequeue(&mut self, program: &LoadedProgram, trace: &mut RuntimeTrace) -> Result<MessageId> {
        let message = *self.mailbox.front().ok_or_else(|| {
            Error::new(format!(
                "process {} mailbox is empty",
                program
                    .process_label(self.process_id)
                    .unwrap_or("<unknown>")
            ))
        })?;
        let queue_depth = self.mailbox.len() - 1;
        trace.push(format!(
            "{{\"event\":\"message_dequeued\",\"pid\":{},\"process\":\"{}\",\"message\":\"{}\",\"queue_depth\":{}}}",
            self.pid,
            json_escape(program.process_label(self.process_id)?),
            json_escape(program.message_label(self.process_id, message)?),
            queue_depth
        ))?;
        let removed = self.mailbox.pop_front().ok_or_else(|| {
            Error::new(format!(
                "process {} mailbox changed during dequeue",
                program
                    .process_label(self.process_id)
                    .unwrap_or("<unknown>")
            ))
        })?;
        Ok(removed)
    }
}

struct RuntimeTrace {
    file: LineWriter<File>,
    bytes_written: usize,
    max_bytes: usize,
}

impl RuntimeTrace {
    fn new(file: File, max_bytes: usize) -> Self {
        Self {
            file: LineWriter::new(file),
            bytes_written: 0,
            max_bytes,
        }
    }

    fn push(&mut self, line: String) -> Result<()> {
        let line_bytes = line
            .len()
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime trace line size overflowed"))?;
        let next_bytes = self
            .bytes_written
            .checked_add(line_bytes)
            .ok_or_else(|| Error::new("runtime trace size overflowed"))?;
        if next_bytes > self.max_bytes {
            return Err(Error::new(format!(
                "runtime trace exceeded maximum size of {} bytes",
                self.max_bytes
            )));
        }

        let mut trace_line = line;
        trace_line.push('\n');
        self.file.write_all(trace_line.as_bytes())?;
        self.bytes_written = next_bytes;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }
}

fn checked_output_bytes(current: usize, next_output_len: usize) -> Result<usize> {
    let next_output_with_newline = next_output_len
        .checked_add(1)
        .ok_or_else(|| Error::new("emitted output size overflowed"))?;
    current
        .checked_add(next_output_with_newline)
        .ok_or_else(|| Error::new("emitted output size overflowed"))
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

pub fn mantle_main<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    match args.next().as_deref() {
        Some("run") => {
            let path = required_path(args.next(), "mantle run <artifact.mta>")?;
            ensure_no_extra_args(args)?;
            let report = run_artifact_path(&path)?;
            println!("mantle: loaded {}", report.artifact_path.display());
            for spawned in &report.spawned_processes {
                println!("mantle: spawned {} pid={}", spawned.process, spawned.pid);
            }
            for delivery in &report.delivered_messages {
                println!(
                    "mantle: delivered {} to {}",
                    delivery.message, delivery.process
                );
            }
            for output in &report.emitted_outputs {
                println!("{output}");
            }
            for process in &report.processes {
                match process.status {
                    ProcessStatus::Running => {
                        println!("mantle: process {} remains running", process.process);
                    }
                    ProcessStatus::Stopped => {
                        println!("mantle: stopped {} normally", process.process);
                    }
                }
            }
            println!("mantle: trace {}", report.trace_path.display());
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_mantle_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!("unknown mantle command {other:?}"))),
        None => {
            print_mantle_usage();
            Err(Error::new("missing mantle command"))
        }
    }
}

fn required_path(value: Option<String>, usage: &str) -> Result<PathBuf> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(format!("missing path; usage: {usage}")))
}

fn ensure_no_extra_args(args: impl IntoIterator<Item = String>) -> Result<()> {
    let extras: Vec<String> = args.into_iter().collect();
    if extras.is_empty() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "unexpected arguments: {}",
            extras.join(" ")
        )))
    }
}

fn print_mantle_usage() {
    println!("usage:");
    println!("  mantle run <path.mta>");
}

pub fn run_mantle_from_env() -> Result<()> {
    mantle_main(env::args())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantle_artifact::{
        write_artifact, ArtifactAction, ArtifactProcess, MessageId, OutputId, ProcessId, StateId,
        ARTIFACT_FORMAT, ARTIFACT_VERSION, STRATA_SOURCE_LANGUAGE,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_rejects_invalid_artifact_identity() {
        let mut artifact = valid_artifact();
        artifact.format = "other".to_string();

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

    #[test]
    fn run_artifact_path_writes_trace_for_current_directory_artifact() {
        let artifact_path = unique_current_dir_artifact_path("runtime-current-dir");
        let trace_path = artifact_path.with_extension("observability.jsonl");
        let artifact = valid_artifact();

        write_artifact(&artifact_path, &artifact).expect("artifact write should succeed");

        let report =
            run_artifact_path(&artifact_path).expect("current-directory artifact run should work");

        assert_eq!(report.trace_path, trace_path);
        assert!(trace_path.exists(), "runtime trace should be written");
        let trace = fs::read_to_string(&trace_path).expect("runtime trace should be readable");
        assert!(trace.contains(r#""event":"artifact_loaded""#));
        assert!(trace.contains(r#""event":"process_stopped""#));

        fs::remove_file(artifact_path).expect("test artifact should be removed");
        fs::remove_file(trace_path).expect("test trace should be removed");
    }

    #[test]
    fn runtime_rejects_dispatch_budget_exhaustion() {
        let artifact_path = unique_current_dir_artifact_path("runtime-budget");
        let trace_path = artifact_path.with_extension("observability.jsonl");
        let artifact = looping_artifact();

        let err = run_artifact_with_limits(
            &artifact_path,
            &artifact,
            RunLimits {
                max_dispatches: 3,
                ..RunLimits::default()
            },
        )
        .expect_err("looping artifact should hit the dispatch budget");

        assert!(err
            .to_string()
            .contains("runtime dispatch budget exceeded after 3 process step(s)"));

        let _ = fs::remove_file(trace_path);
    }

    #[test]
    fn runtime_rejects_trace_limit_exhaustion() {
        let artifact_path = unique_current_dir_artifact_path("runtime-trace-limit");
        let trace_path = artifact_path.with_extension("observability.jsonl");
        let artifact = valid_artifact();

        let err = run_artifact_with_limits(
            &artifact_path,
            &artifact,
            RunLimits {
                max_trace_bytes: 8,
                ..RunLimits::default()
            },
        )
        .expect_err("small trace limit should fail closed");

        assert!(err
            .to_string()
            .contains("runtime trace exceeded maximum size of 8 bytes"));

        let _ = fs::remove_file(trace_path);
    }

    #[test]
    fn runtime_rejects_emitted_output_limit_exhaustion() {
        let artifact_path = unique_current_dir_artifact_path("runtime-output-limit");
        let trace_path = artifact_path.with_extension("observability.jsonl");
        let artifact = valid_artifact();

        let err = run_artifact_with_limits(
            &artifact_path,
            &artifact,
            RunLimits {
                max_emitted_output_bytes: "worker handled Ping".len(),
                ..RunLimits::default()
            },
        )
        .expect_err("small emitted output limit should fail closed");

        assert!(err
            .to_string()
            .contains("emitted output exceeded maximum size"));

        let _ = fs::remove_file(trace_path);
    }

    #[test]
    fn actor_artifact_spawns_sends_updates_state_and_stops() {
        let artifact_path = unique_current_dir_artifact_path("runtime-actor");
        let trace_path = artifact_path.with_extension("observability.jsonl");
        let artifact = valid_artifact();

        write_artifact(&artifact_path, &artifact).expect("artifact write should succeed");

        let report = run_artifact_path(&artifact_path).expect("actor artifact should run");

        assert_eq!(report.spawned_processes.len(), 2);
        assert_eq!(report.delivered_messages.len(), 2);
        assert_eq!(report.emitted_outputs, ["worker handled Ping"]);
        assert!(report
            .processes
            .iter()
            .any(|process| process.process == "Worker"
                && process.state == "Handled"
                && process.status == ProcessStatus::Stopped));

        let trace = fs::read_to_string(&trace_path).expect("runtime trace should be readable");
        assert!(trace.contains(r#""event":"process_spawned""#));
        assert!(trace.contains(r#""process":"Worker""#));
        assert!(trace.contains(r#""event":"message_accepted""#));
        assert!(trace.contains(r#""message":"Ping""#));
        assert!(trace.contains(r#""event":"message_dequeued""#));
        assert!(trace.contains(r#""event":"state_updated""#));
        assert!(trace.contains(r#""from":"Idle","to":"Handled""#));
        assert!(trace.contains(r#""event":"process_stopped""#));

        fs::remove_file(artifact_path).expect("test artifact should be removed");
        fs::remove_file(trace_path).expect("test trace should be removed");
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

    fn looping_artifact() -> MantleArtifact {
        MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            format_version: ARTIFACT_VERSION.to_string(),
            source_language: STRATA_SOURCE_LANGUAGE.to_string(),
            module: "looping".to_string(),
            entry_process: ProcessId::new(0),
            entry_message: MessageId::new(0),
            outputs: Vec::new(),
            processes: vec![ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: "MainState".to_string(),
                state_values: vec!["MainState".to_string()],
                message_type: "MainMsg".to_string(),
                message_variants: vec!["Start".to_string()],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                step_result: StepResult::Continue,
                final_state: StateId::new(0),
                actions: vec![ArtifactAction::Send {
                    target: ProcessId::new(0),
                    message: MessageId::new(0),
                }],
            }],
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
