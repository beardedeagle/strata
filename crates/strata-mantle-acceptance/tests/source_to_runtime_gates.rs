#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Once;

use mantle_artifact::{
    ArtifactAction, ArtifactEffect, ArtifactProcess, ArtifactSendTarget, ArtifactTypeKind,
    ArtifactValue, ArtifactValueTemplate, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, EnumVariantId, MantleArtifact, MessageId, NextState, ProcessId,
    TypeId, read_artifact,
};

static BUILD_WORKSPACE_BINS: Once = Once::new();

struct GateHarness {
    root: PathBuf,
    strata: PathBuf,
    mantle: PathBuf,
}

fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

impl GateHarness {
    fn new() -> Self {
        let root = workspace_root();
        ensure_workspace_binaries(&root);
        let strata = binary_path(&root, "strata");
        let mantle = binary_path(&root, "mantle");
        Self {
            root,
            strata,
            mantle,
        }
    }

    fn check_build_run(&self, source: &str, artifact: &str) -> Output {
        self.check(source);
        self.build(source, artifact);
        self.run_mantle_success(artifact)
    }

    fn check(&self, source: &str) {
        assert_success(
            self.command(&self.strata, ["check", source], "strata check"),
            "strata check",
        );
    }

    fn check_failure(&self, source: &str) -> Output {
        assert_failure(
            self.command(&self.strata, ["check", source], "strata check"),
            "strata check",
        )
    }

    fn build(&self, source: &str, artifact: &str) {
        self.remove_artifact(artifact);
        assert_success(
            self.command(
                &self.strata,
                ["build", source, "--output", artifact],
                "strata build",
            ),
            "strata build",
        );
        assert!(
            self.root.join(artifact).exists(),
            "expected {}",
            self.root.join(artifact).display()
        );
    }

    fn run_mantle_success(&self, artifact: &str) -> Output {
        assert_success(
            self.command(&self.mantle, ["run", artifact], "mantle run"),
            "mantle run",
        )
    }

    fn run_mantle_failure(&self, artifact: &str) -> Output {
        assert_failure(
            self.command(&self.mantle, ["run", artifact], "mantle run"),
            "mantle run",
        )
    }

    fn command<const N: usize>(&self, binary: &Path, args: [&str; N], label: &str) -> Output {
        Command::new(binary)
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap_or_else(|err| panic!("{label} should run: {err}"))
    }

    fn remove_artifact(&self, artifact: &str) {
        let path = self.root.join(artifact);
        remove_file_if_exists(&path);
    }

    fn remove_trace(&self, stem: &str) {
        let path = self.trace_path(stem);
        remove_file_if_exists(&path);
    }

    fn read_trace(&self, stem: &str) -> String {
        let trace_path = self.trace_path(stem);
        fs::read_to_string(&trace_path)
            .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()))
    }

    fn read_artifact(&self, artifact: &str) -> MantleArtifact {
        read_artifact(&self.root.join(artifact))
            .unwrap_or_else(|err| panic!("expected artifact {artifact}: {err}"))
    }

    fn write_target_source(&self, stem: &str, source: &str) -> PathBuf {
        let path = self.root.join("target/strata").join(format!("{stem}.str"));
        fs::create_dir_all(path.parent().expect("target source should have a parent"))
            .unwrap_or_else(|err| panic!("could not create target source directory: {err}"));
        fs::write(&path, source).unwrap_or_else(|err| {
            panic!("could not write target source {}: {err}", path.display())
        });
        path
    }

    fn write_unvalidated_encoded_artifact(&self, artifact: &str, encoded_artifact: &str) {
        fs::write(self.root.join(artifact), encoded_artifact)
            .unwrap_or_else(|err| panic!("could not write artifact {artifact}: {err}"));
    }

    fn trace_exists(&self, stem: &str) -> bool {
        self.trace_path(stem).exists()
    }

    fn trace_path(&self, stem: &str) -> PathBuf {
        self.root
            .join("target/strata")
            .join(format!("{stem}.observability.jsonl"))
    }
}

fn value_type_id(artifact: &MantleArtifact, label: &str) -> TypeId {
    artifact_type_id(artifact, label, ArtifactTypeKind::Value)
}

fn process_ref_type_id(artifact: &MantleArtifact, target: ProcessId) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.kind == ArtifactTypeKind::ProcessRef { target })
        .unwrap_or_else(|| {
            panic!(
                "artifact process reference type targeting process {} should exist",
                target.as_u32()
            )
        });
    TypeId::from_index(index).expect("artifact type index should fit")
}

fn artifact_type_id(artifact: &MantleArtifact, label: &str, kind: ArtifactTypeKind) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.label == label && ty.kind == kind)
        .unwrap_or_else(|| panic!("artifact type {label} with kind {kind:?} should exist"));
    TypeId::from_index(index).expect("artifact type index should fit")
}

