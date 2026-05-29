use crate::support::{ArtifactAction, ArtifactCapabilityDescriptor, GateHarness, ProcessId};

#[test]
fn imports_main_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    gate.remove_trace("imports_main");

    let output = gate.check_build_run(
        "examples/imports_main.str",
        "target/strata/imports_main.mta",
    );
    let stdout = String::from_utf8(output.stdout).expect("mantle stdout should be UTF-8");
    let artifact = gate.read_artifact("target/strata/imports_main.mta");
    let trace = gate.read_trace("imports_main");

    assert_eq!(artifact.module, "imports_main");
    let worker_id = process_id(&artifact, "Worker");
    let main = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Main")
        .expect("Main artifact process should exist");
    assert_eq!(
        main.authorities[0].descriptor,
        ArtifactCapabilityDescriptor::Spawn { target: worker_id }
    );
    assert!(
        main.transitions[0]
            .actions
            .iter()
            .any(|action| matches!(action, ArtifactAction::Send { .. })),
        "imported Worker send should lower as typed artifact IDs"
    );
    assert!(stdout.contains("imported worker handled completed job"));
    assert!(trace.contains(r#""module":"imports_main""#));
    assert!(trace.contains(r#""event":"program_output""#));
    assert!(trace.contains(r#""payload":"Job{phase:Done}""#));
}

fn process_id(artifact: &crate::support::MantleArtifact, name: &str) -> ProcessId {
    let index = artifact
        .processes
        .iter()
        .position(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("{name} artifact process should exist"));
    ProcessId::from_index(index).expect("artifact process index should fit")
}
