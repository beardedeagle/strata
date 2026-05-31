#![forbid(unsafe_code)]

use std::path::Path;

use mantle_artifact::{MantleArtifact, Result, read_artifact};

mod cli;
mod event;
mod executable;
mod feature_declaration;
mod host;
mod limits;
mod program;
mod report;
mod run;

pub use cli::{mantle_main, run_mantle_from_env};
pub use event::{
    RuntimeBranchPath, RuntimeBranchScope, RuntimeEvent, RuntimeEventRecord, RuntimeFailureReason,
    RuntimeLoopContext, RuntimeOutputStream, RuntimeProcessId, RuntimeStepResult,
    RuntimeStopReason,
};
pub use feature_declaration::{
    RUNTIME_FEATURE_DECLARATION_SCHEMA_VERSION, RuntimeFeatureDeclarationFormat,
    render_runtime_feature_declaration,
};
pub use host::{InMemoryRuntimeHost, RuntimeHost};
pub use limits::{
    DEFAULT_MAX_DISPATCHES, DEFAULT_MAX_EMITTED_OUTPUT_BYTES, DEFAULT_MAX_RUNTIME_PROCESSES,
    DEFAULT_MAX_TRACE_BYTES, RunLimits, SpawnAuthorityPolicy,
};
pub use program::RuntimePayload;
pub use report::{
    MessageDelivery, ProcessReport, ProcessStatus, RunReport, RuntimeReport, SpawnReport,
};
pub use run::run_artifact_with_host;

use host::{JsonlTraceHost, prepare_trace_file};
use program::LoadedProgram;
use run::run_loaded_program_with_host;

pub fn run_artifact_path(path: &Path) -> Result<RunReport> {
    run_artifact_path_with_limits(path, RunLimits::default())
}

pub fn run_artifact_path_with_limits(path: &Path, limits: RunLimits) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    run_artifact_with_limits(path, &artifact, limits)
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
    let mut host = JsonlTraceHost::new(trace_file, limits.max_trace_bytes);
    let runtime_report = run_loaded_program_with_host(&program, &mut host, limits)?;

    Ok(RunReport {
        artifact_path: path.to_path_buf(),
        trace_path,
        entry_process: runtime_report.entry_process,
        entry_message: runtime_report.entry_message,
        spawned_processes: runtime_report.spawned_processes,
        delivered_messages: runtime_report.delivered_messages,
        processes: runtime_report.processes,
        emitted_outputs: runtime_report.emitted_outputs,
    })
}

#[cfg(test)]
mod tests;
