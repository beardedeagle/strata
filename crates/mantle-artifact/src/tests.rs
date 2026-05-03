use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn artifact_round_trips_and_validates_magic() {
    let artifact = valid_artifact();
    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("schema_version=1"));
    assert!(encoded.contains("entry_process=0"));
    assert!(encoded.contains("process.0.transition.0.next_state=current"));
    assert!(encoded.contains("process.1.transition.0.next_state=value"));
    assert!(encoded.contains("process.1.transition.0.next_state_value=1"));
    assert!(encoded.contains("process.0.transition.0.action.0.target_process=1"));

    let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
    assert!(err.to_string().contains("invalid Mantle artifact magic"));
}

#[test]
fn decode_rejects_unsupported_schema_before_body_fields() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version=0\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unsupported schema should fail first");

    assert!(err
        .to_string()
        .contains("unsupported artifact schema version 0; expected 1"));
}

#[test]
fn decode_reports_duplicate_fields() {
    let encoded = valid_artifact().encode().replace(
        "process.0.debug_name=Main",
        "process.0.debug_name=Main\nprocess.0.debug_name=Other",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("duplicate field should fail");

    assert!(err
        .to_string()
        .contains("duplicate artifact field \"process.0.debug_name\""));
}

#[test]
fn decode_reports_unknown_fields() {
    let mut encoded = valid_artifact().encode();
    encoded.push_str("process.0.transition.0.action.0.extra=value\n");

    let err = MantleArtifact::decode(&encoded).expect_err("unknown field should fail");

    assert!(err
        .to_string()
        .contains("unknown artifact field \"process.0.transition.0.action.0.extra\""));
}

#[test]
fn decode_rejects_unbounded_process_count_before_allocation() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version={ARTIFACT_SCHEMA_VERSION}\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("process count should be bounded");

    assert!(err
        .to_string()
        .contains("process_count must be no greater than"));
}

#[test]
fn decode_rejects_unbounded_nested_counts_before_allocation() {
    let encoded = valid_artifact().encode().replace(
        "process.0.state_value_count=1",
        &format!(
            "process.0.state_value_count={}",
            MAX_STATE_VALUES_PER_PROCESS + 1
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("state value count should be bounded");

    assert!(err
        .to_string()
        .contains("process.0.state_value_count must be no greater than"));
}

#[test]
fn validate_accepts_non_strata_source_language() {
    let mut artifact = valid_artifact();
    artifact.source_language = "lattice".to_string();

    artifact
        .validate()
        .expect("artifact source language should be language-neutral");

    let decoded = MantleArtifact::decode(&artifact.encode())
        .expect("language-neutral artifact should decode");
    assert_eq!(decoded.source_language, "lattice");
}

#[test]
fn validate_rejects_invalid_source_language_identifier() {
    let mut artifact = valid_artifact();
    artifact.source_language = "not-valid".to_string();

    let err = artifact
        .validate()
        .expect_err("invalid source language should fail");

    assert!(err
        .to_string()
        .contains("artifact field source_language must be an identifier"));
}

#[test]
fn validate_accepts_structured_state_value_labels() {
    let mut artifact = valid_artifact();
    artifact.processes[0].state_values = vec![
        "MainState{phase:Idle}".to_string(),
        "MainState{phase:Handled}".to_string(),
    ];
    artifact.processes[0].transitions[0].next_state = NextState::Value(StateId::new(1));

    artifact
        .validate()
        .expect("structured state labels should remain display metadata");

    let decoded =
        MantleArtifact::decode(&artifact.encode()).expect("structured labels should decode");
    assert_eq!(
        decoded.processes[0].state_values,
        artifact.processes[0].state_values
    );
}

#[test]
fn validate_state_value_label_defines_artifact_metadata_boundary() {
    validate_state_value_label("MainState{phase:Idle}")
        .expect("structured state labels should be valid artifact metadata");

    for (value, expected) in [
        (
            "",
            "state values must be non-empty and contain no control characters",
        ),
        (
            "MainState\n",
            "state values must be non-empty and contain no control characters",
        ),
    ] {
        let err = validate_state_value_label(value).expect_err("invalid label should fail");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }

    let oversized = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let err = validate_state_value_label(&oversized).expect_err("oversized label should fail");
    assert!(err
        .to_string()
        .contains("state value exceeds maximum length"));
}

#[test]
fn validate_rejects_encoded_artifacts_above_size_limit() {
    let mut artifact = valid_artifact();
    let text = "a".repeat(MAX_FIELD_VALUE_BYTES);
    artifact.outputs = (0..70).map(|_| text.clone()).collect();

    let err = artifact
        .validate()
        .expect_err("encoded artifact size should be bounded");

    assert!(err
        .to_string()
        .contains("encoded artifact exceeds maximum size"));
}

#[test]
fn validate_rejects_aggregate_process_action_count_above_limit() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push("Pong".to_string());
    artifact.processes[1].transitions[0].actions = emit_actions(MAX_ACTIONS_PER_PROCESS / 2);
    artifact.processes[1].transitions.push(ArtifactTransition {
        message: MessageId::new(1),
        step_result: StepResult::Stop,
        next_state: NextState::Current,
        actions: emit_actions((MAX_ACTIONS_PER_PROCESS / 2) + 1),
    });

    let err = artifact
        .validate()
        .expect_err("aggregate process action count should be bounded");

    assert!(err.to_string().contains(&format!(
        "action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
}

#[test]
fn validate_rejects_unknown_send_message() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ProcessId::new(1),
            message: MessageId::new(1),
        });

    let err = artifact
        .validate()
        .expect_err("unknown send message should fail");

    assert!(err
        .to_string()
        .contains("sends message id 1 not accepted by process id 1"));
}

#[test]
fn validate_rejects_unknown_spawn_target() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Spawn {
            target: ProcessId::new(99),
        });

    let err = artifact
        .validate()
        .expect_err("unknown spawn target should fail");

    assert!(err.to_string().contains("spawns undefined process id 99"));
}

