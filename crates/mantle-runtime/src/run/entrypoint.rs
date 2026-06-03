use mantle_artifact::{MantleArtifact, Result};

use super::RuntimeRun;
use super::model::RuntimeMessageEnvelope;
use crate::RuntimeCompositionBinding;
use crate::event::RuntimeEvent;
use crate::executable::ExecutableProgram;
use crate::host::RuntimeHost;
use crate::limits::RunLimits;
use crate::program::LoadedProgram;
use crate::report::{ProcessReport, RuntimeReport};

pub fn run_artifact_with_host<H: RuntimeHost>(
    artifact: &MantleArtifact,
    host: &mut H,
    limits: RunLimits,
) -> Result<RuntimeReport> {
    limits.validate()?;
    let program = LoadedProgram::from_artifact(artifact)?;
    run_loaded_program_with_host(&program, host, limits)
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
    let executable = ExecutableProgram::from_admitted(program)?;
    let mut run = RuntimeRun::new_with_composition_binding(
        program,
        &executable,
        host,
        limits,
        composition_binding,
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
