use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Once;

pub(crate) use mantle_artifact::{
    ArtifactAction, ArtifactEffect, ArtifactProcess, ArtifactSendTarget, ArtifactTransition,
    ArtifactTypeKind, ArtifactValue, ArtifactValueBooleanOperator, ArtifactValueEqualityOperator,
    ArtifactValueShape, ArtifactValueTemplate, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, EffectOutcomeId, EnumVariantId, MantleArtifact, MessageId,
    NextState, ProcessId, StateId, TypeId, read_artifact,
};

static BUILD_WORKSPACE_BINS: Once = Once::new();

pub(crate) struct GateHarness {
    pub(crate) root: PathBuf,
    strata: PathBuf,
    mantle: PathBuf,
}

pub(crate) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

impl GateHarness {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn check_build_run(&self, source: &str, artifact: &str) -> Output {
        self.check(source);
        self.build(source, artifact);
        self.run_mantle_success(artifact)
    }

    pub(crate) fn check(&self, source: &str) {
        assert_success(
            self.command(&self.strata, ["check", source], "strata check"),
            "strata check",
        );
    }

    pub(crate) fn check_failure(&self, source: &str) -> Output {
        assert_failure(
            self.command(&self.strata, ["check", source], "strata check"),
            "strata check",
        )
    }

    pub(crate) fn build(&self, source: &str, artifact: &str) {
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

    pub(crate) fn run_mantle_success(&self, artifact: &str) -> Output {
        assert_success(
            self.command(&self.mantle, ["run", artifact], "mantle run"),
            "mantle run",
        )
    }

    pub(crate) fn run_mantle_failure(&self, artifact: &str) -> Output {
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

    pub(crate) fn remove_artifact(&self, artifact: &str) {
        let path = self.root.join(artifact);
        remove_file_if_exists(&path);
    }

    pub(crate) fn remove_trace(&self, stem: &str) {
        let path = self.trace_path(stem);
        remove_file_if_exists(&path);
    }

    pub(crate) fn read_trace(&self, stem: &str) -> String {
        let trace_path = self.trace_path(stem);
        fs::read_to_string(&trace_path)
            .unwrap_or_else(|err| panic!("expected trace {}: {err}", trace_path.display()))
    }

    pub(crate) fn read_artifact(&self, artifact: &str) -> MantleArtifact {
        read_artifact(&self.root.join(artifact))
            .unwrap_or_else(|err| panic!("expected artifact {artifact}: {err}"))
    }

    pub(crate) fn write_target_source(&self, stem: &str, source: &str) -> PathBuf {
        let path = self.root.join("target/strata").join(format!("{stem}.str"));
        fs::create_dir_all(path.parent().expect("target source should have a parent"))
            .unwrap_or_else(|err| panic!("could not create target source directory: {err}"));
        fs::write(&path, source).unwrap_or_else(|err| {
            panic!("could not write target source {}: {err}", path.display())
        });
        path
    }

    pub(crate) fn write_unvalidated_encoded_artifact(
        &self,
        artifact: &str,
        encoded_artifact: &str,
    ) {
        fs::write(self.root.join(artifact), encoded_artifact)
            .unwrap_or_else(|err| panic!("could not write artifact {artifact}: {err}"));
    }

    pub(crate) fn trace_exists(&self, stem: &str) -> bool {
        self.trace_path(stem).exists()
    }

    fn trace_path(&self, stem: &str) -> PathBuf {
        self.root
            .join("target/strata")
            .join(format!("{stem}.observability.jsonl"))
    }
}

pub(crate) fn value_type_id(artifact: &MantleArtifact, label: &str) -> TypeId {
    artifact_type_id(artifact, label, ArtifactTypeKind::Value)
}

pub(crate) fn process_ref_type_id(artifact: &MantleArtifact, target: ProcessId) -> TypeId {
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

pub(crate) fn artifact_type_id(
    artifact: &MantleArtifact,
    label: &str,
    kind: ArtifactTypeKind,
) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.label == label && ty.kind == kind)
        .unwrap_or_else(|| panic!("artifact type {label} with kind {kind:?} should exist"));
    TypeId::from_index(index).expect("artifact type index should fit")
}

pub(crate) fn artifact_process<'a>(
    artifact: &'a MantleArtifact,
    process: &str,
) -> &'a ArtifactProcess {
    artifact
        .processes
        .iter()
        .find(|candidate| candidate.debug_name == process)
        .unwrap_or_else(|| panic!("artifact process {process} should exist"))
}

pub(crate) fn artifact_process_id(artifact: &MantleArtifact, process: &str) -> ProcessId {
    let index = artifact
        .processes
        .iter()
        .position(|candidate| candidate.debug_name == process)
        .unwrap_or_else(|| panic!("artifact process {process} should exist"));
    ProcessId::from_index(index).expect("artifact process index should fit")
}

pub(crate) fn message_id(process: &ArtifactProcess, label: &str) -> MessageId {
    let index = process
        .message_variants
        .iter()
        .position(|message| message.label == label)
        .unwrap_or_else(|| panic!("artifact message {label} should exist"));
    MessageId::from_index(index).expect("artifact message index should fit")
}

pub(crate) fn state_id(process: &ArtifactProcess, label: &str) -> StateId {
    let index = process
        .state_values
        .iter()
        .position(|state| state.ty == process.state_type && state.label == label)
        .unwrap_or_else(|| panic!("artifact state {label} should exist"));
    StateId::from_index(index).expect("artifact state index should fit")
}

pub(crate) fn assert_trace_event(trace: &str, fields: &[&str]) {
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

pub(crate) fn trace_line_index_with_fields(trace: &str, fields: &[&str]) -> usize {
    assert!(
        !fields.is_empty(),
        "trace line assertion should require at least one field"
    );
    trace
        .lines()
        .position(|line| fields.iter().all(|field| line.contains(field)))
        .unwrap_or_else(|| panic!("trace should contain fields {fields:?}\n{trace}"))
}

pub(crate) fn transition_effects<'a>(
    artifact: &'a MantleArtifact,
    process: &str,
) -> &'a [ArtifactEffect] {
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
pub(crate) struct AuthorityAdmissionCase {
    pub(crate) stem: &'static str,
    pub(crate) mutation: AuthorityAdmissionMutation,
    pub(crate) diagnostic: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AuthorityAdmissionMutation {
    MissingEmitAuthority,
    UnusedSpawnAuthority,
    DuplicateEmitAuthority,
    UnknownEncodedEffect,
}

pub(crate) const AUTHORITY_ADMISSION_CASES: [AuthorityAdmissionCase; 4] = [
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
    pub(crate) fn invalid_encoded_artifact(self, mut artifact: MantleArtifact) -> String {
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

pub(crate) fn replace_exactly_once(input: &str, needle: &str, replacement: &str) -> String {
    assert_eq!(
        input.matches(needle).count(),
        1,
        "encoded artifact should contain {needle:?} exactly once"
    );
    input.replace(needle, replacement)
}

pub(crate) fn trace_line_index(trace: &str, needle: &str) -> usize {
    trace
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("trace should contain {needle:?}\n{trace}"))
}
