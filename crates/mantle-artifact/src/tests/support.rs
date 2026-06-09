pub(super) use super::super::*;
pub(super) use std::fs;
#[cfg(unix)]
pub(super) use std::path::Path;
pub(super) use std::path::PathBuf;
pub(super) use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const TEST_SOURCE_LANGUAGE: &str = "test_frontend";
pub(super) const MAIN_STATE: TypeId = TypeId::new(0);
pub(super) const MAIN_MSG: TypeId = TypeId::new(1);
pub(super) const WORKER_STATE: TypeId = TypeId::new(2);
pub(super) const WORKER_MSG: TypeId = TypeId::new(3);
pub(super) const JOB: TypeId = TypeId::new(4);
pub(super) const OTHER_JOB: TypeId = TypeId::new(5);
pub(super) const BOX: TypeId = TypeId::new(6);
pub(super) const MAIN_PAYLOAD: TypeId = TypeId::new(7);
pub(super) const PROCESS_REF_MAIN: TypeId = TypeId::new(8);
pub(super) const PROCESS_REF_WORKER: TypeId = TypeId::new(9);
pub(super) const SPAWN_WORKER_AUTHORITY: AuthorityId = AuthorityId::new(0);
pub(super) const SPAWN_WORKER_SITE: SpawnSiteId = SpawnSiteId::new(0);

#[cfg(unix)]
pub(super) fn create_fifo(path: &Path) {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test fifo should be created");
}

pub(super) fn valid_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.into(),
        schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        source_language: TEST_SOURCE_LANGUAGE.into(),
        target_requirements: test_target_requirements(),
        module: "actor_ping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
            ArtifactType::enum_value(
                "WorkerState",
                vec![
                    "Idle".to_string(),
                    "Handled".to_string(),
                    "Working".to_string(),
                    "Done".to_string(),
                    "Routed".to_string(),
                ],
            ),
            ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]),
            ArtifactType::value("Job"),
            ArtifactType::value("OtherJob"),
            ArtifactType::value("Box"),
            ArtifactType::value("MainPayload"),
            ArtifactType::process_ref("ProcessRef_Main", ProcessId::new(0)),
            ArtifactType::process_ref("ProcessRef_Worker", ProcessId::new(1)),
        ],
        outputs: vec!["worker handled Ping".to_string()],
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: MAIN_STATE,
                state_values: state_values(MAIN_STATE, &["MainState"]),
                message_type: MAIN_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                authorities: vec![ArtifactAuthority {
                    debug_name: "spawn_worker".to_string(),
                    descriptor: ArtifactCapabilityDescriptor::Spawn {
                        target: ProcessId::new(1),
                    },
                }],
                spawn_sites: vec![ArtifactSpawnSite {
                    target: ProcessId::new(1),
                    authority: Some(SPAWN_WORKER_AUTHORITY),
                    supervisor: None,
                    child: None,
                    kind: ArtifactSpawnKind::DynamicLocal,
                }],
                supervisor_plans: Vec::new(),
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
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                            spawn_site: SPAWN_WORKER_SITE,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: WORKER_STATE,
                state_values: state_values(WORKER_STATE, &["Idle", "Handled"]),
                message_type: WORKER_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                authorities: Vec::new(),
                spawn_sites: Vec::new(),
                supervisor_plans: Vec::new(),
                process_refs: Vec::new(),
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    payload_guard: None,
                    step_result: StepResult::Stop,
                    next_state: NextState::Value(StateId::new(1)),
                    effects: vec![ArtifactEffect::Emit],
                    actions: vec![ArtifactAction::Emit {
                        output: OutputId::new(0),
                    }],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

pub(super) fn test_target_requirements() -> ArtifactTargetRequirements {
    ArtifactTargetRequirements::new(
        TEST_SOURCE_LANGUAGE,
        vec![
            RuntimeFeature::BoundedMailbox,
            RuntimeFeature::ComponentCompositionMetadata,
            RuntimeFeature::EmitEffect,
            RuntimeFeature::JsonlTrace,
            RuntimeFeature::LocalExecution,
            RuntimeFeature::LocalSend,
            RuntimeFeature::LocalSpawn,
            RuntimeFeature::LocalSupervision,
            RuntimeFeature::RuntimeBranching,
            RuntimeFeature::RuntimeForEach,
            RuntimeFeature::ScalarValueTemplates,
            RuntimeFeature::TypedBoundaryTables,
            RuntimeFeature::TypedEffectOutcomes,
            RuntimeFeature::TypedValueTemplates,
        ],
    )
}

