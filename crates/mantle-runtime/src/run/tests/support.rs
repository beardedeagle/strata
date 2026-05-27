pub(super) use super::super::model::RuntimeMessageEnvelope;
pub(super) use super::super::*;
pub(super) use crate::host::InMemoryRuntimeHost;
pub(super) use crate::limits::RunLimits;
pub(super) use crate::program::{
    LoadedAuthority, LoadedCapabilityDescriptor, LoadedEffectAuthority, LoadedNextState,
    LoadedProgram, LoadedSendTarget, LoadedSpawnKind, LoadedSpawnSite, LoadedStateValue,
    LoadedValueTemplate, RuntimePayload, RuntimeValue,
};
pub(super) use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAuthority, ArtifactCapabilityDescriptor,
    ArtifactEffect, ArtifactEnumVariant, ArtifactMessageVariant, ArtifactPayload, ArtifactProcess,
    ArtifactProcessRef, ArtifactProcessRefPayload, ArtifactRecordField, ArtifactSpawnKind,
    ArtifactSpawnSite, ArtifactStateValue, ArtifactTransition, ArtifactType, ArtifactTypeField,
    ArtifactValue, ArtifactValueBooleanOperator, ArtifactValueEqualityOperator,
    ArtifactValueTemplate, ArtifactValueTemplateField, AuthorityId, EnumVariantId,
    MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES, MAX_NEXT_STATE_IF_ELSE_DEPTH,
    MAX_PROCESS_REFS_PER_PROCESS, MAX_VALUE_TEMPLATE_DEPTH, MantleArtifact, MessageId, NextState,
    OutputId, ProcessId, ProcessRefId, SpawnSiteId, StateId, StepResult, TypeId,
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
pub(super) const SPAWN_AUTHORITY: AuthorityId = AuthorityId::new(0);
pub(super) const SPAWN_SITE: SpawnSiteId = SpawnSiteId::new(0);

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
            ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
            worker_state_type(&[
                "Idle", "Handled", "Working", "Done", "Routed", "Ready", "Other", "Spoofed",
            ]),
            ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]),
            job_record_type(),
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
                authorities: Vec::new(),
                spawn_sites: Vec::new(),
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    payload_guard: None,
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
                authorities: Vec::new(),
                spawn_sites: Vec::new(),
                process_refs: Vec::new(),
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    payload_guard: None,
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

pub(super) fn spawn_authorities(target: ProcessId) -> Vec<ArtifactAuthority> {
    vec![ArtifactAuthority {
        debug_name: "spawn_target".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn { target },
    }]
}

pub(super) fn spawn_sites(target: ProcessId) -> Vec<ArtifactSpawnSite> {
    vec![ArtifactSpawnSite {
        target,
        authority: SPAWN_AUTHORITY,
        kind: ArtifactSpawnKind::DynamicLocal,
    }]
}

pub(super) fn grant_main_spawn_authority(artifact: &mut MantleArtifact) {
    artifact.processes[0].authorities = spawn_authorities(ProcessId::new(1));
    artifact.processes[0].spawn_sites = spawn_sites(ProcessId::new(1));
}

pub(super) fn grant_loaded_main_spawn_authority(program: &mut LoadedProgram) {
    program.processes[0].authorities = vec![LoadedAuthority {
        debug_name: "spawn_target".to_string(),
        descriptor: LoadedCapabilityDescriptor::Spawn {
            target: ProcessId::new(1),
        },
    }];
    program.processes[0].spawn_sites = vec![LoadedSpawnSite {
        target: ProcessId::new(1),
        authority: SPAWN_AUTHORITY,
        kind: LoadedSpawnKind::DynamicLocal,
    }];
}

pub(super) fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values.iter().map(|value| state_value(ty, value)).collect()
}

pub(super) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

pub(super) fn replace_process_message_variants(
    artifact: &mut MantleArtifact,
    process_index: usize,
    variants: Vec<ArtifactMessageVariant>,
) {
    artifact.processes[process_index].message_variants = variants;
    align_process_message_type(artifact, process_index);
}

pub(super) fn align_process_message_type(artifact: &mut MantleArtifact, process_index: usize) {
    let message_type = artifact.processes[process_index].message_type;
    let label = artifact.types[message_type.index()].label.clone();
    let variants = artifact.processes[process_index]
        .message_variants
        .iter()
        .map(|variant| ArtifactEnumVariant {
            label: variant.label.clone(),
            payload_type: variant.payload_type,
        })
        .collect();
    artifact.types[message_type.index()] = ArtifactType::enum_value_with_payloads(label, variants);
}

