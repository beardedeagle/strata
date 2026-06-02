use super::support::*;

#[test]
fn local_supervision_restarts_permanent_child_with_new_pid() {
    let gate = GateHarness::new();
    let stem = "local_supervision_restart";
    let artifact_path = "target/strata/local_supervision_restart.mta";
    gate.remove_trace(stem);
    gate.check_build_run("examples/local_supervision_restart.str", artifact_path);

    let artifact = gate.read_artifact(artifact_path);
    let main = artifact_process(&artifact, "Main");
    assert_eq!(main.supervisor_plans.len(), 1);
    assert_eq!(
        main.spawn_sites[0].kind,
        ArtifactSpawnKind::LexicalSupervisorChild
    );
    assert_eq!(main.spawn_sites[0].authority, None);

    let trace = gate.read_trace(stem);
    assert!(trace.contains(r#""event":"supervisor_child_started""#));
    assert!(trace.contains(r#""spawn_kind":"lexical_supervisor_child""#));
    assert!(trace.contains(r#""event":"process_failed","pid":2"#));
    assert!(trace.contains(r#""event":"supervisor_restart_decision""#));
    assert!(trace.contains(r#""decision":"restarted""#));
    assert!(trace.contains(r#""new_child_pid":3"#));
    assert_eq!(trace.matches(r#""event":"message_dequeued""#).count(), 2);
}

#[test]
fn local_supervision_permanent_child_restarts_after_normal_stop() {
    let gate = GateHarness::new();
    let stem = "local_supervision_permanent_stop";
    gate.remove_trace(stem);
    gate.check_build_run(
        "examples/local_supervision_permanent_stop.str",
        "target/strata/local_supervision_permanent_stop.mta",
    );

    let trace = gate.read_trace(stem);
    assert!(trace.contains(r#""event":"process_stopped","pid":2"#));
    assert!(trace.contains(r#""reason":"normal","decision":"restarted""#));
    assert!(trace.contains(r#""new_child_pid":3"#));
}

#[test]
fn local_supervision_temporary_child_is_not_restarted() {
    let gate = GateHarness::new();
    let stem = "local_supervision_temporary";
    gate.remove_trace(stem);
    gate.check_build_run(
        "examples/local_supervision_temporary.str",
        "target/strata/local_supervision_temporary.mta",
    );

    let trace = gate.read_trace(stem);
    assert!(trace.contains(r#""event":"process_failed","pid":2"#));
    assert!(trace.contains(r#""decision":"not_restarted""#));
    assert!(!trace.contains(r#""new_child_pid":3"#));
}

#[test]
fn local_supervision_transient_child_restarts_after_crash() {
    let gate = GateHarness::new();
    let stem = "local_supervision_transient_restart";
    gate.remove_trace(stem);
    gate.check_build_run(
        "examples/local_supervision_transient_restart.str",
        "target/strata/local_supervision_transient_restart.mta",
    );

    let trace = gate.read_trace(stem);
    assert!(trace.contains(r#""event":"process_failed","pid":2"#));
    assert!(trace.contains(r#""decision":"restarted""#));
    assert!(trace.contains(r#""new_child_pid":3"#));
}

#[test]
fn local_supervision_transient_child_does_not_restart_after_normal_stop() {
    let gate = GateHarness::new();
    let stem = "local_supervision_transient";
    gate.remove_trace(stem);
    gate.check_build_run(
        "examples/local_supervision_transient.str",
        "target/strata/local_supervision_transient.mta",
    );

    let trace = gate.read_trace(stem);
    assert!(trace.contains(r#""event":"process_stopped","pid":2"#));
    assert!(trace.contains(r#""decision":"not_restarted""#));
    assert!(!trace.contains(r#""decision":"restarted""#));
}

#[test]
fn local_supervision_inactive_child_send_outcome_returns_stopped() {
    let gate = GateHarness::new();
    let stem = "local_supervision_inactive_send_outcome";
    let artifact_path = "target/strata/local_supervision_inactive_send_outcome.mta";
    gate.remove_trace(stem);
    let run = gate.check_build_run(
        "examples/local_supervision_inactive_send_outcome.str",
        artifact_path,
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("inactive child stopped"));

    let artifact = gate.read_artifact(artifact_path);
    let supervisor = artifact_process(&artifact, "Supervisor");
    assert!(supervisor.transitions.iter().any(|transition| {
        transition.actions.iter().any(|action| {
            matches!(
                action,
                ArtifactAction::SendOutcome {
                    target: ArtifactSendTarget::SupervisorChild {
                        target_process,
                        ..
                    },
                    ..
                } if *target_process == ProcessId::new(2)
            )
        })
    }));

    let trace = gate.read_trace(stem);
    assert!(trace.contains(r#""event":"process_stopped","pid":3"#));
    assert!(trace.contains(r#""reason":"normal","decision":"not_restarted""#));
    assert!(!trace.contains(r#""new_child_pid":4"#));
}

#[test]
fn local_supervision_inactive_failed_child_send_outcome_returns_crashed() {
    let gate = GateHarness::new();
    let stem = "local_supervision_inactive_crashed_send_outcome";
    let artifact_path = "target/strata/local_supervision_inactive_crashed_send_outcome.mta";
    gate.remove_trace(stem);
    let run = gate.check_build_run(
        "examples/local_supervision_inactive_crashed_send_outcome.str",
        artifact_path,
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("inactive child crashed"));
    assert!(!stdout.contains("unexpected inactive child outcome"));

    let artifact = gate.read_artifact(artifact_path);
    let supervisor = artifact_process(&artifact, "Supervisor");
    assert!(supervisor.transitions.iter().any(|transition| {
        transition.actions.iter().any(|action| {
            matches!(
                action,
                ArtifactAction::SendOutcome {
                    target: ArtifactSendTarget::SupervisorChild {
                        target_process,
                        ..
                    },
                    ..
                } if *target_process == ProcessId::new(2)
            )
        })
    }));

    let trace = gate.read_trace(stem);
    assert_trace_event(
        &trace,
        &[
            r#""event":"effect_outcome_bound""#,
            r#""outcome_id":0"#,
            r#""action":"send""#,
            r#""target_process_id":2"#,
            r#""message_id":1"#,
            r#""outcome_result":"crashed""#,
        ],
    );
    assert!(trace.contains(r#""event":"process_failed","pid":3"#));
    assert!(trace.contains(r#""reason":"panic","decision":"not_restarted""#));
    assert!(trace.contains(r#""text":"inactive child crashed""#));
    assert!(!trace.contains(r#""decision":"restarted""#));
    assert!(!trace.contains(r#""message":"Work""#));
}
