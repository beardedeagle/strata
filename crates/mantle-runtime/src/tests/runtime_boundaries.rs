use super::support::*;
use std::io::{self, Write};

#[test]
fn runtime_rejects_invalid_artifact_identity() {
    let mut artifact = valid_artifact();
    artifact.format = "other".into();

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
fn runtime_rejects_unsupported_target_feature_before_artifact_loaded() {
    for (feature, feature_name) in [
        (
            RuntimeFeature::DistributedTransport,
            "distributed_transport",
        ),
        (RuntimeFeature::RemoteSend, "remote_send"),
        (RuntimeFeature::RemoteSpawn, "remote_spawn"),
    ] {
        let mut artifact = valid_artifact();
        let mut requirements = test_target_requirements();
        requirements.features.push(feature);
        artifact.target_requirements =
            ArtifactTargetRequirements::new(TEST_SOURCE_LANGUAGE, requirements.features);
        let mut host = InMemoryRuntimeHost::default();

        let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
            .expect_err("unsupported target feature should fail before execution");

        assert!(
            err.to_string().contains(&format!(
                "target runtime feature {feature_name} is not supported"
            )),
            "{err}"
        );
        assert!(
            host.events().is_empty(),
            "unsupported {feature_name} requirement must fail before ArtifactLoaded"
        );
    }
}

#[test]
fn runtime_rejects_underdeclared_target_features_before_artifact_loaded() {
    let mut artifact = valid_artifact();
    artifact.target_requirements =
        ArtifactTargetRequirements::new(TEST_SOURCE_LANGUAGE, vec![RuntimeFeature::BoundedMailbox]);
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("underdeclared target requirements should fail before execution");

    assert!(
        err.to_string()
            .contains("target requirements do not declare required runtime feature jsonl_trace"),
        "{err}"
    );
    assert!(
        host.events().is_empty(),
        "underdeclared requirements must fail before ArtifactLoaded"
    );
}

#[test]
fn runtime_rejects_malformed_source_language_before_artifact_loaded() {
    let mut artifact = valid_artifact();
    artifact.source_language = "not-valid".into();
    artifact.target_requirements.source_language = "not-valid".into();
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("malformed source language should fail before execution");

    assert!(
        err.to_string()
            .contains("artifact field source_language must be an identifier")
    );
    assert!(
        host.events().is_empty(),
        "malformed source language must fail before ArtifactLoaded"
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
    assert!(trace.contains(r#""trace_schema":"mantle-runtime-observability""#));
    validate_runtime_trace_jsonl(&trace).expect("runtime trace should match Mantle trace schema");
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

#[test]
fn filesystem_runtime_host_output_write_failure_fails_closed() {
    let artifact_path = unique_current_dir_artifact_path("runtime-output-write-failure");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();
    let mut output = FailingOutput::fail_on_write();

    let err = run_artifact_with_limits_and_bindings_and_output(
        &artifact_path,
        &artifact,
        RunLimits::default(),
        None,
        None,
        &mut output,
    )
    .expect_err("output sink write failure must fail closed");

    assert!(
        err.to_string().contains("runtime output sink write failed"),
        "unexpected error: {err}"
    );
    assert!(
        output.text().is_empty(),
        "failed output sink must not receive a partial success report"
    );
    let trace = fs::read_to_string(&trace_path)
        .expect("failed output sink should still leave trace-first audit evidence");
    assert!(
        trace.contains(r#""event":"program_output""#),
        "output sink failure must preserve attempted program output in the runtime trace"
    );

    let _ = fs::remove_file(trace_path);
}

#[test]
fn filesystem_runtime_host_output_flush_failure_fails_closed() {
    let artifact_path = unique_current_dir_artifact_path("runtime-output-flush-failure");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();
    let mut output = FailingOutput::fail_on_flush();

    let err = run_artifact_with_limits_and_bindings_and_output(
        &artifact_path,
        &artifact,
        RunLimits::default(),
        None,
        None,
        &mut output,
    )
    .expect_err("output sink flush failure must fail closed");

    assert!(
        err.to_string().contains("runtime output sink flush failed"),
        "unexpected error: {err}"
    );
    assert_eq!(
        output.text(),
        "worker handled Ping\n",
        "program output can be emitted before a later flush failure, but no run report is returned"
    );

    let _ = fs::remove_file(trace_path);
}

#[cfg(any(unix, windows))]
#[test]
fn mantle_cli_output_sink_failure_returns_no_success_report() {
    let artifact_path = unique_current_dir_artifact_path("runtime-cli-output-write-failure");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    write_artifact(&artifact_path, &valid_artifact()).expect("artifact write should succeed");
    let mut output = FailingOutput::fail_on_write();

    let err = crate::cli::mantle_main_with_output(
        [
            "mantle".to_string(),
            "run".to_string(),
            artifact_path.display().to_string(),
        ],
        &mut output,
    )
    .expect_err("CLI output sink failure must fail closed");

    assert!(
        err.to_string().contains("runtime output sink write failed"),
        "unexpected error: {err}"
    );
    assert!(
        !output.text().contains("mantle: loaded"),
        "CLI must not print a success report after output sink failure"
    );

    let _ = fs::remove_file(artifact_path);
    let _ = fs::remove_file(trace_path);
}

#[cfg(any(unix, windows))]
#[test]
fn mantle_cli_run_report_flush_failure_fails_closed() {
    let artifact_path = unique_current_dir_artifact_path("runtime-cli-report-flush-failure");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    write_artifact(&artifact_path, &valid_artifact()).expect("artifact write should succeed");
    let mut output = FailingOutput::fail_on_second_flush();

    let err = crate::cli::mantle_main_with_output(
        [
            "mantle".to_string(),
            "run".to_string(),
            artifact_path.display().to_string(),
        ],
        &mut output,
    )
    .expect_err("CLI run report flush failure must fail closed");

    assert!(
        err.to_string().contains("runtime output sink flush failed"),
        "unexpected error: {err}"
    );
    assert_eq!(
        output.flush_attempts(),
        2,
        "regression must fail on the report flush after the runtime output flush succeeds"
    );
    assert!(
        output.text().contains("mantle: trace"),
        "report bytes may reach a buffered sink before final flush failure"
    );

    let _ = fs::remove_file(artifact_path);
    let _ = fs::remove_file(trace_path);
}

struct FailingOutput {
    text: String,
    fail_writes: bool,
    flushes_before_failure: Option<usize>,
    flush_attempts: usize,
}

impl FailingOutput {
    fn fail_on_write() -> Self {
        Self {
            text: String::new(),
            fail_writes: true,
            flushes_before_failure: None,
            flush_attempts: 0,
        }
    }

    fn fail_on_flush() -> Self {
        Self::fail_after_flush_successes(0)
    }

    fn fail_on_second_flush() -> Self {
        Self::fail_after_flush_successes(1)
    }

    fn fail_after_flush_successes(flushes_before_failure: usize) -> Self {
        Self {
            text: String::new(),
            fail_writes: false,
            flushes_before_failure: Some(flushes_before_failure),
            flush_attempts: 0,
        }
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn flush_attempts(&self) -> usize {
        self.flush_attempts
    }
}

impl Write for FailingOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.fail_writes {
            return Err(io::Error::other("runtime output sink write failed"));
        }
        let text = std::str::from_utf8(buf).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "runtime output sink test received non-UTF-8 output",
            )
        })?;
        self.text.push_str(text);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_attempts += 1;
        if let Some(flushes_before_failure) = &mut self.flushes_before_failure {
            if *flushes_before_failure == 0 {
                return Err(io::Error::other("runtime output sink flush failed"));
            }
            *flushes_before_failure -= 1;
        }
        Ok(())
    }
}
