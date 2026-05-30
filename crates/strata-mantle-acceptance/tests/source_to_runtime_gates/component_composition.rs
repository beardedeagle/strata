use crate::support::{ArtifactAction, GateHarness};

#[test]
fn component_composition_checks_builds_runs_and_admits_typed_graph() {
    let gate = GateHarness::new();
    gate.remove_trace("component_composition_main");

    let output = gate.check_build_run(
        "examples/component_composition_main.str",
        "target/strata/component_composition_main.mta",
    );
    let stdout = String::from_utf8(output.stdout).expect("mantle stdout should be UTF-8");
    let artifact = gate.read_artifact("target/strata/component_composition_main.mta");
    let trace = gate.read_trace("component_composition_main");

    assert_eq!(artifact.compositions.len(), 1);
    assert_eq!(artifact.compositions[0].debug_name, "AppComposition");
    assert_eq!(artifact.compositions[0].component_instances.len(), 2);
    assert_eq!(artifact.compositions[0].port_bindings.len(), 1);
    assert!(artifact.components.iter().any(|component| {
        component.debug_name == "MainComponent" && component.import_ports.len() == 1
    }));
    assert!(artifact.processes.iter().any(|process| {
        process.debug_name == "Main"
            && process.transitions[0]
                .actions
                .iter()
                .any(|action| matches!(action, ArtifactAction::Send { port: Some(_), .. }))
    }));
    assert!(stdout.contains("composed worker handled Work"));
    assert!(trace.contains(r#""event":"boundary_send_checked""#));
    assert!(trace.contains(r#""boundary_result":"accepted""#));
}