fn artifact_process<'a>(artifact: &'a MantleArtifact, process: &str) -> &'a ArtifactProcess {
    artifact
        .processes
        .iter()
        .find(|candidate| candidate.debug_name == process)
        .unwrap_or_else(|| panic!("artifact process {process} should exist"))
}

fn assert_trace_event(trace: &str, fields: &[&str]) {
    assert!(
        !fields.is_empty(),
        "trace event assertion should require at least one field"
    );
    assert!(
        trace
            .lines()
            .any(|line| fields.iter().all(|field| line.contains(field))),
        "expected trace event containing fields {fields:?}\ntrace:\n{trace}"
    );
}

fn transition_effects<'a>(artifact: &'a MantleArtifact, process: &str) -> &'a [ArtifactEffect] {
    artifact_process(artifact, process)
        .transitions
        .first()
        .unwrap_or_else(|| panic!("artifact process {process} should have an entry transition"))
        .effects
        .as_slice()
}

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
        assert_success(build, "cargo build");
    });
}

fn assert_success(output: Output, label: &str) -> Output {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn assert_failure(output: Output, label: &str) -> Output {
    assert!(
        !output.status.success(),
        "{label} should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn remove_file_if_exists(path: &Path) {
    if path.exists() {
        fs::remove_file(path)
            .unwrap_or_else(|err| panic!("could not remove stale file {}: {err}", path.display()));
    }
}

#[derive(Debug, Clone, Copy)]
struct AuthorityAdmissionCase {
    stem: &'static str,
    mutation: AuthorityAdmissionMutation,
    diagnostic: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum AuthorityAdmissionMutation {
    MissingEmitAuthority,
    UnusedSpawnAuthority,
    DuplicateEmitAuthority,
    UnknownEncodedEffect,
}

const AUTHORITY_ADMISSION_CASES: [AuthorityAdmissionCase; 4] = [
    AuthorityAdmissionCase {
        stem: "effect_authority_missing_runtime",
        mutation: AuthorityAdmissionMutation::MissingEmitAuthority,
        diagnostic: "mantle: error: process Main transition 0 uses effect emit but does not declare it",
    },
    AuthorityAdmissionCase {
        stem: "effect_authority_unused_runtime",
        mutation: AuthorityAdmissionMutation::UnusedSpawnAuthority,
        diagnostic: "mantle: error: process Main transition 0 declares effect spawn but no action uses it",
    },
    AuthorityAdmissionCase {
        stem: "effect_authority_duplicate_runtime",
        mutation: AuthorityAdmissionMutation::DuplicateEmitAuthority,
        diagnostic: "mantle: error: process Main transition 0 declares duplicate effect emit",
    },
    AuthorityAdmissionCase {
        stem: "effect_authority_unknown_runtime",
        mutation: AuthorityAdmissionMutation::UnknownEncodedEffect,
        diagnostic: "mantle: error: process.0.transition.0.effect.0: invalid effect value \"write\"",
    },
];

impl AuthorityAdmissionMutation {
    fn invalid_encoded_artifact(self, mut artifact: MantleArtifact) -> String {
        let effects = hello_start_transition_effects_mut(&mut artifact);
        assert_eq!(effects.as_slice(), &[ArtifactEffect::Emit]);

        match self {
            Self::MissingEmitAuthority => {
                *effects = vec![ArtifactEffect::Spawn];
                artifact.encode()
            }
            Self::UnusedSpawnAuthority => {
                *effects = vec![ArtifactEffect::Emit, ArtifactEffect::Spawn];
                artifact.encode()
            }
            Self::DuplicateEmitAuthority => {
                *effects = vec![ArtifactEffect::Emit, ArtifactEffect::Emit];
                artifact.encode()
            }
            Self::UnknownEncodedEffect => replace_exactly_once(
                &artifact.encode(),
                "process.0.transition.0.effect.0=emit\n",
                "process.0.transition.0.effect.0=write\n",
            ),
        }
    }
}

fn hello_start_transition_effects_mut(artifact: &mut MantleArtifact) -> &mut Vec<ArtifactEffect> {
    let main = artifact
        .processes
        .first_mut()
        .expect("hello artifact should define an entry process");
    assert_eq!(main.debug_name, "Main");
    let transition = main
        .transitions
        .first_mut()
        .expect("hello entry process should accept Start");
    &mut transition.effects
}

fn replace_exactly_once(input: &str, needle: &str, replacement: &str) -> String {
    assert_eq!(
        input.matches(needle).count(),
        1,
        "encoded artifact should contain {needle:?} exactly once"
    );
    input.replace(needle, replacement)
}

#[test]
fn hello_source_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run("examples/hello.str", "target/strata/hello.mta");

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("hello from Strata"));
    assert!(stdout.contains("mantle: stopped Main normally"));

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

#[test]
fn runtime_if_else_branches_on_payload_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_if_else");
    gate.check_build_run(
        "examples/runtime_if_else.str",
        "target/strata/runtime_if_else.mta",
    );

    let artifact = gate.read_artifact("target/strata/runtime_if_else.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have a Branch transition");
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::ReceivedPayload { ty },
            ..
        } if *ty == bool_type
    ));
    assert!(matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::ReceivedPayload { ty },
            then_actions,
            else_actions,
        }] if *ty == bool_type
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));

    let trace = gate.read_trace("runtime_if_else");
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker took warm branch""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker took cold branch""#
    ));
    assert!(trace.contains(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#
    ));
    assert!(trace.contains(r#""result":"Stop","state_id":1,"state":"WarmReady""#));
    assert!(trace.contains(r#""result":"Stop","state_id":2,"state":"ColdReady""#));

    let warm_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action""#,
    );
    let warm_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker""#,
    );
    let cold_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"action""#,
    );
    let cold_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker""#,
    );
    assert!(
        warm_branch < warm_output,
        "then branch trace must precede its effect"
    );
    assert!(
        cold_branch < cold_output,
        "else branch trace must precede its effect"
    );
}

