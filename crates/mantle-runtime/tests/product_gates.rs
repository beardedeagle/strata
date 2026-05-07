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

#[test]
fn actor_instances_checks_builds_and_runs_on_mantle() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
        .args(["check", "examples/actor_instances.str"])
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
        .args(["build", "examples/actor_instances.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/actor_instances.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let run = Command::new(&mantle)
        .args(["run", "target/strata/actor_instances.mta"])
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
    assert!(stdout.contains("mantle: spawned Worker pid=3"));
    assert_eq!(stdout.matches("worker instance handled Ping").count(), 2);
    assert!(stdout.contains("mantle: stopped Main normally"));

    let trace_path = root.join("target/strata/actor_instances.observability.jsonl");
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
    assert!(trace.contains(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"message_accepted","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","result":"Stop","state_id":1,"state":"Handled""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","result":"Stop","state_id":1,"state":"Handled""#));
}

#[test]
fn actor_payloads_checks_builds_and_runs_on_mantle() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
        .args(["check", "examples/actor_payloads.str"])
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
        .args(["build", "examples/actor_payloads.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/actor_payloads.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let run = Command::new(&mantle)
        .args(["run", "target/strata/actor_payloads.mta"])
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
    assert!(stdout.contains("mantle: delivered Assign(Job{phase:Ready}) to Worker"));
    assert!(stdout.contains("worker assigned job"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let trace_path = root.join("target/strata/actor_payloads.observability.jsonl");
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
    assert!(trace.contains(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign","payload_type":"Job","payload":"Job{phase:Ready}""#));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{job:Job{phase:Done}}","to_state_id":1,"to":"WorkerState{job:Job{phase:Ready}}""#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign","payload_type":"Job","payload":"Job{phase:Ready}","result":"Stop","state_id":1,"state":"WorkerState{job:Job{phase:Ready}}""#));
}

#[test]
fn actor_reply_checks_builds_and_runs_on_mantle() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
        .args(["check", "examples/actor_reply.str"])
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
        .args(["build", "examples/actor_reply.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/actor_reply.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let run = Command::new(&mantle)
        .args(["run", "target/strata/actor_reply.mta"])
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
    assert!(stdout.contains("mantle: delivered Work(ProcessRef<Sink>#3) to Worker"));
    assert!(stdout.contains("mantle: delivered Done to Sink"));
    assert!(stdout.contains("worker forwarded done"));
    assert!(stdout.contains("sink received done"));

    let trace_path = root.join("target/strata/actor_reply.observability.jsonl");
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
    assert!(trace.contains(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work","payload_type":"ProcessRef<Sink>","payload":"ProcessRef<Sink>#3","payload_process_id":2,"payload_pid":3,"queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"message_accepted","pid":3,"process_id":2,"process":"Sink","message_id":0,"message":"Done","queue_depth":1,"sender_pid":2"#));
    assert!(trace.contains(r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work","payload_type":"ProcessRef<Sink>","payload":"ProcessRef<Sink>#3","payload_process_id":2,"payload_pid":3,"result":"Stop","state_id":0,"state":"WorkerState""#));
}

#[test]
fn effect_authority_missing_fails_source_check_before_build() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let artifact_path = root.join("target/strata/effect_authority_missing.mta");
    if artifact_path.exists() {
        std::fs::remove_file(&artifact_path).unwrap_or_else(|err| {
            panic!(
                "could not remove stale artifact {}: {err}",
                artifact_path.display()
            )
        });
    }

    let check = Command::new(&strata)
        .args(["check", "examples/failures/effect_authority_missing.str"])
        .current_dir(&root)
        .output()
        .expect("strata check should run");

    assert!(
        !check.status.success(),
        "strata check should reject missing effect authority\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        String::from_utf8_lossy(&check.stderr)
            .contains("step uses effect send but does not declare it"),
        "unexpected diagnostic\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        !artifact_path.exists(),
        "source check failure must not create {}",
        artifact_path.display()
    );
}

#[test]
fn actor_panic_no_replay_checks_builds_and_fails_closed_on_mantle() {
    let root = workspace_root();
    ensure_workspace_binaries(&root);
    let strata = binary_path(&root, "strata");
    let mantle = binary_path(&root, "mantle");

    let check = Command::new(&strata)
        .args(["check", "examples/actor_panic_no_replay.str"])
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
        .args(["build", "examples/actor_panic_no_replay.str"])
        .current_dir(&root)
        .output()
        .expect("strata build should run");
    assert!(
        build.status.success(),
        "strata build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact_path = root.join("target/strata/actor_panic_no_replay.mta");
    assert!(
        artifact_path.exists(),
        "expected {}",
        artifact_path.display()
    );

    let trace_path = root.join("target/strata/actor_panic_no_replay.observability.jsonl");
    if trace_path.exists() {
        std::fs::remove_file(&trace_path).unwrap_or_else(|err| {
            panic!(
                "could not remove stale trace {}: {err}",
                trace_path.display()
            )
        });
    }

    let run = Command::new(&mantle)
        .args(["run", "target/strata/actor_panic_no_replay.mta"])
        .current_dir(&root)
        .output()
        .expect("mantle run should run");
    assert!(
        !run.status.success(),
        "mantle run should fail closed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains(
        "mantle: error: process Worker panicked after consuming message Ping; message will not be replayed"
    ));

    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()));
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
