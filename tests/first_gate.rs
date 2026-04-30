use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn hello_source_checks_builds_and_runs_on_mantle() {
    let root = workspace_root();
    let strata = env!("CARGO_BIN_EXE_strata");
    let mantle = env!("CARGO_BIN_EXE_mantle");

    let check = Command::new(strata)
        .args(["check", "examples/hello.str"])
        .current_dir(&root)
        .output()
        .expect("strata check should run");
    assert!(
        check.status.success(),
        "strata check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let build = Command::new(strata)
        .args(["build", "examples/hello.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/hello.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let run = Command::new(mantle)
        .args(["run", "target/strata/hello.mta"])
        .current_dir(&root)
        .output()
        .expect("mantle run should run");
    assert!(
        run.status.success(),
        "mantle run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("hello from Strata"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let trace_path = root.join("target/strata/hello.observability.jsonl");
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
    assert!(trace.contains(r#""event":"artifact_loaded""#));
    assert!(trace.contains(r#""event":"process_spawned""#));
    assert!(trace.contains(r#""event":"message_accepted""#));
    assert!(trace.contains(r#""event":"message_dequeued""#));
    assert!(trace.contains(r#""event":"program_output""#));
    assert!(trace.contains(r#""text":"hello from Strata""#));
    assert!(trace.contains(r#""event":"process_stopped""#));
}