#[test]
fn statement_if_before_final_runtime_if_traces_branch_at_action_position() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_if_statement_trace_order";
    const ARTIFACT: &str = "target/strata/runtime_if_statement_trace_order.mta";
    let source = gate.write_target_source(
        STEM,
        r#"
module runtime_if_statement_trace_order;

record MainState;
enum Bool { False, True }
enum MainMsg { Start }
enum WorkerState { Idle, WarmReady, ColdReady }
enum WorkerMsg { Branch(Bool) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let warm: ProcessRef<Worker> = spawn Worker;
        let cold: ProcessRef<Worker> = spawn Worker;
        send warm Branch(True);
        send cold Branch(False);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "prefix";
        if (flag) {
            emit "statement true";
        } else {
            emit "statement false";
        }
        if (flag) {
            return Stop(WarmReady);
        } else {
            return Stop(ColdReady);
        }
    }
}
"#,
    );
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    let run = gate.check_build_run(source, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("prefix"));
    assert!(stdout.contains("statement true"));
    assert!(stdout.contains("statement false"));

    let trace = gate.read_trace(STEM);
    let warm_next_state_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"next_state""#,
    );
    let warm_action_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action""#,
    );
    let cold_next_state_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"next_state""#,
    );
    let cold_action_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"action""#,
    );

    let warm_prefix = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"prefix""#,
    );
    let warm_statement = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"statement true""#,
    );
    assert!(warm_next_state_branch < warm_prefix);
    assert!(warm_prefix < warm_action_branch);
    assert!(warm_action_branch < warm_statement);

    let cold_prefix = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"prefix""#,
    );
    let cold_statement = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":2,"text":"statement false""#,
    );
    assert!(cold_next_state_branch < cold_prefix);
    assert!(cold_prefix < cold_action_branch);
    assert!(cold_action_branch < cold_statement);
}

#[test]
fn runtime_for_each_iterates_over_payload_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_for_each");
    let run = gate.check_build_run(
        "examples/runtime_for_each.str",
        "target/strata/runtime_for_each.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Branch(True) to Worker"));
    assert!(stdout.contains("mantle: delivered Branch(False) to Worker"));
    assert!(stdout.contains("worker handled true"));
    assert!(stdout.contains("worker handled false"));

    let artifact = gate.read_artifact("target/strata/runtime_for_each.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");
    let batch_payload_type = batch_worker.message_variants[0]
        .payload_type
        .expect("Batch message should carry a list payload");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { ty },
                max_items: 2,
                body,
            },
        ] if element.ty == bool_type
            && *ty == batch_payload_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::Send {
                    payload: Some(ArtifactValueTemplate::LoopElement {
                        ty,
                        element: payload_element,
                    }),
                    ..
                }] if *ty == bool_type && *payload_element == element.id
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop element dispatch must not rely on the source binding name"
    );

    let trace = gate.read_trace("runtime_for_each");
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""process":"BatchWorker""#,
            r#""element_id":0"#,
            r#""max_items":2"#,
            r#""item_count":2"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":0"#,
            r#""element":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":1"#,
            r#""element":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""process":"BatchWorker""#,
            r#""iteration_count":2"#,
        ],
    );

    let loop_start = trace_line_index(
        &trace,
        r#""event":"loop_started","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let first_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let second_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":0,"text":"worker handled true""#,
    );
    let false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":1,"text":"worker handled false""#,
    );

    assert!(loop_start < first_iteration);
    assert!(first_iteration < first_send);
    assert!(first_send < second_iteration);
    assert!(second_iteration < second_send);
    assert!(second_send < loop_complete);
    assert!(loop_complete < true_output);
    assert!(true_output < false_output);
}

