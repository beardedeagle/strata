pub(super) use super::super::*;
pub(super) use std::fs;
pub(super) use std::path::{Path, PathBuf};
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

#[cfg(unix)]
pub(super) fn create_fifo(path: &Path) {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test fifo should be created");
}

pub(super) fn valid_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "actor_ping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::value("MainMsg"),
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
            ArtifactType::value("WorkerMsg"),
            ArtifactType::value("Job"),
            ArtifactType::value("OtherJob"),
            ArtifactType::value("Box"),
            ArtifactType::value("MainPayload"),
            ArtifactType::process_ref("ProcessRef_Main", ProcessId::new(0)),
            ArtifactType::process_ref("ProcessRef_Worker", ProcessId::new(1)),
        ],
        outputs: vec!["worker handled Ping".to_string()],
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
                    payload_guard: None,
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
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

pub(super) fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values.iter().map(|value| state_value(ty, value)).collect()
}

pub(super) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

pub(super) fn state_value(ty: TypeId, value: &str) -> ArtifactStateValue {
    ArtifactStateValue::new(ty, artifact_value(value)).expect("test state value should be valid")
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
