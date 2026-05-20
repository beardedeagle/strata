use super::super::support::*;

#[test]
fn runtime_rejects_loaded_process_ref_state_type_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_type = PROCESS_REF_WORKER;

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "state_type type id 8 must be a value type",
    );
}

#[test]
fn runtime_rejects_loaded_payload_bearing_entry_message_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].message_variants[0].payload_type = Some(START_PAYLOAD);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "entry message id 0 must not require a payload",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_message_payload_type_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(TypeId::new(99));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message payload_type: loaded type id 99 is not loaded",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_init_state_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].init_state = StateId::new(1);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main init_state id 1 is not a loaded state value",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_state_value_shape_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_values[0].value = RuntimeValue::Atom("not-valid".to_string());

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "artifact field state value must be an identifier",
    );
}

#[test]
fn runtime_rejects_loaded_state_value_label_mismatch_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_values[0].label = "Spoofed".to_string();

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main state value label Spoofed does not match ordered value label MainState",
    );
}

#[test]
fn runtime_rejects_loaded_state_value_outside_declared_enum_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].state_values[0].value = RuntimeValue::Atom("Bogus".to_string());
    program.processes[1].state_values[0].label = "Bogus".to_string();

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker state value: state value value Bogus is not a member of enum type WorkerState",
    );
}
