use crate::support::{ArtifactAction, ArtifactCapabilityDescriptor, GateHarness};

#[test]
fn boundary_contracts_check_build_run_and_trace_typed_port() {
    let gate = GateHarness::new();
    gate.remove_trace("boundary_contracts_main");

    let output = gate.check_build_run(
        "examples/boundary_contracts_main.str",
        "target/strata/boundary_contracts_main.mta",
    );
    let stdout = String::from_utf8(output.stdout).expect("mantle stdout should be UTF-8");
    let artifact = gate.read_artifact("target/strata/boundary_contracts_main.mta");
    let trace = gate.read_trace("boundary_contracts_main");

    assert_eq!(artifact.protocols[0].debug_name, "WorkerProtocol");
    assert_eq!(artifact.ports[0].debug_name, "WorkerPort");
    assert_eq!(artifact.components[0].debug_name, "WorkerComponent");
    let main = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Main")
        .expect("Main artifact process should exist");
    assert!(main.authorities.iter().any(|authority| matches!(
        authority.descriptor,
        ArtifactCapabilityDescriptor::PortConnect { port } if port.as_u32() == 0
    )));
    assert!(main.transitions[0].actions.iter().any(|action| matches!(
        action,
        ArtifactAction::Send {
            port: Some(port),
            ..
        } if port.as_u32() == 0
    )));
    assert!(stdout.contains("boundary worker handled Work"));
    assert!(trace.contains(r#""event":"boundary_send_checked""#));
    assert!(trace.contains(r#""port":"WorkerPort""#));
    assert!(trace.contains(r#""protocol":"WorkerProtocol""#));
    assert!(trace.contains(r#""boundary_result":"accepted""#));
}