#[test]
fn runtime_for_each_if_branches_inside_loop_body_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_for_each_if");
    let run = gate.check_build_run(
        "examples/runtime_for_each_if.str",
        "target/strata/runtime_for_each_if.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("batch selected true"));
    assert!(stdout.contains("batch selected false"));
    assert!(stdout.contains("worker handled true"));
    assert!(stdout.contains("worker handled false"));

    let artifact = gate.read_artifact("target/strata/runtime_for_each_if.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
            },
        ] if element.ty == bool_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::IfElse {
                    condition: ArtifactValueTemplate::LoopElement {
                        ty,
                        element: condition_element,
                    },
                    then_actions,
                    else_actions,
                }] if *ty == bool_type
                    && *condition_element == element.id
                    && matches!(
                        then_actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Send {
                                payload: Some(ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: payload_element,
                                }),
                                ..
                            },
                        ] if *ty == bool_type && *payload_element == element.id
                    )
                    && matches!(
                        else_actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Send {
                                payload: Some(ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: payload_element,
                                }),
                                ..
                            },
                        ] if *ty == bool_type && *payload_element == element.id
                    )
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop branch artifact must not dispatch through the source loop binding name"
    );

    let trace = gate.read_trace("runtime_for_each_if");
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":0"#,
            r#""element":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );

    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let batch_true_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"then","scope":"action""#,
    );
    let batch_true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":0,"text":"batch selected true""#,
    );
    let true_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let batch_false_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"else","scope":"action""#,
    );
    let batch_false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":1,"text":"batch selected false""#,
    );
    let false_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let worker_true_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action""#,
    );
    let worker_true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":2,"text":"worker handled true""#,
    );
    let worker_false_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"action""#,
    );
    let worker_false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":3,"text":"worker handled false""#,
    );

    assert!(first_iteration < batch_true_branch);
    assert!(batch_true_branch < batch_true_output);
    assert!(batch_true_output < true_send);
    assert!(true_send < second_iteration);
    assert!(second_iteration < batch_false_branch);
    assert!(batch_false_branch < batch_false_output);
    assert!(batch_false_output < false_send);
    assert!(false_send < loop_complete);
    assert!(loop_complete < worker_true_branch);
    assert!(worker_true_branch < worker_true_output);
    assert!(worker_true_output < worker_false_branch);
    assert!(worker_false_branch < worker_false_output);
}

