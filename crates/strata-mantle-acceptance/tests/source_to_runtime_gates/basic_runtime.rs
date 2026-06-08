use super::support::*;

#[test]
fn forged_remote_distributed_target_requirements_fail_before_runtime_admission() {
    let gate = GateHarness::new();
    let seed_artifact = "target/strata/hello.mta";
    gate.check("examples/hello.str");
    gate.build("examples/hello.str", seed_artifact);
    let seed = gate.read_artifact(seed_artifact);

    for (feature, feature_name, forged_artifact, trace_stem) in [
        (
            RuntimeFeature::DistributedTransport,
            "distributed_transport",
            "target/strata/hello_distributed_transport.mta",
            "hello_distributed_transport",
        ),
        (
            RuntimeFeature::RemoteSend,
            "remote_send",
            "target/strata/hello_remote_send.mta",
            "hello_remote_send",
        ),
        (
            RuntimeFeature::RemoteSpawn,
            "remote_spawn",
            "target/strata/hello_remote_spawn.mta",
            "hello_remote_spawn",
        ),
    ] {
        gate.remove_artifact(forged_artifact);
        gate.remove_trace(trace_stem);
        let mut artifact = seed.clone();
        ensure_target_requirement(&mut artifact, feature);
        gate.write_unvalidated_encoded_artifact(forged_artifact, &artifact.encode());

        let expected = format!("target runtime feature {feature_name} is not supported");
        let admission = gate.admit_failure(forged_artifact);
        let admit_stderr = String::from_utf8_lossy(&admission.stderr);
        assert!(
            admit_stderr.contains(&expected),
            "forged {feature_name} target requirement should fail admission: {admit_stderr}"
        );

        let run = gate.run_mantle_failure(forged_artifact);
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            stderr.contains(&expected),
            "forged {feature_name} target requirement should fail before runtime: {stderr}"
        );
        assert!(!stdout.contains("mantle: loaded"));
        assert!(
            !gate.trace_exists(trace_stem),
            "forged {feature_name} target requirement must not create a runtime trace"
        );
    }
}

#[test]
fn checked_strata_examples_stay_inside_local_runtime_target_profile() {
    let gate = GateHarness::new();
    let unsupported_features = ["distributed_transport", "remote_send", "remote_spawn"];
    let sources = gate.top_level_entry_example_sources();
    for representative in [
        "examples/hello.str",
        "examples/actor_ping.str",
        "examples/component_composition_main.str",
    ] {
        assert!(
            sources.iter().any(|source| source == representative),
            "entry example scan should include {representative}: {sources:?}"
        );
    }

    for source in sources {
        let requirements = gate.target_requirements(&source, "json");
        let requirements = String::from_utf8(requirements.stdout)
            .expect("target requirements should render UTF-8 JSON");

        assert!(
            requirements.contains("\"source_language\":\"strata\""),
            "{source} must keep Strata source language metadata: {requirements}"
        );
        assert!(
            requirements.contains("\"local_execution\""),
            "{source} must declare local runtime execution: {requirements}"
        );
        for unsupported in unsupported_features {
            assert!(
                !requirements.contains(&format!("\"{unsupported}\"")),
                "{source} must not lower into unsupported runtime feature {unsupported}: {requirements}"
            );
        }
    }
}

