use crate::support::{ArtifactAction, GateHarness, RuntimeFeature};

#[test]
fn component_composition_checks_builds_runs_and_admits_typed_graph() {
    let gate = GateHarness::new();
    gate.remove_trace("component_composition_main");

    let output = gate.check_build_run(
        "examples/component_composition_main.str",
        "target/strata/component_composition_main.mta",
    );
    let report_output = gate.composition_report("examples/component_composition_main.str", "json");
    gate.composition_build(
        "examples/component_composition_main.str",
        "target/strata/component_composition_main.component-composition.json",
    );
    let composition_artifact_output = gate.composition_admit(
        "target/strata/component_composition_main.component-composition.json",
        "json",
    );
    let requirements_output =
        gate.target_requirements("examples/component_composition_main.str", "json");
    let declaration_output = gate.feature_declaration("json");
    let admission_output = gate.admit("target/strata/component_composition_main.mta", "json");
    let stdout = String::from_utf8(output.stdout).expect("mantle stdout should be UTF-8");
    let report =
        String::from_utf8(report_output.stdout).expect("composition report should be UTF-8");
    let composition_artifact = gate
        .read_text_artifact("target/strata/component_composition_main.component-composition.json");
    let composition_admission = String::from_utf8(composition_artifact_output.stdout)
        .expect("composition artifact admission should be UTF-8");
    let requirements =
        String::from_utf8(requirements_output.stdout).expect("target requirements should be UTF-8");
    let declaration = String::from_utf8(declaration_output.stdout)
        .expect("runtime feature declaration should be UTF-8");
    let admission =
        String::from_utf8(admission_output.stdout).expect("runtime admission should be UTF-8");
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
    assert!(
        report.contains(
            "\"report_format\":\"strata.checked_component_composition_admission_report\""
        )
    );
    assert!(report.contains("\"source_hash_algorithm\":\"fnv1a64-diagnostic\""));
    assert!(
        composition_artifact.contains("\"schema_id\":\"strata.checked_component_composition\"")
    );
    assert!(composition_artifact.contains("\"schema_version_major\":1"));
    assert!(composition_artifact.contains("\"schema_version_minor\":0"));
    assert!(composition_artifact.contains("\"hash_alg\":\"fnv1a64-diagnostic\""));
    assert!(composition_artifact.contains("\"artifact_kind\":\"checked_component_composition\""));
    assert!(composition_artifact.contains("\"import_ports\":["));
    assert!(composition_artifact.contains("\"export_port\":{"));
    assert!(composition_artifact.contains("\"capability_bindings\":[]"));
    assert!(composition_artifact.contains("\"interface_bindings\":[]"));
    assert!(composition_artifact.contains("\"runtime_feature_bindings\":[]"));
    assert!(composition_artifact.contains("\"archive_format_bindings\":[]"));
    assert!(composition_artifact.contains("\"crypto_policy_bindings\":[]"));
    assert!(composition_artifact.contains("\"source_language\":\"strata\""));
    assert!(composition_artifact.contains("\"admission_result\":\"admitted\""));
    assert!(composition_artifact.contains("\"unsatisfied_imports\":[]"));
    assert!(
        composition_artifact
            .contains("\"cross_component_authority_edges\":[{\"port_binding_id\":0")
    );
    assert!(
        composition_admission.contains("\"schema_id\":\"strata.checked_component_composition\"")
    );
    assert!(composition_admission.contains("\"admission_result\":\"admitted\""));
    assert!(composition_admission.contains("\"unsatisfied_import_count\":0"));
    assert!(composition_admission.contains("\"port_binding_count\":1"));
    assert!(composition_admission.contains("\"authority_edge_count\":1"));
    assert!(requirements.contains("\"source_language\":\"strata\""));
    assert!(requirements.contains("\"component_composition_metadata\""));
    assert!(requirements.contains("\"typed_boundary_tables\""));
    assert!(declaration.contains("\"strata_version\":\"0.16.0\""));
    assert!(declaration.contains("\"source_language_support\":\"artifact_declared_metadata\""));
    assert!(declaration.contains("\"scheduler.reduction_window\":2000"));
    assert!(declaration.contains("\"wire_format.version\":\"mantle.archive.v1\""));
    assert!(declaration.contains("\"component_composition_metadata\""));
    assert!(declaration.contains("\"implementation_limits\":[\"distributed_transport\""));
    assert!(admission.contains("\"admitted\":true"));
    assert!(admission.contains("\"component_composition_metadata\""));
    assert!(
        artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::ComponentCompositionMetadata)
    );
    assert!(
        artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::TypedBoundaryTables)
    );
    assert_checked_report_binding(&report, binding);
    assert!(trace.contains(r#""event":"boundary_send_checked""#));
    assert!(trace.contains(r#""boundary_result":"accepted""#));
}

#[test]
fn component_composition_artifact_forgery_fails_closed() {
    let gate = GateHarness::new();
    let artifact = "target/strata/component_composition_forgery.component-composition.json";
    gate.composition_build("examples/component_composition_main.str", artifact);
    let original = gate.read_text_artifact(artifact);
    let forged = original.replace(
        "\"unsatisfied_imports\":[]",
        "\"unsatisfied_imports\":[{\"component_instance_id\":0,\"instance\":\"main\",\"imported_port_id\":0,\"imported_port\":\"WorkerPort\",\"reason\":\"forged missing binding\"}]",
    );
    gate.write_text_artifact(
        "target/strata/component_composition_main.forged.component-composition.json",
        &forged,
    );

    let failure = gate.composition_admit_failure(
        "target/strata/component_composition_main.forged.component-composition.json",
    );
    let stderr = String::from_utf8(failure.stderr)
        .expect("composition admit failure stderr should be UTF-8");

    assert!(
        stderr.contains("both bound and unsatisfied"),
        "unexpected stderr: {stderr}"
    );

    let forged_fingerprint =
        replace_string_field(&original, "source_fingerprint", "not-a-fnv-hash");
    gate.write_text_artifact(
        "target/strata/component_composition_main.bad-fingerprint.component-composition.json",
        &forged_fingerprint,
    );
    let fingerprint_failure = gate.composition_admit_failure(
        "target/strata/component_composition_main.bad-fingerprint.component-composition.json",
    );
    let fingerprint_stderr = String::from_utf8(fingerprint_failure.stderr)
        .expect("composition admit fingerprint failure stderr should be UTF-8");
    assert!(
        fingerprint_stderr.contains("field \"source_fingerprint\" must be"),
        "unexpected stderr: {fingerprint_stderr}"
    );
}

#[test]
fn component_composition_artifact_binding_evidence_fails_closed() {
    let gate = GateHarness::new();
    let artifact =
        "target/strata/component_composition_binding_evidence.component-composition.json";
    gate.composition_build("examples/component_composition_main.str", artifact);
    let original = gate.read_text_artifact(artifact);

    let stripped = strip_binding_evidence(&original);
    gate.write_text_artifact(
        "target/strata/component_composition_main.stripped.component-composition.json",
        &stripped,
    );
    let stripped_failure = gate.composition_admit_failure(
        "target/strata/component_composition_main.stripped.component-composition.json",
    );
    let stripped_stderr = String::from_utf8(stripped_failure.stderr)
        .expect("composition admit failure stderr should be UTF-8");
    assert!(
        stripped_stderr.contains("omits binding or unsatisfied-import evidence"),
        "unexpected stderr: {stripped_stderr}"
    );

    let duplicated = duplicate_binding_evidence(&original);
    gate.write_text_artifact(
        "target/strata/component_composition_main.duplicate-binding.component-composition.json",
        &duplicated,
    );
    let duplicate_failure = gate.composition_admit_failure(
        "target/strata/component_composition_main.duplicate-binding.component-composition.json",
    );
    let duplicate_stderr = String::from_utf8(duplicate_failure.stderr)
        .expect("composition admit failure stderr should be UTF-8");
    assert!(
        duplicate_stderr.contains("more than once"),
        "unexpected stderr: {duplicate_stderr}"
    );
}

#[test]
fn component_composition_rejected_artifact_exits_nonzero() {
    let gate = GateHarness::new();
    let artifact = "target/strata/component_composition_rejected_gate.component-composition.json";
    gate.composition_build("examples/component_composition_main.str", artifact);
    let rejected = gate
        .read_text_artifact(artifact)
        .replace(
            "\"binding_result\":\"admitted\",\"rejection_reason\":\"\"",
            "\"binding_result\":\"rejected\",\"rejection_reason\":\"forged rejection\"",
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );
    gate.write_text_artifact(
        "target/strata/component_composition_main.rejected.component-composition.json",
        &rejected,
    );

    let failure = gate.composition_admit_failure(
        "target/strata/component_composition_main.rejected.component-composition.json",
    );
    let stderr = String::from_utf8(failure.stderr)
        .expect("composition admit rejected stderr should be UTF-8");

    assert!(
        stderr.contains("component composition artifact admission rejected"),
        "unexpected stderr: {stderr}"
    );
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

fn strip_binding_evidence(artifact: &str) -> String {
    let without_bindings = replace_array_body(
        artifact,
        "\"port_bindings\":[",
        "],\"runtime_feature_bindings\"",
        "",
    );
    replace_array_body(
        &without_bindings,
        "\"cross_component_authority_edges\":[",
        "],\"unsatisfied_imports\"",
        "",
    )
}

fn duplicate_binding_evidence(artifact: &str) -> String {
    let binding = array_body(
        artifact,
        "\"port_bindings\":[",
        "],\"runtime_feature_bindings\"",
    );
    let edge = array_body(
        artifact,
        "\"cross_component_authority_edges\":[",
        "],\"unsatisfied_imports\"",
    );
    let duplicated_binding = binding.replace("\"port_binding_id\":0", "\"port_binding_id\":1");
    let duplicated_edge = edge.replace("\"port_binding_id\":0", "\"port_binding_id\":1");
    let with_duplicate_binding = replace_array_body(
        artifact,
        "\"port_bindings\":[",
        "],\"runtime_feature_bindings\"",
        &format!("{binding},{duplicated_binding}"),
    );
    replace_array_body(
        &with_duplicate_binding,
        "\"cross_component_authority_edges\":[",
        "],\"unsatisfied_imports\"",
        &format!("{edge},{duplicated_edge}"),
    )
}

fn replace_array_body(
    artifact: &str,
    start_marker: &str,
    end_marker: &str,
    replacement: &str,
) -> String {
    let start = artifact
        .find(start_marker)
        .expect("artifact should contain start marker")
        + start_marker.len();
    let end = artifact[start..]
        .find(end_marker)
        .expect("artifact should contain end marker")
        + start;
    let mut forged = String::with_capacity(artifact.len() - (end - start) + replacement.len());
    forged.push_str(&artifact[..start]);
    forged.push_str(replacement);
    forged.push_str(&artifact[end..]);
    forged
}

fn replace_string_field(artifact: &str, field: &str, replacement: &str) -> String {
    let marker = format!("\"{field}\":\"");
    let start = artifact
        .find(&marker)
        .expect("artifact should contain string field")
        + marker.len();
    let end = artifact[start..]
        .find('"')
        .expect("artifact string field should terminate")
        + start;
    let mut forged = String::with_capacity(artifact.len() - (end - start) + replacement.len());
    forged.push_str(&artifact[..start]);
    forged.push_str(replacement);
    forged.push_str(&artifact[end..]);
    forged
}

fn array_body<'a>(artifact: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
    let start = artifact
        .find(start_marker)
        .expect("artifact should contain start marker")
        + start_marker.len();
    let end = artifact[start..]
        .find(end_marker)
        .expect("artifact should contain end marker")
        + start;
    &artifact[start..end]
}