#[test]
fn runtime_for_each_if_rejects_inactive_branch_condition_loop_element_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_if_bad_condition_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_if_bad_condition.mta";
    let invalid_trace_stem = "runtime_for_each_if_bad_condition";

    gate.check("examples/runtime_for_each_if.str");
    gate.build("examples/runtime_for_each_if.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action.0.condition.loop_element=0\n",
        "process.1.transition.0.action.1.body_action.0.condition.loop_element=1\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process BatchWorker transition 0 if condition references inactive loop element id 1"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_if_rejects_malformed_branch_send_target_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_if_bad_target_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_if_bad_target.mta";
    let invalid_trace_stem = "runtime_for_each_if_bad_target";

    gate.check("examples/runtime_for_each_if.str");
    gate.build("examples/runtime_for_each_if.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action.0.then_action.1.target_process_ref=0\n",
        "process.1.transition.0.action.1.body_action.0.then_action.1.target_process_ref=1\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process BatchWorker references undefined process reference id 1"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_if_preflights_malformed_loop_bool_before_branch_effects() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_if_bad_loop_bool_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_if_bad_loop_bool.mta";
    let invalid_trace_stem = "runtime_for_each_if_bad_loop_bool";

    gate.check("examples/runtime_for_each_if.str");
    gate.build("examples/runtime_for_each_if.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.0.transition.0.action.1.payload_template.value=List[True,False]\n",
        "process.0.transition.0.action.1.payload_template.value=List[True,Maybe]\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("mantle: error: process Main transition 0 send payload.item.1 value Maybe is not a member of enum type Bool"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_empty_collection_runs_zero_body_iterations() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_for_each_empty");
    let run = gate.check_build_run(
        "examples/runtime_for_each_empty.str",
        "target/strata/runtime_for_each_empty.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Batch(List[]) to BatchWorker"));
    assert!(!stdout.contains("mantle: delivered Branch("));
    assert!(!stdout.contains("worker handled"));

    let artifact = gate.read_artifact("target/strata/runtime_for_each_empty.mta");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                max_items: 0,
                body,
                ..
            },
        ] if matches!(body.as_slice(), [ArtifactAction::Send { .. }])
    ));

    let trace = gate.read_trace("runtime_for_each_empty");
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""process":"BatchWorker""#,
            r#""max_items":0"#,
            r#""item_count":0"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""process":"BatchWorker""#,
            r#""iteration_count":0"#,
        ],
    );
    assert!(
        !trace.contains(r#""event":"loop_iteration""#),
        "empty runtime collection must not execute loop body"
    );
    assert!(
        !trace.contains(r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker""#),
        "empty runtime collection must not send loop body messages"
    );
}

#[test]
fn runtime_for_each_rejects_missing_artifact_body_block_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_missing_body_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_missing_body.mta";
    let invalid_trace_stem = "runtime_for_each_missing_body";

    gate.check("examples/runtime_for_each.str");
    gate.build("examples/runtime_for_each.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action_count=1\n",
        "",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: missing artifact field process.1.transition.0.action.1.body_action_count"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_rejects_inactive_artifact_loop_element_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_bad_element_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_bad_element.mta";
    let invalid_trace_stem = "runtime_for_each_bad_element";

    gate.check("examples/runtime_for_each.str");
    gate.build("examples/runtime_for_each.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action.0.payload_template.loop_element=0\n",
        "process.1.transition.0.action.1.body_action.0.payload_template.loop_element=1\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process BatchWorker transition 0 send payload references inactive loop element id 1"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_rejects_malformed_runtime_collection_value_fail_closed() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_malformed_collection_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_malformed_collection.mta";
    let invalid_trace_stem = "runtime_for_each_malformed_collection";

    gate.check("examples/runtime_for_each.str");
    gate.build("examples/runtime_for_each.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.0.transition.0.action.1.payload_template.value=List[True,False]\n",
        "process.0.transition.0.action.1.payload_template.value=True\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process Main transition 0 send payload value True does not match list type"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn trace_line_index(trace: &str, needle: &str) -> usize {
    trace
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("trace should contain {needle:?}\n{trace}"))
}

#[test]
fn actor_payload_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_match.str",
        "target/strata/actor_payload_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Assign(Job{phase:Ready}) to Worker"));
    assert!(stdout.contains("worker matched Assign payload"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_match.mta");
    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("actor_payload_match");
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{job:Job{phase:Done}}","to_state_id":1,"to":"WorkerState{job:Job{phase:Ready}}""#));
    assert!(trace.contains(&format!(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}","result":"Stop","state_id":1,"state":"WorkerState{{job:Job{{phase:Ready}}}}""#
    )));
}

#[test]
fn actor_payload_split_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_split_match.str",
        "target/strata/actor_payload_split_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled Done assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_split_match.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    let worker_message = worker.transitions[0].message;
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.payload_guard.is_some()),
        "same-message split should lower exact typed payload guards"
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard")
                .value
                .clone()
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(
        payload_guards,
        [
            artifact_value("Assign(Done)"),
            artifact_value("Assign(Ready)")
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific dispatch must not lower constructor names as executable fields"
    );

    let routed_type = value_type_id(&artifact, "Routed");
    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_split_match");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_split_signature_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_split_signature.str",
        "target/strata/actor_payload_split_signature.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled Done assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_split_signature.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    let worker_message = worker.transitions[0].message;
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.payload_guard.is_some()),
        "same-message signature split should lower exact typed payload guards"
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard")
                .value
                .clone()
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(
        payload_guards,
        [
            artifact_value("Assign(Done)"),
            artifact_value("Assign(Ready)")
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific signature dispatch must not lower constructor names as executable fields"
    );

    let routed_type = value_type_id(&artifact, "Routed");
    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_split_signature");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_split_signature_wildcard_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_split_signature_wildcard.str",
        "target/strata/actor_payload_split_signature_wildcard.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled fallback assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_split_signature_wildcard.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    let worker_message = worker.transitions[0].message;
    let routed_type = value_type_id(&artifact, "Routed");
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.payload_guard.is_some()),
        "same-message signature wildcard fallback should lower exact typed payload guards"
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard");
            assert_eq!(guard.ty, routed_type);
            guard.value.clone()
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(
        payload_guards,
        [
            artifact_value("Assign(Done)"),
            artifact_value("Assign(Ready)")
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific signature wildcard must not lower constructor names as executable fields"
    );

    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_split_signature_wildcard");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_state_match_split_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_state_match_split.str",
        "target/strata/actor_payload_state_match_split.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled Done assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_state_match_split.mta");
    let worker = artifact_process(&artifact, "Worker");
    let worker_message = worker.transitions[0].message;
    let routed_type = value_type_id(&artifact, "Routed");

    assert_eq!(worker.transitions.len(), 6);
    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "SawReady");
    assert_eq!(worker.state_values[2].label, "Done");
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.current_state.is_some()
                && transition
                    .payload_guard
                    .as_ref()
                    .is_some_and(|guard| guard.ty == routed_type)),
        "state-match payload split should lower message, current-state, and exact typed payload guards"
    );

    let mut transition_keys = worker
        .transitions
        .iter()
        .map(|transition| {
            let current_state = transition
                .current_state
                .expect("state-match transition should carry current state");
            let state_label = worker.state_values[current_state.index()].label.clone();
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("state-match transition should carry payload guard");
            let result_label = match transition.step_result {
                mantle_artifact::StepResult::Continue => "Continue",
                mantle_artifact::StepResult::Stop => "Stop",
                mantle_artifact::StepResult::Panic => "Panic",
            };
            (state_label, guard.value.clone(), result_label)
        })
        .collect::<Vec<_>>();
    transition_keys.sort();

    assert_eq!(
        transition_keys,
        [
            ("Done".to_string(), artifact_value("Assign(Done)"), "Stop"),
            ("Done".to_string(), artifact_value("Assign(Ready)"), "Stop"),
            ("Idle".to_string(), artifact_value("Assign(Done)"), "Stop"),
            (
                "Idle".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Done)"),
                "Stop"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
        ]
    );

    let encoded = artifact.encode();
    assert!(encoded.contains(".current_state="));
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific state-match dispatch must not lower constructor names as executable fields"
    );

    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_state_match_split");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"state_updated""#,
            r#""process":"Worker""#,
            r#""from_state_id":0"#,
            r#""from":"Idle""#,
            r#""to_state_id":1"#,
            r#""to":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_state_match_wildcard_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_state_match_wildcard.str",
        "target/strata/actor_payload_state_match_wildcard.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled wildcard assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_state_match_wildcard.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 6);

    let worker_message = worker.transitions[0].message;
    let routed_type = value_type_id(&artifact, "Routed");

    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "SawReady");
    assert_eq!(worker.state_values[2].label, "Done");
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.current_state.is_some()
                && transition
                    .payload_guard
                    .as_ref()
                    .is_some_and(|guard| guard.ty == routed_type)),
        "state-match wildcard fallback should lower current-state and exact typed payload guards"
    );

    let mut transition_keys = worker
        .transitions
        .iter()
        .map(|transition| {
            let current_state = transition
                .current_state
                .expect("state-match transition should carry current state");
            let state_label = worker.state_values[current_state.index()].label.clone();
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("state-match transition should carry payload guard");
            let result_label = match transition.step_result {
                mantle_artifact::StepResult::Continue => "Continue",
                mantle_artifact::StepResult::Stop => "Stop",
                mantle_artifact::StepResult::Panic => "Panic",
            };
            (state_label, guard.value.clone(), result_label)
        })
        .collect::<Vec<_>>();
    transition_keys.sort();

    assert_eq!(
        transition_keys,
        [
            ("Done".to_string(), artifact_value("Assign(Done)"), "Stop"),
            ("Done".to_string(), artifact_value("Assign(Ready)"), "Stop"),
            ("Idle".to_string(), artifact_value("Assign(Done)"), "Stop"),
            (
                "Idle".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Done)"),
                "Stop"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
        ]
    );

    let encoded = artifact.encode();
    assert!(encoded.contains(".current_state="));
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific state-match wildcard fallback must not lower constructor names as executable fields"
    );

    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_state_match_wildcard");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"state_updated""#,
            r#""process":"Worker""#,
            r#""from_state_id":0"#,
            r#""from":"Idle""#,
            r#""to_state_id":1"#,
            r#""to":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn function_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_match.str",
        "target/strata/function_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source functions selected WarmReady"));
    assert!(stdout.contains("process helper assigned job"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/function_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.debug_name, "Main");
    assert_eq!(main.init_state, mantle_artifact::StateId::new(0));
    assert_eq!(
        main.state_values[0].label,
        "MainState{signature:WarmReady,body:WarmReady}"
    );

    let worker = &artifact.processes[1];
    assert_eq!(worker.debug_name, "Worker");
    assert_eq!(
        worker.state_values[0].label,
        "WorkerState{job:Job{phase:Done}}"
    );
    assert_eq!(
        worker.state_values[1].label,
        "WorkerState{job:Job{phase:Ready}}"
    );
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::Record {
            ty: value_type_id(&artifact, "WorkerState"),
            fields: vec![ArtifactValueTemplateField {
                name: "job".to_string(),
                value: ArtifactValueTemplate::ReceivedPayload {
                    ty: value_type_id(&artifact, "Job"),
                },
            }],
        })
    );

    let trace = gate.read_trace("function_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{signature:WarmReady,body:WarmReady}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source functions selected WarmReady""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{job:Job{phase:Done}}","to_state_id":1,"to":"WorkerState{job:Job{phase:Ready}}""#
    ));
}

