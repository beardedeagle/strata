use super::support::*;

#[test]
fn actor_reply_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run("examples/actor_reply.str", "target/strata/actor_reply.mta");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let artifact = gate.read_artifact("target/strata/actor_reply.mta");
    let sink_ref_type = process_ref_type_id(&artifact, ProcessId::new(2));
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(
        worker.transitions[0].actions[1],
        ArtifactAction::Send {
            target: ArtifactSendTarget::ReceivedPayload {
                ty: sink_ref_type,
                target_process: ProcessId::new(2),
            },
            message: MessageId::new(0),
            payload: None,
        }
    );
    let process_ref_payload = format!("type{}#3", sink_ref_type.as_u32());
    assert!(stdout.contains(&format!(
        "mantle: delivered Work({process_ref_payload}) to Worker"
    )));
    assert!(stdout.contains("mantle: delivered Done to Sink"));
    assert!(stdout.contains("worker forwarded done"));
    assert!(stdout.contains("sink received done"));

    let payload_type = format!(r#""payload_type_id":{}"#, sink_ref_type.as_u32());
    let trace = gate.read_trace("actor_reply");
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work",{payload_type},"payload":"{process_ref_payload}","payload_process_id":2,"payload_pid":3,"queue_depth":1,"sender_pid":1"#
    )));
    assert!(trace.contains(r#""event":"message_accepted","pid":3,"process_id":2,"process":"Sink","message_id":0,"message":"Done","queue_depth":1,"sender_pid":2"#));
    assert!(trace.contains(&format!(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work",{payload_type},"payload":"{process_ref_payload}","payload_process_id":2,"payload_pid":3,"result":"Stop","state_id":0,"state":"WorkerState""#
    )));
}

#[test]
fn actor_emit_spawn_send_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_emit_spawn_send.str",
        "target/strata/actor_emit_spawn_send.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: delivered Start to Main"));
    assert!(stdout.contains("mantle: delivered Ping to Worker"));
    assert!(stdout.contains("main authorized worker"));
    assert!(stdout.contains("worker handled authorized Ping"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_emit_spawn_send.mta");
    assert_eq!(
        transition_effects(&artifact, "Main"),
        &[
            ArtifactEffect::Emit,
            ArtifactEffect::Spawn,
            ArtifactEffect::Send
        ]
    );
    assert_eq!(
        transition_effects(&artifact, "Worker"),
        &[ArtifactEffect::Emit]
    );

    let trace = gate.read_trace("actor_emit_spawn_send");
    assert!(trace.contains(r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"main authorized worker""#));
    assert!(trace.contains(r#""event":"process_spawned","pid":2,"process_id":1,"process":"Worker","state_id":0,"state":"Idle""#));
    assert!(trace.contains(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"state_updated","pid":1,"process_id":0,"process":"Main","from_state_id":0,"from":"MainState{phase:Ready}","to_state_id":1,"to":"MainState{phase:Done}""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":1,"process_id":0,"process":"Main","message_id":0,"message":"Start","result":"Stop","state_id":1,"state":"MainState{phase:Done}""#));
    assert!(trace.contains(r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker handled authorized Ping""#));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Idle","to_state_id":1,"to":"Handled""#));
    assert!(trace.contains(
        r#""event":"process_stopped","pid":1,"process_id":0,"process":"Main","reason":"normal""#
    ));
    assert!(trace.contains(
        r#""event":"process_stopped","pid":2,"process_id":1,"process":"Worker","reason":"normal""#
    ));
}

#[test]
fn effect_authority_missing_fails_source_check_before_build() {
    let gate = GateHarness::new();
    gate.remove_artifact("target/strata/effect_authority_missing.mta");

    let check = gate.check_failure("examples/failures/effect_authority_missing.str");

    assert!(
        String::from_utf8_lossy(&check.stderr)
            .contains("step uses effect send but does not declare it"),
        "unexpected diagnostic\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        !gate
            .root
            .join("target/strata/effect_authority_missing.mta")
            .exists(),
        "source check failure must not create target/strata/effect_authority_missing.mta"
    );
}

#[test]
fn mantle_run_rejects_authority_mismatched_artifacts_before_trace() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/authority_admission_seed.mta";

    gate.check("examples/hello.str");
    gate.build("examples/hello.str", seed_artifact_path);

    for case in AUTHORITY_ADMISSION_CASES {
        let invalid_artifact_path = format!("target/strata/{}.mta", case.stem);
        gate.remove_artifact(&invalid_artifact_path);
        gate.remove_trace(case.stem);

        let artifact = gate.read_artifact(seed_artifact_path);
        let encoded_artifact = case.mutation.invalid_encoded_artifact(artifact);
        gate.write_unvalidated_encoded_artifact(&invalid_artifact_path, &encoded_artifact);

        let run = gate.run_mantle_failure(&invalid_artifact_path);

        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            stderr.contains(case.diagnostic),
            "unexpected diagnostic for {:?}\nstdout:\n{}\nstderr:\n{}",
            case.mutation,
            stdout,
            stderr
        );
        assert!(
            !stdout.contains("mantle: loaded"),
            "authority admission failure must not report artifact loading for {:?}",
            case.mutation
        );
        assert!(
            !stdout.contains("hello from Strata"),
            "authority admission failure must not produce runtime output for {:?}",
            case.mutation
        );
        assert!(
            !gate.trace_exists(case.stem),
            "authority admission failure must not create an observability trace for {:?}",
            case.mutation
        );
    }
}

#[test]
fn actor_panic_no_replay_checks_builds_and_fails_closed_on_mantle() {
    let gate = GateHarness::new();
    gate.check("examples/actor_panic_no_replay.str");
    gate.build(
        "examples/actor_panic_no_replay.str",
        "target/strata/actor_panic_no_replay.mta",
    );
    gate.remove_trace("actor_panic_no_replay");

    let run = gate.run_mantle_failure("target/strata/actor_panic_no_replay.mta");

    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains(
        "mantle: error: process Worker panicked after consuming message Ping; message will not be replayed"
    ));

    let trace = gate.read_trace("actor_panic_no_replay");
    assert_eq!(
        trace
            .matches(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping""#)
            .count(),
        2
    );
    assert_eq!(
        trace
            .matches(r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping""#)
            .count(),
        1
    );
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Ready","to_state_id":1,"to":"Failed""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","result":"Panic","state_id":1,"state":"Failed""#));
    assert!(trace.contains(r#""event":"process_failed","pid":2,"process_id":1,"process":"Worker","state_id":1,"state":"Failed","reason":"panic""#));
    assert!(
        !trace.contains(r#""event":"process_stopped","pid":2,"process_id":1,"process":"Worker""#)
    );
}
