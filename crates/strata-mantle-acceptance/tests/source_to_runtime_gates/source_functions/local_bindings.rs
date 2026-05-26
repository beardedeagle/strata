use crate::support::*;

#[test]
fn function_local_bindings_check_build_and_run_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_local_bindings.str",
        "target/strata/function_local_bindings.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("immutable source computation selected active"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_local_bindings.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{selected:Idle,echoed:Idle}"
    );
    assert_eq!(
        main.state_values[1].label,
        "MainState{selected:Active,echoed:Active}"
    );
    let encoded = artifact.encode();
    for source_only_name in [
        "current_local",
        "active_flag_local",
        "selected_local",
        "routed_local",
        "flags_local",
        "mapping_local",
        "selected_if_local",
        "selected_else_local",
        "echo_source_local",
        "echo_local",
        "route_value_local",
        "phase_local",
        "process_route",
        "select_phase",
        "echo_route",
        "status",
    ] {
        assert!(
            !encoded.contains(source_only_name),
            "{source_only_name} must not lower into executable artifact meaning"
        );
    }

    let trace = gate.read_trace("function_local_bindings");
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"immutable source computation selected active""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":1,"process_id":0,"process":"Main","from_state_id":0,"from":"MainState{selected:Idle,echoed:Idle}","to_state_id":1,"to":"MainState{selected:Active,echoed:Active}""#
    ));
}

#[test]
fn source_local_binding_process_ref_check_fails_closed() {
    let gate = GateHarness::new();
    let artifact = "target/strata/source_local_binding_process_ref.mta";
    gate.remove_artifact(artifact);

    let check = gate.check_failure("examples/failures/source_local_binding_process_ref.str");
    let stderr = String::from_utf8_lossy(&check.stderr);

    assert!(
        stderr.contains(
            "source-local binding worker_local must use a declared record, enum, scalar, list, or map type"
        ),
        "unexpected diagnostic\nstderr:\n{stderr}"
    );
    assert!(
        !gate.root.join(artifact).exists(),
        "source check failure must not create {artifact}"
    );
}

#[test]
fn source_local_binding_process_ref_carrier_enum_check_fails_closed() {
    let gate = GateHarness::new();
    let artifact = "target/strata/source_local_binding_process_ref_carrier_enum.mta";
    gate.remove_artifact(artifact);

    let check =
        gate.check_failure("examples/failures/source_local_binding_process_ref_carrier_enum.str");
    let stderr = String::from_utf8_lossy(&check.stderr);

    assert!(
        stderr.contains(
            "source-local binding copy must use a declared record, enum, scalar, list, or map type without process-reference authority"
        ),
        "unexpected diagnostic\nstderr:\n{stderr}"
    );
    assert!(
        !gate.root.join(artifact).exists(),
        "source check failure must not create {artifact}"
    );
}

#[test]
fn source_function_parameter_process_ref_shadow_check_fails_closed() {
    let gate = GateHarness::new();
    let artifact = "target/strata/source_function_parameter_process_ref_shadow.mta";
    gate.remove_artifact(artifact);

    let check =
        gate.check_failure("examples/failures/source_function_parameter_process_ref_shadow.str");
    let stderr = String::from_utf8_lossy(&check.stderr);

    assert!(
        stderr.contains(
            "source function parameter worker conflicts with a process reference binding"
        ),
        "unexpected diagnostic\nstderr:\n{stderr}"
    );
    assert!(
        !gate.root.join(artifact).exists(),
        "source check failure must not create {artifact}"
    );
}
