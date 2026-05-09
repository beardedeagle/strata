use super::support::*;

#[test]
fn runtime_rejects_loaded_action_without_effect_authority_before_emit() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.outputs.push("forbidden output".to_string());
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 uses effect emit without admitted authority",
    );
}

#[test]
fn runtime_rejects_loaded_unused_effect_authority_before_state_update() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 admits effect emit but no action uses it",
    );
}

#[test]
fn runtime_rejects_loaded_duplicate_effect_authority_before_emit() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.outputs.push("forbidden output".to_string());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[
            ArtifactEffect::Emit,
            ArtifactEffect::Emit,
        ]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 admits duplicate effect emit",
    );
}
