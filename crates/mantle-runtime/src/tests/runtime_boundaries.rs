use super::support::*;

#[test]
fn runtime_rejects_invalid_artifact_identity() {
    let mut artifact = valid_artifact();
    artifact.format = "other".to_string();

    let err = run_artifact(Path::new("target/test/bad.mta"), &artifact)
        .expect_err("invalid artifact must fail closed");
    assert!(err.to_string().contains("unsupported artifact format"));
}

#[test]
fn runtime_rejects_action_without_declared_effect() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("artifact admission should reject undeclared effect");

    assert!(
        err.to_string()
            .contains("process Main transition 0 uses effect send but does not declare it")
    );
    assert!(
        host.events().is_empty(),
        "rejected artifacts must not reach runtime execution"
    );
}

#[test]
fn runtime_rejects_blocked_trace_sink_before_returning_run_report() {
    let dir = unique_test_dir("blocked-trace-sink");
    fs::create_dir_all(&dir).expect("test dir should be created");
    let blocked_parent = dir.join("blocked");
    fs::write(&blocked_parent, "not a directory").expect("blocking file should be written");

    let artifact_path = blocked_parent.join("hello.mta");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    let err = run_artifact(&artifact_path, &artifact)
        .expect_err("blocked trace sink should fail before a run report is returned");

    assert!(!err.to_string().is_empty());
    assert!(!trace_path.exists(), "trace path must not be created");

    let _ = fs::remove_file(blocked_parent);
    let _ = fs::remove_dir(dir);
}

#[cfg(any(unix, windows))]
#[test]
fn run_artifact_path_writes_trace_for_current_directory_artifact() {
    let artifact_path = unique_current_dir_artifact_path("runtime-current-dir");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    write_artifact(&artifact_path, &artifact).expect("artifact write should succeed");

    let report =
        run_artifact_path(&artifact_path).expect("current-directory artifact run should work");

    assert_eq!(report.trace_path, trace_path);
    assert!(trace_path.exists(), "runtime trace should be written");
    let trace = fs::read_to_string(&trace_path).expect("runtime trace should be readable");
    assert!(trace.contains(r#""event":"artifact_loaded""#));
    assert!(trace.contains(&format!(r#""schema_version":"{ARTIFACT_SCHEMA_VERSION}""#)));
    assert!(trace.contains(r#""event":"process_stopped""#));

    fs::remove_file(artifact_path).expect("test artifact should be removed");
    fs::remove_file(trace_path).expect("test trace should be removed");
}

#[cfg(all(not(unix), not(windows)))]
#[test]
fn run_artifact_path_fails_closed_without_secure_file_identity_support() {
    let artifact_path = unique_current_dir_artifact_path("runtime-unsupported-artifact-io");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    fs::write(&artifact_path, artifact.encode()).expect("test artifact should be written");

    let err = run_artifact_path(&artifact_path).expect_err("unsupported artifact read should fail");

    assert!(
        err.to_string().contains("secure file identity support"),
        "unexpected error: {err}"
    );
    assert!(!trace_path.exists(), "trace path must not be created");

    fs::remove_file(artifact_path).expect("test artifact should be removed");
}

#[test]
fn runtime_rejects_dispatch_budget_exhaustion() {
    let artifact_path = unique_current_dir_artifact_path("runtime-budget");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = looping_artifact();

    let err = run_artifact_with_limits(
        &artifact_path,
        &artifact,
        RunLimits {
            max_dispatches: 3,
            ..RunLimits::default()
        },
    )
    .expect_err("looping artifact should hit the dispatch budget");

    assert!(
        err.to_string()
            .contains("runtime dispatch budget exceeded after 3 process step(s)")
    );

    let _ = fs::remove_file(trace_path);
}

#[test]
fn runtime_limits_reject_zero_runtime_processes() {
    let err = RunLimits {
        max_runtime_processes: 0,
        ..RunLimits::default()
    }
    .validate()
    .expect_err("zero runtime process limit should fail closed");

    assert!(
        err.to_string()
            .contains("max_runtime_processes must be greater than zero")
    );
}

#[test]
fn runtime_rejects_runtime_process_limit_before_spawning_child() {
    let artifact = valid_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(
        &artifact,
        &mut host,
        RunLimits {
            max_runtime_processes: 1,
            ..RunLimits::default()
        },
    )
    .expect_err("runtime process limit should fail before spawning Worker");

    assert!(
        err.to_string()
            .contains("runtime process instance limit exceeded at 1 process instance(s)")
    );
    assert!(!host.events().iter().any(|event| {
        matches!(
            event,
            RuntimeEvent::ProcessSpawned { process, .. } if process == "Worker"
        )
    }));
}

#[test]
fn runtime_rejects_trace_limit_exhaustion() {
    let artifact_path = unique_current_dir_artifact_path("runtime-trace-limit");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    let err = run_artifact_with_limits(
        &artifact_path,
        &artifact,
        RunLimits {
            max_trace_bytes: 8,
            ..RunLimits::default()
        },
    )
    .expect_err("small trace limit should fail closed");

    assert!(
        err.to_string()
            .contains("runtime trace exceeded maximum size of 8 bytes")
    );

    let _ = fs::remove_file(trace_path);
}

#[test]
fn runtime_rejects_emitted_output_limit_exhaustion() {
    let artifact_path = unique_current_dir_artifact_path("runtime-output-limit");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    let err = run_artifact_with_limits(
        &artifact_path,
        &artifact,
        RunLimits {
            max_emitted_output_bytes: "worker handled Ping".len(),
            ..RunLimits::default()
        },
    )
    .expect_err("small emitted output limit should fail closed");

    assert!(
        err.to_string()
            .contains("emitted output exceeded maximum size")
    );

    let _ = fs::remove_file(trace_path);
}