#[test]
fn function_payload_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_payload_match.str",
        "target/strata/function_payload_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source helper matched payload enum"));
    assert!(stdout.contains("process helper wrapped payload enum"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/function_payload_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{signature:Active(Job{phase:Ready}),body:Active(Job{phase:Done})}"
    );

    let worker = &artifact.processes[1];
    assert_eq!(worker.state_values[0].label, "WorkerState{work:Empty}");
    assert_eq!(
        worker.state_values[1].label,
        "WorkerState{work:Assigned(Job{phase:Ready})}"
    );
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::Record {
            ty: value_type_id(&artifact, "WorkerState"),
            fields: vec![ArtifactValueTemplateField {
                name: "work".to_string(),
                value: ArtifactValueTemplate::EnumVariant {
                    ty: value_type_id(&artifact, "Work"),
                    variant: EnumVariantId::new(1),
                    payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                        ty: value_type_id(&artifact, "Job"),
                    }),
                },
            }],
        })
    );

    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("function_payload_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{signature:Active(Job{phase:Ready}),body:Active(Job{phase:Done})}""#
    ));
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{work:Empty}","to_state_id":1,"to":"WorkerState{work:Assigned(Job{phase:Ready})}""#
    ));
}

#[test]
fn function_if_else_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_if_else.str",
        "target/strata/function_if_else.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("pure conditional selected source values"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_if_else.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values.len(), 2);
    assert_eq!(
        main.state_values[0].label,
        "MainState{init:WarmReady,step:ColdReady}"
    );
    assert_eq!(
        main.state_values[1].label,
        "MainState{init:ColdReady,step:WarmReady}"
    );
    let encoded = artifact.encode();
    assert!(!encoded.contains("is_warm"));
    assert!(!encoded.contains("choose"));
    assert!(!encoded.contains("readiness"));

    let trace = gate.read_trace("function_if_else");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{init:WarmReady,step:ColdReady}""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":1,"process_id":0,"process":"Main","from_state_id":0,"from":"MainState{init:WarmReady,step:ColdReady}","to_state_id":1,"to":"MainState{init:ColdReady,step:WarmReady}""#
    ));
}

