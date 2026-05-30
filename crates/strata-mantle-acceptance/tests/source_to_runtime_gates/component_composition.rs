use crate::support::{ArtifactAction, GateHarness};

#[test]
fn component_composition_checks_builds_runs_and_admits_typed_graph() {
    let gate = GateHarness::new();
    gate.remove_trace("component_composition_main");

    let output = gate.check_build_run(
        "examples/component_composition_main.str",
        "target/strata/component_composition_main.mta",
    );
    let report_output = gate.composition_report("examples/component_composition_main.str", "json");
    let stdout = String::from_utf8(output.stdout).expect("mantle stdout should be UTF-8");
    let report =
        String::from_utf8(report_output.stdout).expect("composition report should be UTF-8");
    let artifact = gate.read_artifact("target/strata/component_composition_main.mta");
    let trace = gate.read_trace("component_composition_main");

    assert_eq!(artifact.compositions.len(), 1);
    assert_eq!(artifact.compositions[0].debug_name, "AppComposition");
    assert_eq!(artifact.compositions[0].component_instances.len(), 2);
    assert_eq!(artifact.compositions[0].port_bindings.len(), 1);
    let binding = &artifact.compositions[0].port_bindings[0];
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
    assert!(report.contains("\"report_format\":\"strata.component_composition_admission_report\""));
    assert!(report.contains("\"source_hash_algorithm\":\"fnv1a64-diagnostic\""));
    assert_checked_report_binding(&report, binding);
    assert!(trace.contains(r#""event":"boundary_send_checked""#));
    assert!(trace.contains(r#""boundary_result":"accepted""#));
}

fn assert_checked_report_binding(report: &str, binding: &mantle_artifact::ArtifactPortBinding) {
    assert!(report.contains("\"admission_result\":\"admitted\""));
    assert!(report.contains("\"unsatisfied_imports\":[]"));
    assert!(report.contains(&format!(
        r#""port_bindings":[{{"port_binding_id":0,"importer_instance_id":{},"importer_instance":"main","imported_port_id":{},"imported_port":"WorkerPort","exporter_instance_id":{},"exporter_instance":"worker","exported_port_id":{},"exported_port":"WorkerPort","protocol_id":0,"protocol":"WorkerProtocol","binding_result":"admitted","imported_port_authority":{{"kind":"port_connect","port_id":{},"port":"WorkerPort"}},"exported_port_authority":{{"kind":"port_connect","port_id":{},"port":"WorkerPort"}}}}]"#,
        binding.importer.as_u32(),
        binding.imported_port.as_u32(),
        binding.exporter.as_u32(),
        binding.exported_port.as_u32(),
        binding.imported_port.as_u32(),
        binding.exported_port.as_u32(),
    )));
    assert!(report.contains(&format!(
        r#""authority_edges":[{{"port_binding_id":0,"edge_kind":"port_binding","exporter_component_id":0,"exporter_component":"WorkerComponent","importer_component_id":1,"importer_component":"MainComponent","exported_port_id":{},"exported_port":"WorkerPort","imported_port_id":{},"imported_port":"WorkerPort","protocol_id":0,"protocol":"WorkerProtocol","export_authority":{{"kind":"component_export","component_id":0,"component":"WorkerComponent"}},"exported_port_authority":{{"kind":"port_connect","port_id":{},"port":"WorkerPort"}},"imported_port_authority":{{"kind":"port_connect","port_id":{},"port":"WorkerPort"}}}}]"#,
        binding.exported_port.as_u32(),
        binding.imported_port.as_u32(),
        binding.exported_port.as_u32(),
        binding.imported_port.as_u32(),
    )));
}
