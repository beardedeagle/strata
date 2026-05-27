use super::support::*;

#[test]
fn validate_rejects_duplicate_process_ref_name() {
    let mut artifact = valid_artifact();
    artifact.processes[0].process_refs.push(ArtifactProcessRef {
        debug_name: "worker".to_string(),
        target: ProcessId::new(1),
    });

    let err = artifact
        .validate()
        .expect_err("duplicate process reference name should fail");

    assert!(
        err.to_string()
            .contains("duplicate process reference worker")
    );
}

#[test]
fn validate_rejects_process_ref_targeting_entry_process() {
    let mut artifact = valid_artifact();
    artifact.processes[1].process_refs = vec![ArtifactProcessRef {
        debug_name: "main".to_string(),
        target: ProcessId::new(0),
    }];

    let err = artifact
        .validate()
        .expect_err("process reference targeting entry process should fail");

    assert!(
        err.to_string()
            .contains("process Worker process reference main targets entry process id 0")
    );
}

#[test]
fn validate_rejects_process_ref_targeting_same_process() {
    let mut artifact = valid_artifact();
    artifact.processes[1].process_refs = vec![ArtifactProcessRef {
        debug_name: "self_ref".to_string(),
        target: ProcessId::new(1),
    }];

    let err = artifact
        .validate()
        .expect_err("process reference targeting same process should fail");

    assert!(
        err.to_string()
            .contains("process Worker process reference self_ref targets itself")
    );
}

#[test]
fn validate_rejects_spawn_process_ref_target_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions[0] = ArtifactAction::Spawn {
        target: ProcessId::new(0),
        process_ref: ProcessRefId::new(0),
        spawn_site: SPAWN_WORKER_SITE,
    };

    let err = artifact
        .validate()
        .expect_err("spawn process reference target mismatch should fail");

    assert!(
        err.to_string()
            .contains("spawn process reference id 0 targets process id 0, expected 1")
    );
}

#[test]
fn validate_rejects_duplicate_spawn_process_ref_with_transition_context() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_WORKER_SITE,
        });

    let err = artifact
        .validate()
        .expect_err("duplicate spawn process reference should fail");

    assert!(
        err.to_string()
            .contains("duplicates process reference id 0 within message transition 0")
    );
}

#[test]
fn validate_rejects_send_before_process_ref_spawn_with_transition_context() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions.reverse();

    let err = artifact
        .validate()
        .expect_err("send before process reference spawn should fail");

    assert!(err.to_string().contains(
        "process Main sends through unbound process reference id 0 within message transition 0"
    ));
}
