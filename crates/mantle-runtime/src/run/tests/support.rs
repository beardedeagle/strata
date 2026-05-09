pub(super) use super::super::model::RuntimeMessageEnvelope;
pub(super) use super::super::*;
pub(super) use crate::host::InMemoryRuntimeHost;
pub(super) use crate::limits::{
    DEFAULT_MAX_EMITTED_OUTPUT_BYTES, DEFAULT_MAX_RUNTIME_PROCESSES, DEFAULT_MAX_TRACE_BYTES,
    RunLimits,
};
pub(super) use crate::program::LoadedProgram;
pub(super) use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactEffect, ArtifactMessageVariant,
    ArtifactPayload, ArtifactProcess, ArtifactProcessRef, ArtifactProcessRefPayload,
    ArtifactStateValue, ArtifactTransition, ArtifactType, ArtifactValueTemplateField,
    MAX_FIELD_VALUE_BYTES, MAX_PROCESS_REFS_PER_PROCESS, MAX_VALUE_TEMPLATE_DEPTH, MantleArtifact,
    MessageId, NextState, OutputId, ProcessId, ProcessRefId, StateId, StepResult, TypeId,
};

pub(super) const TEST_SOURCE_LANGUAGE: &str = "test_frontend";
pub(super) const MAIN_STATE: TypeId = TypeId::new(0);
pub(super) const MAIN_MSG: TypeId = TypeId::new(1);
pub(super) const WORKER_STATE: TypeId = TypeId::new(2);
pub(super) const WORKER_MSG: TypeId = TypeId::new(3);
pub(super) const JOB: TypeId = TypeId::new(4);
pub(super) const BOX: TypeId = TypeId::new(5);
pub(super) const LEAF: TypeId = TypeId::new(6);
pub(super) const START_PAYLOAD: TypeId = TypeId::new(7);
pub(super) const PROCESS_REF_WORKER: TypeId = TypeId::new(8);

pub(super) fn assert_loaded_admission_rejects_before_artifact_loaded(
    program: &LoadedProgram,
    expected: &str,
) {
    let err = loaded_admission_error_before_artifact_loaded(program);

    assert!(
        err.contains(expected),
        "expected error containing {expected:?}, got {err}"
    );
}

pub(super) fn loaded_admission_error_before_artifact_loaded(program: &LoadedProgram) -> String {
    let mut host = InMemoryRuntimeHost::default();

    let err = run_loaded_program_with_host(program, &mut host, RunLimits::default())
        .expect_err("loaded runtime admission should fail closed");
    let err = err.to_string();

    assert!(
        host.stdout().is_empty(),
        "loaded runtime admission failure must happen before host output"
    );
    assert!(
        host.events().is_empty(),
        "loaded runtime admission failure must happen before ArtifactLoaded"
    );
    err
}

pub(super) fn artifact_with_unbound_worker_process_ref() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "unbound_worker_process_ref".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::value("MainMsg"),
            ArtifactType::value("WorkerState"),
            ArtifactType::value("WorkerMsg"),
            ArtifactType::value("Job"),
            ArtifactType::value("Box"),
            ArtifactType::value("Leaf"),
            ArtifactType::value("StartPayload"),
            ArtifactType::process_ref("ProcessRef_Worker", ProcessId::new(1)),
            ArtifactType::process_ref("ProcessRef_Main", ProcessId::new(0)),
        ],
        outputs: Vec::new(),
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: MAIN_STATE,
                state_values: state_values(MAIN_STATE, &["MainState"]),
                message_type: MAIN_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: Vec::new(),
                    actions: Vec::new(),
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: WORKER_STATE,
                state_values: state_values(WORKER_STATE, &["Idle"]),
                message_type: WORKER_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                process_refs: Vec::new(),
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: Vec::new(),
                    actions: Vec::new(),
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

pub(super) fn artifact_with_large_unbound_process_ref_table() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.module = "large_process_ref_table".to_string();
    artifact.processes[0].process_refs = (0..MAX_PROCESS_REFS_PER_PROCESS)
        .map(|index| ArtifactProcessRef {
            debug_name: format!("worker_{index}"),
            target: ProcessId::new(1),
        })
        .collect();
    artifact
}

pub(super) fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values
        .iter()
        .map(|value| ArtifactStateValue::new(ty, *value))
        .collect()
}

pub(super) fn record_template_with_depth(depth: usize) -> ArtifactValueTemplate {
    let mut template = ArtifactValueTemplate::Literal {
        ty: LEAF,
        value: "Leaf".to_string(),
    };
    for _ in 0..depth {
        template = ArtifactValueTemplate::Record {
            ty: BOX,
            fields: vec![ArtifactValueTemplateField {
                name: "item".to_string(),
                value: template,
            }],
        };
    }
    match &mut template {
        ArtifactValueTemplate::Record { ty, .. } => *ty = MAIN_STATE,
        _ => unreachable!("depth above zero produces a record"),
    }
    template
}
