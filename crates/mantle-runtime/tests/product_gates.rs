#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

static BUILD_WORKSPACE_BINS: Once = Once::new();

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test crate should be under crates/")
        .to_path_buf()
}

fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join("target"))
}

fn cargo_profile() -> String {
    std::env::var("PROFILE")
        .ok()
        .filter(|profile| !profile.is_empty())
        .or_else(profile_from_current_exe)
        .expect("Cargo profile should be available from PROFILE or current test executable path")
}

fn profile_from_current_exe() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let deps_dir = exe.parent()?;
    let profile_dir = deps_dir.parent()?;
    profile_dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(String::from)
}

fn binary_path(root: &Path, name: &str) -> PathBuf {
    target_dir(root)
        .join(cargo_profile())
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn ensure_workspace_binaries(root: &Path) {
    BUILD_WORKSPACE_BINS.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let profile = cargo_profile();
        let mut build = Command::new(cargo);
        build.args(["build", "--workspace", "--bins"]);
        if profile == "release" {
            build.arg("--release");
        } else if profile != "debug" {
            build.args(["--profile", profile.as_str()]);
        }
        let build = build
            .current_dir(root)
            .output()
            .expect("cargo build should run");
        assert!(
            build.status.success(),
            "cargo build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
    });
}

#[test]
fn hello_source_checks_builds_and_runs_on_mantle() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
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

    let build = Command::new(&strata)
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

    let run = Command::new(&mantle)
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
    assert!(trace.contains(r#""process":"Main""#));
    assert!(trace.contains(r#""event":"message_accepted""#));
    assert!(trace.contains(r#""event":"message_dequeued""#));
    assert!(trace.contains(r#""event":"program_output""#));
    assert!(trace.contains(r#""text":"hello from Strata""#));
    assert!(trace.contains(r#""event":"process_stopped""#));
}

#[test]
fn actor_ping_checks_builds_and_runs_on_mantle() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
        .args(["check", "examples/actor_ping.str"])
        .current_dir(&root)
        .output()
        .expect("strata check should run");
    assert!(
        check.status.success(),
        "strata check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let build = Command::new(&strata)
        .args(["build", "examples/actor_ping.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/actor_ping.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let run = Command::new(&mantle)
        .args(["run", "target/strata/actor_ping.mta"])
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
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: delivered Start to Main"));
    assert!(stdout.contains("mantle: delivered Ping to Worker"));
    assert!(stdout.contains("worker handled Ping"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let trace_path = root.join("target/strata/actor_ping.observability.jsonl");
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
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
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
        .args(["check", "examples/actor_sequence.str"])
        .current_dir(&root)
        .output()
        .expect("strata check should run");
    assert!(
        check.status.success(),
        "strata check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let build = Command::new(&strata)
        .args(["build", "examples/actor_sequence.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/actor_sequence.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let run = Command::new(&mantle)
        .args(["run", "target/strata/actor_sequence.mta"])
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
    assert!(stdout.contains("mantle: spawned Main pid=1"));
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: delivered First to Worker"));
    assert!(stdout.contains("mantle: delivered Second to Worker"));
    assert!(stdout.contains("worker handled First"));
    assert!(stdout.contains("worker handled Second"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let trace_path = root.join("target/strata/actor_sequence.observability.jsonl");
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
    assert!(trace.contains(r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"First","result":"Continue","state_id":1,"state":"SawFirst""#));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":1,"from":"SawFirst","to_state_id":2,"to":"Done""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Second","result":"Stop","state_id":2,"state":"Done""#));
}
