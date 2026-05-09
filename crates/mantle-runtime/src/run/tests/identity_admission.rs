use super::support::*;

#[test]
fn runtime_rejects_loaded_invalid_artifact_identity_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.format = "unexpected-format".to_string();

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "loaded artifact format \"unexpected-format\"; expected \"mantle-target-artifact\"",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_schema_version_with_field_name_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.schema_version = "0".to_string();

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "loaded artifact schema_version \"0\"; expected",
    );
}

#[test]
fn runtime_rejects_loaded_control_character_artifact_identity_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.format = "bad\nformat".to_string();

    let err = loaded_admission_error_before_artifact_loaded(&program);

    assert!(
        err.contains("loaded artifact format must be non-empty and contain no control characters")
    );
    assert!(err.contains("\"bad\\nformat\""));
    assert!(!err.contains("bad\nformat"));
}

#[test]
fn runtime_rejects_loaded_oversized_artifact_identity_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.schema_version = "x".repeat(MAX_FIELD_VALUE_BYTES + 1);

    let err = loaded_admission_error_before_artifact_loaded(&program);

    assert!(err.contains("loaded artifact schema_version exceeds maximum length"));
    assert!(!err.contains(&"x".repeat(256)));
}

#[test]
fn runtime_rejects_loaded_control_character_process_name_before_duplicate_check() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].debug_name = "bad\nprocess".to_string();
    program.processes[1].debug_name = "bad\nprocess".to_string();

    let err = loaded_admission_error_before_artifact_loaded(&program);

    assert!(err.contains("process debug_name must be an identifier"));
    assert!(err.contains("\"bad\\nprocess\""));
    assert!(!err.contains("bad\nprocess"));
    assert!(!err.contains("duplicate loaded process debug_name"));
}

#[test]
fn runtime_rejects_loaded_control_character_state_type_before_mismatch_diagnostic() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[MAIN_STATE.index()].label = "Bad\nState".to_string();

    let err = loaded_admission_error_before_artifact_loaded(&program);

    assert!(err.contains("type.0.label must be an identifier"));
    assert!(err.contains("\"Bad\\nState\""));
    assert!(!err.contains("Bad\nState"));
    assert!(!err.contains("loaded state value MainState has type"));
}

#[test]
fn runtime_rejects_loaded_invalid_output_text_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.outputs.push(String::new());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "loaded output must be non-empty and contain no control characters",
    );
}
