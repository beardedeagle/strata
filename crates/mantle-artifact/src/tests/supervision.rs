use super::support::*;

#[test]
fn validate_admits_lexical_supervisor_child_spawn_site() {
    let artifact = artifact_with_lexical_supervisor_child();

    artifact
        .validate()
        .expect("lexical supervisor child plan should validate");
}

#[test]
fn codec_round_trips_lexical_supervisor_child_plan() {
    let artifact = artifact_with_supervisor_child_send_target();
    let encoded = artifact.encode();

    assert!(encoded.contains("process.0.spawn_site.0.kind=lexical_supervisor_child"));
    assert!(encoded.contains("process.0.spawn_site.0.supervisor=0"));
    assert!(encoded.contains("process.0.spawn_site.0.supervisor_child=0"));
    assert!(encoded.contains("process.0.supervisor.0.strategy=one_for_one"));
    assert!(encoded.contains("process.0.supervisor.0.child.0.mode=permanent"));
    assert!(encoded.contains("process.0.transition.0.action.0.target=supervisor_child"));
    assert!(encoded.contains("process.0.transition.0.action.0.target_supervisor=0"));
    assert!(encoded.contains("process.0.transition.0.action.0.target_supervisor_child=0"));

    let decoded = MantleArtifact::decode(&encoded).expect("supervisor artifact should decode");
    assert_eq!(decoded, artifact);
    decoded
        .validate()
        .expect("decoded lexical supervisor child plan should validate");
}

#[test]
fn codec_rejects_invalid_supervisor_child_mode() {
    let encoded = artifact_with_lexical_supervisor_child().encode().replace(
        "process.0.supervisor.0.child.0.mode=permanent",
        "process.0.supervisor.0.child.0.mode=ephemeral",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("invalid child mode should fail decode");

    assert!(
        err.to_string()
            .contains("invalid supervisor child mode value \"ephemeral\""),
        "{err}"
    );
}

#[test]
fn validate_rejects_lexical_supervisor_child_with_dynamic_authority() {
    let mut artifact = artifact_with_lexical_supervisor_child();
    artifact.processes[0].spawn_sites[0].authority = Some(AuthorityId::new(0));

    let err = artifact
        .validate()
        .expect_err("lexical child site must not carry dynamic authority");

    assert!(
        err.to_string()
            .contains("lexical supervisor child spawn site 0 carries dynamic authority"),
        "{err}"
    );
}

#[test]
fn validate_rejects_lexical_supervisor_child_without_supervisor_id() {
    assert_supervisor_child_spawn_site_rejected(
        |site| site.supervisor = None,
        "lexical supervisor child spawn site 0 must carry supervisor and child ids",
    );
}

#[test]
fn validate_rejects_lexical_supervisor_child_without_child_id() {
    assert_supervisor_child_spawn_site_rejected(
        |site| site.child = None,
        "lexical supervisor child spawn site 0 must carry supervisor and child ids",
    );
}

#[test]
fn validate_rejects_supervisor_child_referencing_dynamic_spawn_site() {
    let mut artifact = valid_artifact();
    artifact.processes[0].supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "worker".to_string(),
            target: ProcessId::new(1),
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SPAWN_WORKER_SITE,
        }],
    }];

    let err = artifact
        .validate()
        .expect_err("supervisor child must reference a lexical spawn site");

    assert!(
        err.to_string()
            .contains("supervisor child worker references non-lexical spawn site id 0"),
        "{err}"
    );
}

#[test]
fn validate_rejects_supervisor_child_with_wrong_supervisor_id() {
    assert_supervisor_child_spawn_site_rejected(
        |site| site.supervisor = Some(SupervisorId::new(1)),
        "supervisor child worker spawn site references wrong supervisor id",
    );
}

#[test]
fn validate_rejects_supervisor_child_with_wrong_child_id() {
    assert_supervisor_child_spawn_site_rejected(
        |site| site.child = Some(SupervisorChildId::new(1)),
        "supervisor child worker spawn site references wrong child id",
    );
}

#[test]
fn validate_rejects_invalid_supervisor_restart_intensity() {
    let mut artifact = artifact_with_lexical_supervisor_child();
    artifact.processes[0].supervisor_plans[0]
        .intensity
        .max_restarts = 0;

    let err = artifact
        .validate()
        .expect_err("zero restart intensity should fail");

    assert!(
        err.to_string()
            .contains("supervisor 0 max_restarts must be greater than zero"),
        "{err}"
    );
}

#[test]
fn validate_rejects_zero_supervisor_restart_window() {
    let mut artifact = artifact_with_lexical_supervisor_child();
    artifact.processes[0].supervisor_plans[0]
        .intensity
        .within_ms = 0;

    let err = artifact
        .validate()
        .expect_err("zero restart window should fail");

    assert!(
        err.to_string()
            .contains("supervisor 0 within_ms must be greater than zero"),
        "{err}"
    );
}

#[test]
fn validate_rejects_indirect_supervisor_cycle() {
    let mut artifact = artifact_with_lexical_supervisor_child();
    artifact.processes[1].spawn_sites = vec![ArtifactSpawnSite {
        target: ProcessId::new(2),
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    }];
    artifact.processes[1].supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "helper".to_string(),
            target: ProcessId::new(2),
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SpawnSiteId::new(0),
        }],
    }];

    let mut helper = artifact.processes[1].clone();
    helper.debug_name = "Helper".to_string();
    helper.spawn_sites = vec![ArtifactSpawnSite {
        target: ProcessId::new(1),
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    }];
    helper.supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "worker".to_string(),
            target: ProcessId::new(1),
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SpawnSiteId::new(0),
        }],
    }];
    artifact.processes.push(helper);

    let err = artifact
        .validate()
        .expect_err("indirect supervisor cycle should fail admission");

    assert!(
        err.to_string()
            .contains("local supervisor graph contains cycle Worker -> Helper -> Worker"),
        "{err}"
    );
}

fn assert_supervisor_child_spawn_site_rejected(
    mutate: impl FnOnce(&mut ArtifactSpawnSite),
    expected: &str,
) {
    let mut artifact = artifact_with_lexical_supervisor_child();
    mutate(&mut artifact.processes[0].spawn_sites[0]);

    let err = artifact
        .validate()
        .expect_err("mutated supervisor child spawn site should fail validation");

    assert!(err.to_string().contains(expected), "{err}");
}

fn artifact_with_lexical_supervisor_child() -> MantleArtifact {
    let mut artifact = valid_artifact();
    let main = &mut artifact.processes[0];
    main.authorities = Vec::new();
    main.process_refs = Vec::new();
    main.spawn_sites = vec![ArtifactSpawnSite {
        target: ProcessId::new(1),
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    }];
    main.supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "worker".to_string(),
            target: ProcessId::new(1),
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SpawnSiteId::new(0),
        }],
    }];
    main.transitions[0].effects = Vec::new();
    main.transitions[0].actions = Vec::new();
    artifact
}

fn artifact_with_supervisor_child_send_target() -> MantleArtifact {
    let mut artifact = artifact_with_lexical_supervisor_child();
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Send];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::Send {
        target: ArtifactSendTarget::SupervisorChild {
            supervisor: SupervisorId::new(0),
            child: SupervisorChildId::new(0),
            target_process: ProcessId::new(1),
        },
        port: None,
        message: MessageId::new(0),
        payload: None,
    }];
    artifact
}
