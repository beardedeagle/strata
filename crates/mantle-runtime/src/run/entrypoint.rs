use mantle_artifact::{Error, MantleArtifact, Result};

use super::RuntimeRun;
use super::model::RuntimeMessageEnvelope;
use crate::event::RuntimeEvent;
use crate::executable::ExecutableProgram;
use crate::host::RuntimeHost;
use crate::limits::{RunLimits, SpawnAuthorityPolicy};
use crate::program::LoadedProgram;
use crate::report::{ProcessReport, RuntimeReport};
use crate::{RuntimeAuthorityEffectBinding, RuntimeAuthorityPolicy, RuntimeCompositionBinding};

pub fn run_artifact_with_host<H: RuntimeHost>(
    artifact: &MantleArtifact,
    host: &mut H,
    limits: RunLimits,
) -> Result<RuntimeReport> {
    limits.validate()?;
    let program = LoadedProgram::from_artifact(artifact)?;
    run_loaded_program_with_host(&program, host, limits)
}

pub fn run_artifact_with_host_and_binding_texts<H: RuntimeHost>(
    artifact: &MantleArtifact,
    host: &mut H,
    limits: RunLimits,
    composition_binding_text: Option<&str>,
    authority_effect_binding_text: Option<&str>,
) -> Result<RuntimeReport> {
    validate_authority_binding_limits(limits, authority_effect_binding_text.is_some())?;
    limits.validate()?;
    let program = LoadedProgram::from_artifact(artifact)?;
    let composition_binding = composition_binding_text
        .map(|text| RuntimeCompositionBinding::decode_text(text, artifact))
        .transpose()?;
    let authority_policy = authority_effect_binding_text
        .map(|text| RuntimeAuthorityEffectBinding::decode_text(text, artifact))
        .transpose()?
        .map(RuntimeAuthorityEffectBinding::into_policy)
        .unwrap_or_else(RuntimeAuthorityPolicy::admit_all);
    run_loaded_program_with_bindings(
        &program,
        host,
        limits,
        composition_binding,
        authority_policy,
    )
}

pub(crate) fn run_loaded_program_with_host<H: RuntimeHost>(
    program: &LoadedProgram,
    host: &mut H,
    limits: RunLimits,
) -> Result<RuntimeReport> {
    run_loaded_program_with_composition_binding(program, host, limits, None)
}

pub(crate) fn run_loaded_program_with_composition_binding<H: RuntimeHost>(
    program: &LoadedProgram,
    host: &mut H,
    limits: RunLimits,
    composition_binding: Option<RuntimeCompositionBinding>,
) -> Result<RuntimeReport> {
    run_loaded_program_with_bindings(
        program,
        host,
        limits,
        composition_binding,
        RuntimeAuthorityPolicy::admit_all(),
    )
}

pub(crate) fn validate_authority_binding_limits(
    limits: RunLimits,
    has_authority_effect_binding: bool,
) -> Result<()> {
    if has_authority_effect_binding
        && limits.spawn_authority_policy != SpawnAuthorityPolicy::AdmitDeclared
    {
        return Err(Error::new(
            "RunLimits spawn_authority_policy cannot be combined with an authority/effect binding; encode the policy in an authority policy artifact before binding",
        ));
    }
    Ok(())
}

pub(crate) fn run_loaded_program_with_bindings<H: RuntimeHost>(
    program: &LoadedProgram,
    host: &mut H,
    limits: RunLimits,
    composition_binding: Option<RuntimeCompositionBinding>,
    authority_policy: RuntimeAuthorityPolicy,
) -> Result<RuntimeReport> {
    let executable = ExecutableProgram::from_admitted(program)?;
    let mut run = RuntimeRun::new_with_composition_binding(
        program,
        &executable,
        host,
        limits,
        composition_binding,
        authority_policy,
    );
    let entry = executable.entry();
    run.record_event(RuntimeEvent::ArtifactLoaded {
        format: program.format.clone(),
        schema_version: program.schema_version.clone(),
        source_language: program.source_language.clone(),
        module: program.module.clone(),
        entry_process_id: entry.process_id,
        entry_process: entry.process_label.to_string(),
        entry_message_id: entry.message_id,
        process_count: run.executable.process_count(),
    })?;
    let entry_pid = run.spawn_process(entry.process_id, None)?;
    run.send_message(
        entry_pid,
        RuntimeMessageEnvelope::new(entry.message_id, None),
        None,
    )?;
    run.drain_mailboxes(limits.max_dispatches)?;
    run.reject_unhandled_messages()?;
    run.flush_host()?;

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

    Ok(RuntimeReport {
        entry_process: entry.process_label.to_string(),
        entry_message: entry.message_label.to_string(),
        spawned_processes: run.spawned_processes,
        delivered_messages: run.delivered_messages,
        processes: process_reports,
        emitted_outputs: run.emitted_outputs,
    })
}