pub(super) fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values.iter().map(|value| state_value(ty, value)).collect()
}

pub(super) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

pub(super) fn state_value(ty: TypeId, value: &str) -> ArtifactStateValue {
    ArtifactStateValue::new(ty, artifact_value(value)).expect("test state value should be valid")
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

pub(super) fn artifact_payload(ty: TypeId, value: &str) -> ArtifactPayload {
    ArtifactPayload::value(ty, artifact_value(value))
        .expect("test artifact payload should be valid")
}

pub(super) fn append_bool_type(artifact: &mut MantleArtifact) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    ty
}

pub(super) fn append_primitive_type(
    artifact: &mut MantleArtifact,
    label: &str,
    primitive: ArtifactPrimitiveType,
) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact
        .types
        .push(ArtifactType::primitive(label, primitive));
    ty
}

pub(super) fn append_list_type(
    artifact: &mut MantleArtifact,
    label: &str,
    element: TypeId,
    capacity: usize,
) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact
        .types
        .push(ArtifactType::list(label, element, capacity));
    ty
}

pub(super) fn append_map_type(
    artifact: &mut MantleArtifact,
    label: &str,
    key: TypeId,
    value: TypeId,
    capacity: usize,
) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact
        .types
        .push(ArtifactType::map(label, key, value, capacity));
    ty
}

pub(super) fn declare_job_record_types(artifact: &mut MantleArtifact) {
    artifact.types[WORKER_STATE.index()] = ArtifactType::enum_value_with_payloads(
        "WorkerState",
        vec![
            ArtifactEnumVariant {
                label: "Idle".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Handled".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Working".to_string(),
                payload_type: Some(JOB),
            },
            ArtifactEnumVariant {
                label: "Done".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Routed".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Ready".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Other".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Spoofed".to_string(),
                payload_type: None,
            },
        ],
    );
    artifact.types[JOB.index()] = ArtifactType::record(
        "Job",
        vec![ArtifactTypeField {
            name: "phase".to_string(),
            ty: WORKER_STATE,
        }],
    );
    artifact.types[OTHER_JOB.index()] = ArtifactType::record(
        "OtherJob",
        vec![ArtifactTypeField {
            name: "phase".to_string(),
            ty: WORKER_STATE,
        }],
    );
}

pub(super) fn nested_if_else_action(depth: usize, bool_type: TypeId) -> ArtifactAction {
    let condition = ArtifactValueTemplate::Literal {
        ty: bool_type,
        value: artifact_value("True"),
    };
    let mut action = ArtifactAction::Emit {
        output: OutputId::new(0),
    };
    for _ in 0..depth {
        action = ArtifactAction::IfElse {
            condition: condition.clone(),
            then_actions: vec![action],
            else_actions: Vec::new(),
        };
    }
    action
}

pub(super) fn nested_if_else_next_state(depth: usize, bool_type: TypeId) -> NextState {
    let condition = ArtifactValueTemplate::Literal {
        ty: bool_type,
        value: artifact_value("True"),
    };
    let mut next_state = NextState::Value(StateId::new(0));
    for _ in 0..depth {
        next_state = NextState::IfElse {
            condition: condition.clone(),
            then_state: Box::new(next_state),
            else_state: Box::new(NextState::Value(StateId::new(0))),
        };
    }
    next_state
}

pub(super) fn emit_actions(count: usize) -> Vec<ArtifactAction> {
    vec![
        ArtifactAction::Emit {
            output: OutputId::new(0)
        };
        count
    ]
}

pub(super) fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(unique_artifact_name(name))
}

pub(super) fn unique_current_dir_artifact_path(name: &str) -> PathBuf {
    PathBuf::from(unique_artifact_name(name))
}

pub(super) fn unique_artifact_name(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    format!("mantle-{name}-{}-{nanos}.mta", std::process::id())
}