#[test]
fn function_collection_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_collection_match.str",
        "target/strata/function_collection_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source helper collection match selected values"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_collection_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{selected:Ready,tail:List[Done]}"
    );

    let trace = gate.read_trace("function_collection_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{selected:Ready,tail:List[Done]}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source helper collection match selected values""#
    ));
}

#[test]
fn nested_patterns_check_build_and_run_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/nested_patterns.str",
        "target/strata/nested_patterns.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));

    let trace = gate.read_trace("nested_patterns");
    assert!(trace.contains("\"event\":\"artifact_loaded\""));

    let artifact = gate.read_artifact("target/strata/nested_patterns.mta");
    let encoded = artifact.encode();
    assert!(
        encoded.contains(".kind=enum_payload"),
        "nested constructor projection should lower as typed enum payload templates"
    );
}

#[test]
fn function_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_return_match.str",
        "target/strata/function_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source helper return match selected payload"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_return_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{status:Active(Job{phase:Ready})}"
    );

    let trace = gate.read_trace("function_return_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{status:Active(Job{phase:Ready})}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source helper return match selected payload""#
    ));
}

#[test]
fn process_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/process_return_match.str",
        "target/strata/process_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout
            .matches("process return match uniform prefix")
            .count(),
        2
    );

    let artifact = gate.read_artifact("target/strata/process_return_match.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(
        worker
            .state_values
            .iter()
            .map(|state| state.label.as_str())
            .collect::<Vec<_>>(),
        ["Idle", "SawReady", "Done"]
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .map(|payload| payload.value.label())
                .expect("process return-match transition should have a payload guard")
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(payload_guards, ["Assign(Done)", "Assign(Ready)"]);
    for transition in &worker.transitions {
        assert_eq!(transition.effects, [ArtifactEffect::Emit]);
        assert!(
            matches!(transition.actions.as_slice(), [ArtifactAction::Emit { .. }]),
            "process return-match prefix must lower as one typed emit action"
        );
    }
    let encoded = artifact.encode();
    assert!(
        !encoded.contains("field_name=Assign"),
        "process return-match must not lower constructor names as executable fields"
    );

    let trace = gate.read_trace("process_return_match");
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_eq!(
        trace
            .matches(r#""text":"process return match uniform prefix""#)
            .count(),
        2
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn function_record_pattern_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_record_pattern.str",
        "target/strata/function_record_pattern.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source helper record pattern selected field"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_record_pattern.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values[0].label, "MainState{phase:Ready}");

    let trace = gate.read_trace("function_record_pattern");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{phase:Ready}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source helper record pattern selected field""#
    ));
}

#[test]
fn function_record_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_record_return_match.str",
        "target/strata/function_record_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source helper record return match selected field"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_record_return_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values[0].label, "MainState{phase:Ready}");

    let trace = gate.read_trace("function_record_return_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{phase:Ready}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source helper record return match selected field""#
    ));
}

#[test]
fn function_record_body_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_record_body_match.str",
        "target/strata/function_record_body_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source helper record body match selected field"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_record_body_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values[0].label, "MainState{phase:Ready}");

    let trace = gate.read_trace("function_record_body_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{phase:Ready}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source helper record body match selected field""#
    ));
}

#[test]
fn state_payload_enum_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/state_payload_enum.str",
        "target/strata/state_payload_enum.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("worker entered payload state"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/state_payload_enum.mta");
    let worker = &artifact.processes[1];
    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "Working(Job{phase:Ready})");
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: value_type_id(&artifact, "WorkerState"),
            variant: EnumVariantId::new(1),
            payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                ty: value_type_id(&artifact, "Job"),
            }),
        })
    );

    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("state_payload_enum");
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Idle","to_state_id":1,"to":"Working(Job{phase:Ready})""#
    ));
}

