#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::path::Path;

use mantle_artifact::{MantleArtifact, Result, read_artifact};

mod authority_effect_binding;
mod cli;
mod composition_binding;
mod event;
mod executable;
mod feature_declaration;
mod host;
mod limits;
mod program;
mod report;
mod run;
mod target_profile;

pub use authority_effect_binding::validate_runtime_authority_effect_binding_text;
pub use cli::{mantle_main, run_mantle_from_env};
pub use composition_binding::validate_runtime_composition_binding_text;
pub use event::{
    RUNTIME_TRACE_SCHEMA_ID, RUNTIME_TRACE_SCHEMA_VERSION, RuntimeBranchPath, RuntimeBranchScope,
    RuntimeEffectOutcomeAction, RuntimeEffectOutcomeResult, RuntimeEvent, RuntimeEventRecord,
    RuntimeFailureReason, RuntimeLoopContext, RuntimeOutputStream, RuntimeProcessId,
    RuntimeStepResult, RuntimeStopReason, RuntimeTraceEventContract, RuntimeTraceEventKind,
    RuntimeTraceSummary, RuntimeTraceValidationLimits, validate_runtime_trace_jsonl,
    validate_runtime_trace_jsonl_with_limits,
};
pub use feature_declaration::{
    RUNTIME_FEATURE_DECLARATION_SCHEMA_VERSION, RuntimeFeatureDeclarationFormat,
    render_runtime_feature_declaration,
};
pub use host::{InMemoryRuntimeHost, RuntimeHost};
pub use limits::{
    DEFAULT_MAX_DISPATCHES, DEFAULT_MAX_EMITTED_OUTPUT_BYTES, DEFAULT_MAX_RUNTIME_PROCESSES,
    DEFAULT_MAX_TRACE_BYTES, LocalSpawnBackend, RunLimits, SpawnAuthorityPolicy,
};
pub use program::RuntimePayload;
pub use report::{
    MessageDelivery, ProcessReport, ProcessStatus, RunReport, RuntimeReport, SpawnReport,
};
pub use run::{run_artifact_with_host, run_artifact_with_host_and_binding_texts};

pub(crate) use authority_effect_binding::{RuntimeAuthorityEffectBinding, RuntimeAuthorityPolicy};
pub(crate) use composition_binding::RuntimeCompositionBinding;
use host::{FilesystemRuntimeHost, prepare_trace_file};
use program::LoadedProgram;
use run::{run_loaded_program_with_bindings, validate_authority_binding_limits};

pub fn run_artifact_path(path: &Path) -> Result<RunReport> {
    run_artifact_path_with_limits(path, RunLimits::default())
}

pub fn run_artifact_path_with_limits(path: &Path, limits: RunLimits) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    run_artifact_with_limits(path, &artifact, limits)
}

pub fn run_artifact_path_with_limits_and_composition_binding(
    path: &Path,
    limits: RunLimits,
    composition_binding_path: &Path,
) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    let composition_binding =
        RuntimeCompositionBinding::read_path(composition_binding_path, &artifact)?;
    run_artifact_with_limits_and_composition_binding(
        path,
        &artifact,
        limits,
        Some(composition_binding),
    )
}

pub fn run_artifact_path_with_limits_and_authority_effect_binding(
    path: &Path,
    limits: RunLimits,
    authority_effect_binding_path: &Path,
) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    let authority_effect_binding =
        RuntimeAuthorityEffectBinding::read_path(authority_effect_binding_path, &artifact)?;
    run_artifact_with_limits_and_bindings(
        path,
        &artifact,
        limits,
        None,
        Some(authority_effect_binding),
    )
}

pub fn run_artifact_path_with_limits_and_bindings(
    path: &Path,
    limits: RunLimits,
    composition_binding_path: Option<&Path>,
    authority_effect_binding_path: Option<&Path>,
) -> Result<RunReport> {
    run_artifact_path_with_limits_and_bindings_and_output(
        path,
        limits,
        composition_binding_path,
        authority_effect_binding_path,
        io::sink(),
    )
}

pub(crate) fn run_artifact_path_with_limits_and_bindings_and_output<W: Write>(
    path: &Path,
    limits: RunLimits,
    composition_binding_path: Option<&Path>,
    authority_effect_binding_path: Option<&Path>,
    output: W,
) -> Result<RunReport> {
    let artifact = read_artifact(path)?;
    let composition_binding = composition_binding_path
        .map(|binding_path| RuntimeCompositionBinding::read_path(binding_path, &artifact))
        .transpose()?;
    let authority_effect_binding = authority_effect_binding_path
        .map(|binding_path| RuntimeAuthorityEffectBinding::read_path(binding_path, &artifact))
        .transpose()?;
    run_artifact_with_limits_and_bindings_and_output(
        path,
        &artifact,
        limits,
        composition_binding,
        authority_effect_binding,
        output,
    )
}

pub fn run_artifact(path: &Path, artifact: &MantleArtifact) -> Result<RunReport> {
    run_artifact_with_limits(path, artifact, RunLimits::default())
}

pub fn run_artifact_with_limits(
    path: &Path,
    artifact: &MantleArtifact,
    limits: RunLimits,
) -> Result<RunReport> {
    run_artifact_with_limits_and_composition_binding(path, artifact, limits, None)
}

pub(crate) fn run_artifact_with_limits_and_composition_binding(
    path: &Path,
    artifact: &MantleArtifact,
    limits: RunLimits,
    composition_binding: Option<RuntimeCompositionBinding>,
) -> Result<RunReport> {
    run_artifact_with_limits_and_bindings(path, artifact, limits, composition_binding, None)
}

pub(crate) fn run_artifact_with_limits_and_bindings(
    path: &Path,
    artifact: &MantleArtifact,
    limits: RunLimits,
    composition_binding: Option<RuntimeCompositionBinding>,
    authority_effect_binding: Option<RuntimeAuthorityEffectBinding>,
) -> Result<RunReport> {
    run_artifact_with_limits_and_bindings_and_output(
        path,
        artifact,
        limits,
        composition_binding,
        authority_effect_binding,
        io::sink(),
    )
}

pub(crate) fn run_artifact_with_limits_and_bindings_and_output<W: Write>(
    path: &Path,
    artifact: &MantleArtifact,
    limits: RunLimits,
    composition_binding: Option<RuntimeCompositionBinding>,
    authority_effect_binding: Option<RuntimeAuthorityEffectBinding>,
    output: W,
) -> Result<RunReport> {
    validate_authority_binding_limits(limits, authority_effect_binding.is_some())?;
    let authority_policy = authority_effect_binding
        .map(RuntimeAuthorityEffectBinding::into_policy)
        .unwrap_or_else(RuntimeAuthorityPolicy::admit_all);
    limits.validate()?;
    let program = LoadedProgram::from_artifact(artifact)?;
    let trace_path = path.with_extension("observability.jsonl");
    let trace_file = prepare_trace_file(&trace_path)?;
    let mut host = FilesystemRuntimeHost::new(trace_file, limits.max_trace_bytes, output);
    let runtime_report = run_loaded_program_with_bindings(
        &program,
        &mut host,
        limits,
        composition_binding,
        authority_policy,
    )?;

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