pub(super) fn align_loaded_process_message_type(program: &mut LoadedProgram, process_index: usize) {
    let message_type = program.processes[process_index].message_type;
    let label = program.types[message_type.index()].label.clone();
    let variants = program.processes[process_index]
        .message_variants
        .iter()
        .map(|variant| ArtifactEnumVariant {
            label: variant.label.clone(),
            payload_type: variant.payload_type,
        })
        .collect();
    program.types[message_type.index()] = ArtifactType::enum_value_with_payloads(label, variants);
}

pub(super) fn state_value(ty: TypeId, value: &str) -> ArtifactStateValue {
    ArtifactStateValue::new(ty, artifact_value(value)).expect("test state value should be valid")
}

pub(super) fn loaded_state_values(ty: TypeId, values: &[&str]) -> Vec<LoadedStateValue> {
    state_values(ty, values)
        .iter()
        .map(|value| LoadedStateValue::from_artifact(value).expect("test state value should load"))
        .collect()
}

pub(super) fn artifact_type_field(name: &str, ty: TypeId) -> ArtifactTypeField {
    ArtifactTypeField {
        name: name.to_string(),
        ty,
    }
}

pub(super) fn artifact_enum_variant(
    label: &str,
    payload_type: Option<TypeId>,
) -> ArtifactEnumVariant {
    ArtifactEnumVariant {
        label: label.to_string(),
        payload_type,
    }
}

pub(super) fn worker_state_type(variants: &[&str]) -> ArtifactType {
    ArtifactType::enum_value(
        "WorkerState",
        variants
            .iter()
            .map(|variant| (*variant).to_string())
            .collect(),
    )
}

pub(super) fn worker_state_type_with_payloads(variants: &[(&str, Option<TypeId>)]) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "WorkerState",
        variants
            .iter()
            .map(|(label, payload_type)| artifact_enum_variant(label, *payload_type))
            .collect(),
    )
}

pub(super) fn job_record_type() -> ArtifactType {
    ArtifactType::record("Job", vec![artifact_type_field("phase", WORKER_STATE)])
}

pub(super) fn box_record_type(field: &str, ty: TypeId) -> ArtifactType {
    ArtifactType::record("Box", vec![artifact_type_field(field, ty)])
}

pub(super) fn push_map_type(
    program: &mut LoadedProgram,
    label: &str,
    key: TypeId,
    value: TypeId,
    capacity: usize,
) -> TypeId {
    let ty = TypeId::from_index(program.types.len()).expect("test type id should fit");
    program
        .types
        .push(ArtifactType::map(label, key, value, capacity));
    ty
}

pub(super) fn push_list_type(
    program: &mut LoadedProgram,
    label: &str,
    element: TypeId,
    capacity: usize,
) -> TypeId {
    let ty = TypeId::from_index(program.types.len()).expect("test type id should fit");
    program
        .types
        .push(ArtifactType::list(label, element, capacity));
    ty
}

pub(super) fn recursive_main_state_type() -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "MainState",
        vec![
            artifact_enum_variant("Leaf", None),
            artifact_enum_variant("Node", Some(MAIN_STATE)),
        ],
    )
}

pub(super) fn recursive_main_state_template_with_depth(depth: usize) -> ArtifactValueTemplate {
    let mut template = ArtifactValueTemplate::Literal {
        ty: MAIN_STATE,
        value: artifact_value("Leaf"),
    };
    for _ in 0..depth {
        template = ArtifactValueTemplate::EnumVariant {
            ty: MAIN_STATE,
            variant: EnumVariantId::new(1),
            payload: Box::new(template),
        };
    }
    template
}

pub(super) fn loaded_template(template: ArtifactValueTemplate) -> LoadedValueTemplate {
    LoadedValueTemplate::from_artifact(&template).expect("test template should load")
}

pub(super) fn loaded_next_state(next_state: NextState) -> LoadedNextState {
    LoadedNextState::from_artifact(&next_state).expect("test next state should load")
}

pub(super) fn nested_loaded_if_else_next_state(depth: usize, bool_type: TypeId) -> LoadedNextState {
    let condition = LoadedValueTemplate::Literal {
        ty: bool_type,
        value: RuntimeValue::Atom("True".to_string()),
    };
    let mut next_state = LoadedNextState::Value(StateId::new(0));
    for _ in 0..depth {
        next_state = LoadedNextState::IfElse {
            condition: condition.clone(),
            then_state: Box::new(next_state),
            else_state: Box::new(LoadedNextState::Value(StateId::new(0))),
        };
    }
    next_state
}

pub(super) fn runtime_payload(payload: ArtifactPayload) -> RuntimePayload {
    RuntimePayload::from_artifact(&payload).expect("test payload should load")
}