#[test]
fn collection_state_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/collection_state.str",
        "target/strata/collection_state.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("collection state replaced"));
    assert!(stdout.contains("collection map state replaced"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert!(stdout.contains("mantle: stopped MapWorker normally"));

    let artifact = gate.read_artifact("target/strata/collection_state.mta");
    let worker = &artifact.processes[1];
    let list_type = value_type_id(&artifact, "__strata_checked_4_List_1_1_5_Phase_1");
    let payload_list_type = value_type_id(&artifact, "__strata_checked_4_List_1_1_5_Phase_2");
    let phase_type = value_type_id(&artifact, "Phase");
    assert_eq!(worker.state_values[0].label, "List[Ready]");
    assert_eq!(worker.state_values[1].label, "List[Done]");
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::ListRest {
            ty: list_type,
            list: Box::new(ArtifactValueTemplate::ReceivedPayload {
                ty: payload_list_type,
            }),
            prefix_len: 1,
        })
    );

    let map_worker = &artifact.processes[2];
    let map_type = value_type_id(&artifact, "__strata_checked_3_Map_2_1_5_Phase_5_Phase_2");
    assert_eq!(
        map_worker.state_values[0].label,
        "Map[Ready=>Ready,Done=>Ready]"
    );
    assert_eq!(
        map_worker.state_values[1].label,
        "Map[Ready=>Done,Done=>Ready]"
    );
    assert_eq!(
        map_worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::Map {
            ty: map_type,
            entries: vec![
                ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: phase_type,
                        value: artifact_value("Ready"),
                    },
                    value: ArtifactValueTemplate::MapValue {
                        ty: phase_type,
                        map: Box::new(ArtifactValueTemplate::ReceivedPayload { ty: map_type }),
                        key: artifact_value("Ready"),
                        keys: vec![artifact_value("Ready")],
                        projection: mantle_artifact::MapProjectionMode::Subset,
                    },
                },
                ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: phase_type,
                        value: artifact_value("Done"),
                    },
                    value: ArtifactValueTemplate::Literal {
                        ty: phase_type,
                        value: artifact_value("Ready"),
                    },
                },
            ],
        })
    );

    let trace = gate.read_trace("collection_state");
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"collection state replaced""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"List[Ready]","to_state_id":1,"to":"List[Done]""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":3,"process_id":2,"process":"MapWorker","stream":"stdout","output_id":1,"text":"collection map state replaced""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":3,"process_id":2,"process":"MapWorker","from_state_id":0,"from":"Map[Ready=>Ready,Done=>Ready]","to_state_id":1,"to":"Map[Ready=>Done,Done=>Ready]""#
    ));
}

#[test]
fn state_payload_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/state_payload_match.str",
        "target/strata/state_payload_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("worker accepted job"));
    assert!(stdout.contains("worker completed job"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/state_payload_match.mta");
    let worker = &artifact.processes[1];
    let job_type = value_type_id(&artifact, "Job");
    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "Working(Job{phase:Ready})");
    assert_eq!(worker.state_values[2].label, "Done(Job{phase:Ready})");
    assert_eq!(
        worker.state_values[1].payload.as_ref(),
        Some(&mantle_artifact::ArtifactPayload {
            ty: job_type,
            value: artifact_value("Job{phase:Ready}"),
            process_ref: None,
        })
    );
    assert_eq!(
        worker.state_values[2].payload.as_ref(),
        Some(&mantle_artifact::ArtifactPayload {
            ty: job_type,
            value: artifact_value("Job{phase:Ready}"),
            process_ref: None,
        })
    );
    assert_eq!(worker.transitions.len(), 4);
    assert_eq!(worker.transitions[0].current_state, None);
    assert_eq!(
        worker.transitions[1].current_state,
        Some(mantle_artifact::StateId::new(0))
    );
    assert_eq!(
        worker.transitions[2].current_state,
        Some(mantle_artifact::StateId::new(1))
    );
    assert_eq!(
        worker.transitions[3].current_state,
        Some(mantle_artifact::StateId::new(2))
    );
    assert_eq!(
        worker.transitions[2].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: value_type_id(&artifact, "WorkerState"),
            variant: EnumVariantId::new(2),
            payload: Box::new(ArtifactValueTemplate::CurrentStatePayload { ty: job_type }),
        })
    );

    let trace = gate.read_trace("state_payload_match");
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Idle","to_state_id":1,"to":"Working(Job{phase:Ready})""#
    ));
    assert!(trace.contains(
        r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Complete""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":2,"text":"worker completed job""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":1,"from":"Working(Job{phase:Ready})","to_state_id":2,"to":"Done(Job{phase:Ready})""#
    ));
    assert!(trace.contains(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Complete","result":"Stop","state_id":2,"state":"Done(Job{phase:Ready})""#
    ));
}

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