#[test]
fn validate_rejects_unknown_output_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::Emit {
        output: OutputId::new(99),
    }];

    let err = artifact
        .validate()
        .expect_err("unknown output id should fail");

    assert!(err.to_string().contains("emits undefined output id 99"));
}

#[test]
fn validate_rejects_unknown_entry_process_id() {
    let mut artifact = valid_artifact();
    artifact.entry_process = ProcessId::new(99);

    let err = artifact
        .validate()
        .expect_err("unknown entry process id should fail");

    assert!(err
        .to_string()
        .contains("entry process id 99 is not defined"));
}

#[test]
fn validate_rejects_unknown_next_state_value_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].next_state = NextState::Value(StateId::new(99));

    let err = artifact
        .validate()
        .expect_err("unknown next state value should fail");

    assert!(err
        .to_string()
        .contains("process Worker transition next_state id 99 is not a valid state value"));
}

#[test]
fn validate_rejects_missing_transition_for_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push("Pong".to_string());

    let err = artifact
        .validate()
        .expect_err("missing transition should fail");

    assert!(err
        .to_string()
        .contains("process Worker transition_count must equal message_count"));
}

#[test]
fn validate_rejects_duplicate_transition_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push("Pong".to_string());
    let duplicate = artifact.processes[1].transitions[0].clone();
    artifact.processes[1].transitions.push(duplicate);

    let err = artifact
        .validate()
        .expect_err("duplicate transition should fail");

    assert!(err
        .to_string()
        .contains("process Worker declares duplicate transition for message id 0"));
}

#[test]
fn validate_rejects_unknown_transition_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].message = MessageId::new(1);

    let err = artifact
        .validate()
        .expect_err("unknown transition message should fail");

    assert!(err
        .to_string()
        .contains("process Worker transition message id 1 is not accepted"));
}

#[test]
fn validate_rejects_duplicate_process_debug_names() {
    let mut artifact = valid_artifact();
    artifact.processes[1].debug_name = "Main".to_string();

    let err = artifact
        .validate()
        .expect_err("duplicate debug labels should fail");

    assert!(err
        .to_string()
        .contains("duplicate process debug_name Main"));
}

#[test]
fn validate_treats_debug_names_as_metadata_not_targets() {
    let mut artifact = valid_artifact();
    artifact.processes[1].debug_name = "RenamedWorker".to_string();

    artifact
        .validate()
        .expect("renaming debug metadata should not affect indexed references");
}

#[test]
fn write_artifact_rejects_invalid_artifacts_before_writing() {
    let dir = unique_test_dir("invalid-artifact-write");
    let path = dir.join("bad.mta");
    let mut artifact = valid_artifact();
    artifact.format = "invalid-format".to_string();

    let err = write_artifact(&path, &artifact).expect_err("invalid artifact should fail");

    assert!(err.to_string().contains("unsupported artifact format"));
    assert!(!path.exists(), "invalid artifact must not be written");
    assert!(
        !dir.exists(),
        "invalid artifact must not create parent dirs"
    );
}

#[test]
fn write_artifact_accepts_current_directory_output_path() {
    let path = unique_current_dir_artifact_path("artifact-current-dir");
    let artifact = valid_artifact();

    write_artifact(&path, &artifact).expect("current-directory artifact write should succeed");

    let decoded = read_artifact(&path).expect("written artifact should decode");
    assert_eq!(decoded, artifact);

    fs::remove_file(path).expect("test artifact should be removed");
}

#[test]
fn write_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-write-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");
    let artifact = valid_artifact();

    let err =
        write_artifact(&path, &artifact).expect_err("directory artifact output path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
}

#[cfg(unix)]
#[test]
fn write_artifact_rejects_fifo_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-write-fifo");
    create_fifo(&path);
    let artifact = valid_artifact();

    let err = write_artifact(&path, &artifact).expect_err("fifo artifact output path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[test]
fn read_artifact_rejects_oversized_file() {
    let path = unique_current_dir_artifact_path("artifact-too-large");
    fs::write(&path, vec![b'a'; MAX_ARTIFACT_BYTES + 1])
        .expect("oversized test file should be written");

    let err = read_artifact(&path).expect_err("oversized artifact file should fail");

    assert!(err.to_string().contains("is too large"));

    fs::remove_file(path).expect("test artifact should be removed");
}

#[test]
fn read_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");

    let err = read_artifact(&path).expect_err("directory artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
}

#[cfg(unix)]
#[test]
fn read_artifact_rejects_fifo_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-fifo");
    create_fifo(&path);

    let err = read_artifact(&path).expect_err("fifo artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[cfg(unix)]
fn create_fifo(path: &Path) {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test fifo should be created");
}

fn valid_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
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
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                        },
                        ArtifactAction::Send {
                            target: ProcessId::new(1),
                            message: MessageId::new(0),
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: "WorkerState".to_string(),
                state_values: vec!["Idle".to_string(), "Handled".to_string()],
                message_type: "WorkerMsg".to_string(),
                message_variants: vec!["Ping".to_string()],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Value(StateId::new(1)),
                    actions: vec![ArtifactAction::Emit {
                        output: OutputId::new(0),
                    }],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn emit_actions(count: usize) -> Vec<ArtifactAction> {
    vec![
        ArtifactAction::Emit {
            output: OutputId::new(0)
        };
        count
    ]
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