#[test]
fn hello_source_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run("examples/hello.str", "target/strata/hello.mta");

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("hello from Strata"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    let requirements = gate.target_requirements("examples/hello.str", "json");
    let requirements =
        String::from_utf8(requirements.stdout).expect("target requirements should be UTF-8");
    assert!(requirements.contains("\"source_language\":\"strata\""));
    assert!(requirements.contains("\"emit_effect\""));
    assert!(!requirements.contains("\"local_spawn\""));
    assert!(!requirements.contains("\"typed_value_templates\""));
    let admission = gate.admit("target/strata/hello.mta", "json");
    let admission = String::from_utf8(admission.stdout).expect("runtime admission should be UTF-8");
    assert!(admission.contains("\"runtime_profile\":\"mantle.local_only.v1\""));
    assert!(admission.contains("\"runtime_scope\":\"single_host_local_runtime\""));

    let artifact = gate.read_artifact("target/strata/hello.mta");
    assert!(
        artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::EmitEffect)
    );
    assert!(
        !artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::LocalSpawn)
    );
    assert!(
        !artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::TypedValueTemplates)
    );

    let trace = gate.read_trace("hello");
    assert!(trace.contains(r#""event":"artifact_loaded""#));
    assert!(trace.contains(r#""event":"process_spawned""#));
    assert!(trace.contains(r#""process":"Main""#));
    assert!(trace.contains(r#""event":"message_accepted""#));
    assert!(trace.contains(r#""event":"message_dequeued""#));
    assert!(trace.contains(r#""event":"program_output""#));
    assert!(trace.contains(r#""text":"hello from Strata""#));
    assert!(trace.contains(r#""event":"process_stopped""#));
}

#[test]
fn actor_ping_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run("examples/actor_ping.str", "target/strata/actor_ping.mta");

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: delivered Start to Main"));
    assert!(stdout.contains("mantle: delivered Ping to Worker"));
    assert!(stdout.contains("worker handled Ping"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    let artifact = gate.read_artifact("target/strata/actor_ping.mta");
    assert!(
        artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::LocalSpawn)
    );
    assert!(
        artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::LocalSend)
    );
    assert!(
        !artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::RemoteSpawn)
    );

    let trace = gate.read_trace("actor_ping");
    assert!(trace.contains(r#""event":"process_spawned""#));
    assert!(trace.contains(r#""process":"Worker""#));
    assert!(trace.contains(r#""event":"message_accepted""#));
    assert!(trace.contains(r#""message":"Ping""#));
    assert!(trace.contains(r#""event":"message_dequeued""#));
    assert!(trace.contains(r#""event":"state_updated""#));
    assert!(trace.contains(r#""from_state_id":0,"from":"Idle","to_state_id":1,"to":"Handled""#));
    assert!(trace.contains(r#""event":"program_output""#));
    assert!(trace.contains(r#""text":"worker handled Ping""#));
    assert!(trace.contains(r#""event":"process_stopped""#));
}

#[test]
fn actor_sequence_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_sequence.str",
        "target/strata/actor_sequence.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: delivered First to Worker"));
    assert!(stdout.contains("mantle: delivered Second to Worker"));
    assert!(stdout.contains("worker handled First"));
    assert!(stdout.contains("worker handled Second"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let trace = gate.read_trace("actor_sequence");
    assert!(trace.contains(r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","result":"Continue","state_id":1,"state":"SawFirst""#));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":1,"from":"SawFirst","to_state_id":2,"to":"Done""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Second","result":"Stop","state_id":2,"state":"Done""#));
}

#[test]
fn actor_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run("examples/actor_match.str", "target/strata/actor_match.mta");

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: delivered First to Worker"));
    assert!(stdout.contains("mantle: delivered Second to Worker"));
    assert!(stdout.contains("worker matched First"));
    assert!(stdout.contains("worker matched Second"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.debug_name, "Main");
    assert_eq!(main.transitions.len(), 1);
    assert_eq!(
        main.transitions[0].message,
        mantle_artifact::MessageId::new(0)
    );
    assert_eq!(
        main.transitions[0].effects,
        [ArtifactEffect::Spawn, ArtifactEffect::Send]
    );

    let worker = &artifact.processes[1];
    assert_eq!(worker.debug_name, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    assert_eq!(
        worker.transitions[0].message,
        mantle_artifact::MessageId::new(0)
    );
    assert_eq!(
        worker.transitions[1].message,
        mantle_artifact::MessageId::new(1)
    );
    assert_eq!(worker.transitions[0].effects, [ArtifactEffect::Emit]);
    assert_eq!(worker.transitions[1].effects, [ArtifactEffect::Emit]);

    let trace = gate.read_trace("actor_match");
    assert!(trace.contains(r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First""#));
    assert!(trace.contains(r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker matched First""#));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Waiting","to_state_id":1,"to":"SawFirst""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","result":"Continue","state_id":1,"state":"SawFirst""#));
    assert!(trace.contains(r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Second""#));
    assert!(trace.contains(r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker matched Second""#));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":1,"from":"SawFirst","to_state_id":2,"to":"Done""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Second","result":"Stop","state_id":2,"state":"Done""#));
    assert!(trace.contains(
        r#""event":"process_stopped","pid":1,"process_id":0,"process":"Main","reason":"normal""#
    ));
    assert!(trace.contains(
        r#""event":"process_stopped","pid":2,"process_id":1,"process":"Worker","reason":"normal""#
    ));
}

#[test]
fn init_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run("examples/init_match.str", "target/strata/init_match.mta");

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("init match selected WarmReady"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/init_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.debug_name, "Main");
    assert_eq!(main.init_state, mantle_artifact::StateId::new(0));
    assert_eq!(main.state_values.len(), 1);
    assert_eq!(main.state_values[0].label, "MainState{readiness:WarmReady}");
    assert_eq!(main.transitions.len(), 1);
    assert_eq!(
        main.transitions[0].next_state,
        mantle_artifact::NextState::Current
    );

    let trace = gate.read_trace("init_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{readiness:WarmReady}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"init match selected WarmReady""#
    ));
    assert!(trace.contains(
        r#""event":"process_stepped","pid":1,"process_id":0,"process":"Main","message_id":0,"message":"Start","result":"Stop","state_id":0,"state":"MainState{readiness:WarmReady}""#
    ));
}

#[test]
fn init_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/init_return_match.str",
        "target/strata/init_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("init return match selected WarmReady"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/init_return_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.debug_name, "Main");
    assert_eq!(main.init_state, mantle_artifact::StateId::new(0));
    assert_eq!(main.state_values.len(), 1);
    assert_eq!(main.state_values[0].label, "MainState{readiness:WarmReady}");
    assert_eq!(main.transitions.len(), 1);
    assert_eq!(
        main.transitions[0].next_state,
        mantle_artifact::NextState::Current
    );

    let trace = gate.read_trace("init_return_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{readiness:WarmReady}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"init return match selected WarmReady""#
    ));
    assert!(trace.contains(
        r#""event":"process_stepped","pid":1,"process_id":0,"process":"Main","message_id":0,"message":"Start","result":"Stop","state_id":0,"state":"MainState{readiness:WarmReady}""#
    ));
}

#[test]
fn actor_instances_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_instances.str",
        "target/strata/actor_instances.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: spawned Worker pid=3"));
    assert_eq!(stdout.matches("worker instance handled Ping").count(), 2);
    assert!(stdout.contains("mantle: stopped Main normally"));

    let trace = gate.read_trace("actor_instances");
    assert!(trace.contains(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"message_accepted","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","result":"Stop","state_id":1,"state":"Handled""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","result":"Stop","state_id":1,"state":"Handled""#));
}

#[test]
fn actor_payloads_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payloads.str",
        "target/strata/actor_payloads.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Assign(Job{phase:Ready}) to Worker"));
    assert!(stdout.contains("worker assigned job"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payloads.mta");
    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("actor_payloads");
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{job:Job{phase:Done}}","to_state_id":1,"to":"WorkerState{job:Job{phase:Ready}}""#));
    assert!(trace.contains(&format!(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}","result":"Stop","state_id":1,"state":"WorkerState{{job:Job{{phase:Ready}}}}""#
    )));
}
