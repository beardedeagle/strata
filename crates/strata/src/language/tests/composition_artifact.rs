use super::super::{
    COMPONENT_COMPOSITION_HASH_ALG, COMPONENT_COMPOSITION_SCHEMA_ID,
    COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR, COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR,
    ComponentCompositionAdmissionResult, ComponentCompositionArtifactAdmitFormat,
    RUNTIME_COMPOSITION_BINDING_SCHEMA_ID, RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MAJOR,
    RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MINOR, SourceProgram, SourceUnit, SourceUnitId,
    admit_component_composition_artifact, check_source_program, lower_to_artifact,
    render_component_composition_admission_summary, render_component_composition_artifact,
    render_runtime_composition_binding,
};
use mantle_artifact::{MAX_FIELD_VALUE_BYTES, MantleArtifact, ProcessId};

#[test]
fn artifact_emits_required_identity_fields_and_admits() {
    let artifact = example_artifact();
    let summary = admit_component_composition_artifact(&artifact)
        .expect("rendered component composition artifact should admit");

    assert!(artifact.contains(&format!(
        "\"schema_id\":\"{COMPONENT_COMPOSITION_SCHEMA_ID}\""
    )));
    assert!(artifact.contains("\"schema_version_major\":1"));
    assert!(artifact.contains("\"schema_version_minor\":0"));
    assert!(artifact.contains(&format!(
        "\"hash_alg\":\"{COMPONENT_COMPOSITION_HASH_ALG}\""
    )));
    assert!(artifact.contains("\"artifact_kind\":\"checked_component_composition\""));
    assert!(artifact.contains("\"source_language\":\"strata\""));
    assert!(artifact.contains("\"source_module\":\"component_composition_main\""));
    assert!(artifact.contains("\"composition_name\":\"AppComposition\""));
    assert!(artifact.contains("\"components\":[{"));
    assert!(artifact.contains("\"component_instance_id\":0"));
    assert!(artifact.contains("\"import_ports\":["));
    assert!(artifact.contains("\"export_port\":{"));
    assert!(artifact.contains("\"port_binding_id\":0"));
    assert!(artifact.contains("\"capability_bindings\":[]"));
    assert!(artifact.contains("\"interface_bindings\":[]"));
    assert!(artifact.contains("\"runtime_feature_bindings\":[]"));
    assert!(artifact.contains("\"archive_format_bindings\":[]"));
    assert!(artifact.contains("\"crypto_policy_bindings\":[]"));
    assert!(artifact.contains("\"cross_component_authority_edges\":[{"));
    assert!(artifact.contains("\"component_id\":"));
    assert!(artifact.contains("\"port_id\":"));
    assert_eq!(summary.schema_id, COMPONENT_COMPOSITION_SCHEMA_ID);
    assert_eq!(
        summary.schema_version_major,
        COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR
    );
    assert_eq!(
        summary.schema_version_minor,
        COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR
    );
    assert_eq!(summary.hash_alg, COMPONENT_COMPOSITION_HASH_ALG);
    assert_eq!(summary.component_instance_count, 2);
    assert_eq!(summary.port_binding_count, 1);
    assert_eq!(summary.authority_edge_count, 1);
    assert_eq!(summary.unsatisfied_import_count, 0);
    assert_eq!(
        summary.admission_result,
        ComponentCompositionAdmissionResult::Admitted
    );
}

#[test]
fn artifact_admission_summary_renders_text_and_json() {
    let artifact = example_artifact();
    let summary = admit_component_composition_artifact(&artifact)
        .expect("rendered component composition artifact should admit");
    let text = render_component_composition_admission_summary(
        &summary,
        "target/strata/component_composition_main.component-composition.json",
        ComponentCompositionArtifactAdmitFormat::Text,
    );
    let json = render_component_composition_admission_summary(
        &summary,
        "target/strata/component_composition_main.component-composition.json",
        ComponentCompositionArtifactAdmitFormat::Json,
    );

    assert!(text.contains("admission_result: admitted"));
    assert!(text.contains("unsatisfied_imports: 0"));
    assert!(json.contains(&format!(
        "\"schema_id\":\"{COMPONENT_COMPOSITION_SCHEMA_ID}\""
    )));
    assert!(json.contains("\"admission_result\":\"admitted\""));
    assert!(json.contains("\"authority_edge_count\":1"));
}

#[test]
fn artifact_rendering_is_deterministic_for_equivalent_graphs() {
    let first = example_artifact();
    let second = example_artifact();

    assert_eq!(first, second);
    assert_admitted_property_invariants(&first);
}

#[test]
fn runtime_binding_emits_explicit_deployment_identity_and_matches_runtime_artifact() {
    let (composition, artifact) = example_composition_and_runtime_artifact();
    let binding = render_runtime_composition_binding(&composition, &artifact)
        .expect("admitted checked composition should bind to matching runtime artifact");

    assert!(binding.contains(&format!(
        "\"schema_id\":\"{RUNTIME_COMPOSITION_BINDING_SCHEMA_ID}\""
    )));
    assert!(binding.contains(&format!(
        "\"schema_version_major\":{RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MAJOR}"
    )));
    assert!(binding.contains(&format!(
        "\"schema_version_minor\":{RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MINOR}"
    )));
    assert!(binding.contains("\"artifact_kind\":\"runtime_composition_binding\""));
    assert!(binding.contains("\"deployment_id\":0"));
    assert!(binding.contains("\"composition_schema_id\":\"strata.checked_component_composition\""));
    assert!(binding.contains("\"composition_id\":0"));
    assert!(binding.contains("\"component_instances\":[{"));
    assert!(binding.contains("\"component_instance_id\":0"));
    assert!(binding.contains("\"process_id\":"));
    assert!(binding.contains("\"port_bindings\":[{"));
    assert!(binding.contains("\"admission_result\":\"admitted\""));
    assert!(binding.contains("\"extensions\":{}"));
    assert!(
        !binding.contains("\"component\":\""),
        "binding must not carry source component labels as executable references: {binding}"
    );
    assert!(
        !binding.contains("\"port\":\""),
        "binding must not carry source port labels as executable references: {binding}"
    );
}

#[test]
fn runtime_binding_rejects_rejected_composition_evidence() {
    let (composition, artifact) = example_composition_and_runtime_artifact();
    let rejected = composition
        .replace(
            "\"binding_result\":\"admitted\",\"rejection_reason\":\"\"",
            "\"binding_result\":\"rejected\",\"rejection_reason\":\"forged rejection\"",
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );
    let err = render_runtime_composition_binding(&rejected, &artifact)
        .expect_err("rejected composition evidence must not bind a runtime run");

    assert!(
        err.to_string().contains("requires an admitted"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_rejects_mismatched_runtime_artifact_identity() {
    let (composition, mut artifact) = example_composition_and_runtime_artifact();
    artifact.source_hash_fnv1a64 = "1111111111111111".to_string();
    let err = render_runtime_composition_binding(&composition, &artifact)
        .expect_err("mismatched .mta source identity must fail closed");

    assert!(
        err.to_string()
            .contains("source fingerprint does not match"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_rejects_runtime_component_process_out_of_bounds() {
    let (composition, mut artifact) = example_composition_and_runtime_artifact();
    let component_id = artifact.compositions[0].component_instances[0].component;
    let export_port_id = artifact.components[component_id.index()].export_port;
    artifact.ports[export_port_id.index()].target_process =
        ProcessId::from_index(artifact.processes.len()).expect("test process id should fit");

    let err = render_runtime_composition_binding(&composition, &artifact)
        .expect_err("out-of-bounds runtime target process must fail closed");

    assert!(
        err.to_string().contains("target process is out of bounds"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_rejects_duplicate_component_process_correlation() {
    let program = duplicate_worker_instance_program()
        .expect("duplicate worker-instance source should parse as checked composition input");
    let source_hash = program.source_provenance_hash();
    let source_hash_input = program.source_hash_input();
    let checked = check_source_program(program)
        .expect("duplicate worker-instance composition should check before runtime binding");
    let composition = render_component_composition_artifact(
        &checked,
        "examples/component_composition_main.str",
        &source_hash,
        None,
    )
    .expect("duplicate worker-instance composition artifact should render");
    let artifact = lower_to_artifact(&checked, &source_hash_input)
        .expect("duplicate worker-instance artifact should lower");

    let err = render_runtime_composition_binding(&composition, &artifact)
        .expect_err("duplicate process correlation must fail closed");

    assert!(
        err.to_string()
            .contains("duplicate component instances to the same process id"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_rejects_unbound_runtime_processes() {
    let (composition, mut artifact) = example_composition_and_runtime_artifact();
    artifact.processes.push(artifact.processes[0].clone());

    let err = render_runtime_composition_binding(&composition, &artifact)
        .expect_err("unbound runtime process must fail closed");

    assert!(
        err.to_string().contains("process_id 2 is unbound"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_output_is_stable_for_equivalent_checked_graphs() {
    let (first_composition, first_artifact) = example_composition_and_runtime_artifact();
    let (second_composition, second_artifact) = example_composition_and_runtime_artifact();
    let first = render_runtime_composition_binding(&first_composition, &first_artifact)
        .expect("first binding should render");
    let second = render_runtime_composition_binding(&second_composition, &second_artifact)
        .expect("second binding should render");

    assert_eq!(first, second);
}

#[test]
fn admission_rejects_missing_schema_id() {
    assert_rejects(
        &example_artifact().replacen(
            &format!("\"schema_id\":\"{COMPONENT_COMPOSITION_SCHEMA_ID}\","),
            "",
            1,
        ),
        "missing field \"schema_id\"",
    );
}

#[test]
fn admission_rejects_canonical_schema_id_without_canonical_payload() {
    assert_rejects(
        &example_artifact().replace(
            &format!("\"schema_id\":\"{COMPONENT_COMPOSITION_SCHEMA_ID}\""),
            "\"schema_id\":\"strata.component_composition\"",
        ),
        "field \"schema_id\" must be",
    );
}

#[test]
fn admission_rejects_unsupported_schema_version_major() {
    assert_rejects(
        &example_artifact().replace("\"schema_version_major\":1", "\"schema_version_major\":2"),
        "field \"schema_version_major\" must be schema version 1, got 2",
    );
}

#[test]
fn admission_rejects_missing_hash_alg() {
    assert_rejects(
        &example_artifact().replacen(
            &format!("\"hash_alg\":\"{COMPONENT_COMPOSITION_HASH_ALG}\","),
            "",
            1,
        ),
        "missing field \"hash_alg\"",
    );
}

#[test]
fn admission_rejects_noncanonical_source_fingerprint() {
    assert_rejects(
        &example_artifact().replace(
            "\"source_fingerprint\":\"fd0a28ca5ed2ba8d\"",
            "\"source_fingerprint\":\"not-a-fnv-hash\"",
        ),
        "field \"source_fingerprint\" must be a 16-character lowercase hexadecimal",
    );
    assert_rejects(
        &example_artifact().replace(
            "\"source_fingerprint\":\"fd0a28ca5ed2ba8d\"",
            "\"source_fingerprint\":\"FD0A28CA5ED2BA8D\"",
        ),
        "field \"source_fingerprint\" must be a 16-character lowercase hexadecimal",
    );
}

#[test]
fn admission_decodes_json_string_escapes_before_comparison() {
    let escaped = example_artifact()
        .replace(
            &format!("\"schema_id\":\"{COMPONENT_COMPOSITION_SCHEMA_ID}\""),
            "\"schema_id\":\"strata.checked\\u005fcomponent\\u005fcomposition\"",
        )
        .replace(
            &format!("\"hash_alg\":\"{COMPONENT_COMPOSITION_HASH_ALG}\""),
            "\"hash_alg\":\"fnv1a64\\u002ddiagnostic\"",
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"admit\\u0074ed\"",
        )
        .replace(
            "\"component\":\"WorkerComponent\"",
            "\"component\":\"Worker\\u0043omponent\"",
        );

    let summary = admit_component_composition_artifact(&escaped)
        .expect("semantic JSON string escapes should admit");

    assert_eq!(
        summary.admission_result,
        ComponentCompositionAdmissionResult::Admitted
    );
}

#[test]
fn admission_rejects_unpaired_unicode_string_escape() {
    assert_rejects(
        &example_artifact().replacen(
            "\"source_path\":\"examples/component_composition_main.str\"",
            "\"source_path\":\"examples/\\uD800.str\"",
            1,
        ),
        "unpaired high surrogate",
    );
}

#[test]
fn admission_rejects_non_empty_unimplemented_binding_classes() {
    assert_rejects(
        &example_artifact().replace("\"capability_bindings\":[]", "\"capability_bindings\":[{}]"),
        "field \"capability_bindings\" is not implemented",
    );
}

#[test]
fn admission_rejects_duplicate_component_instance_ids() {
    assert_rejects(
        &example_artifact().replacen(
            "\"component_instance_id\":1",
            "\"component_instance_id\":0",
            1,
        ),
        "component_instance_id 0 at array index 1 is not canonical",
    );
}

#[test]
fn admission_rejects_duplicate_binding_ids() {
    assert_rejects(
        &example_artifact().replacen("\"port_binding_id\":0", "\"port_binding_id\":1", 1),
        "port_binding_id 1 at array index 0 is not canonical",
    );
}

#[test]
fn admission_rejects_port_binding_to_unknown_instance() {
    assert_rejects(
        &example_artifact().replacen(
            "\"exporter_instance_id\":1",
            "\"exporter_instance_id\":99",
            1,
        ),
        "references unknown component instance id 99",
    );
}

#[test]
fn admission_rejects_admitted_artifact_with_unsatisfied_imports() {
    let forged = example_artifact().replace(
        "\"unsatisfied_imports\":[]",
        "\"unsatisfied_imports\":[{\"component_instance_id\":0,\"instance\":\"main\",\"imported_port_id\":0,\"imported_port\":\"WorkerPort\",\"reason\":\"missing binding\"}]",
    );

    assert_rejects(&forged, "both bound and unsatisfied");
}

#[test]
fn admission_rejects_rejected_binding_with_global_admitted_result() {
    let forged = example_artifact().replace(
        "\"binding_result\":\"admitted\",\"rejection_reason\":\"\"",
        "\"binding_result\":\"rejected\",\"rejection_reason\":\"forged rejection\"",
    );

    assert_rejects(
        &forged,
        "admitted component composition artifact has rejected port_bindings",
    );
}

#[test]
fn admission_summarizes_rejected_artifact_with_rejection_evidence() {
    let rejected = example_artifact()
        .replace(
            "\"binding_result\":\"admitted\",\"rejection_reason\":\"\"",
            "\"binding_result\":\"rejected\",\"rejection_reason\":\"forged rejection\"",
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );

    let summary = admit_component_composition_artifact(&rejected)
        .expect("rejected artifact with evidence should validate for inspection");

    assert_eq!(
        summary.admission_result,
        ComponentCompositionAdmissionResult::Rejected
    );
    assert_eq!(summary.rejected_binding_count, 1);
    assert_eq!(summary.rejection_reason_count, 1);
}

#[test]
fn admission_rejects_unbounded_binding_rejection_reason() {
    let reason = "x".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let rejected = example_artifact()
        .replace(
            "\"binding_result\":\"admitted\",\"rejection_reason\":\"\"",
            &format!("\"binding_result\":\"rejected\",\"rejection_reason\":\"{reason}\""),
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );

    assert_rejects(&rejected, "metadata field \"rejection_reason\" exceeds");
}

#[test]
fn admission_rejects_unbounded_unsatisfied_import_reason() {
    let reason = "x".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let rejected = example_artifact()
        .replace(
            "\"unsatisfied_imports\":[]",
            &format!(
                "\"unsatisfied_imports\":[{{\"component_instance_id\":0,\"instance\":\"main\",\"imported_port_id\":0,\"imported_port\":\"WorkerPort\",\"reason\":\"{reason}\"}}]"
            ),
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );

    assert_rejects(&rejected, "metadata field \"reason\" exceeds");
}

#[test]
fn admission_summarizes_rejected_artifact_with_unsatisfied_import_evidence() {
    let rejected = strip_binding_evidence(&example_artifact())
        .replace(
            "\"unsatisfied_imports\":[]",
            "\"unsatisfied_imports\":[{\"component_instance_id\":0,\"instance\":\"main\",\"imported_port_id\":0,\"imported_port\":\"WorkerPort\",\"reason\":\"missing binding\"}]",
        )
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );

    let summary = admit_component_composition_artifact(&rejected)
        .expect("rejected artifact should carry unsatisfied import evidence");

    assert_eq!(
        summary.admission_result,
        ComponentCompositionAdmissionResult::Rejected
    );
    assert_eq!(summary.unsatisfied_import_count, 1);
    assert_eq!(summary.rejection_reason_count, 1);
}

#[test]
fn admission_rejects_duplicate_unsatisfied_import_evidence() {
    let repeated_imports = "\"unsatisfied_imports\":[{\"component_instance_id\":0,\"instance\":\"main\",\"imported_port_id\":0,\"imported_port\":\"WorkerPort\",\"reason\":\"missing binding\"},{\"component_instance_id\":0,\"instance\":\"main\",\"imported_port_id\":0,\"imported_port\":\"WorkerPort\",\"reason\":\"still missing\"}]";
    let rejected = strip_binding_evidence(&example_artifact())
        .replace("\"unsatisfied_imports\":[]", repeated_imports)
        .replace(
            "\"admission_result\":\"admitted\"",
            "\"admission_result\":\"rejected\"",
        );

    assert_rejects(&rejected, "more than once");
}

#[test]
fn admission_rejects_admitted_artifact_without_required_binding_evidence() {
    assert_rejects(
        &strip_binding_evidence(&example_artifact()),
        "omits binding or unsatisfied-import evidence",
    );
}

#[test]
fn admission_rejects_duplicate_import_binding_evidence() {
    assert_rejects(
        &duplicate_binding_evidence(&example_artifact()),
        "more than once",
    );
}

#[test]
fn admission_rejects_malformed_authority_edge_references() {
    assert_rejects(
        &example_artifact().replace("\"exporter_component_id\":0", "\"exporter_component_id\":1"),
        "field \"exporter_component_id\" must reference typed id 0, got 1",
    );
}

#[test]
fn admission_rejects_source_name_only_executable_references() {
    assert_rejects(
        &example_artifact().replacen("\"importer_instance_id\":0,", "", 1),
        "missing field \"importer_instance_id\"",
    );
}

#[test]
fn admission_rejects_inconsistent_authority_descriptor_ids() {
    assert_rejects(
        &example_artifact().replace(
            "\"export_authority\":{\"kind\":\"component_export\",\"component_id\":0",
            "\"export_authority\":{\"kind\":\"component_export\",\"component_id\":1",
        ),
        "field \"component_id\" must reference typed id 0, got 1",
    );
}

#[test]
fn admission_treats_source_labels_as_metadata_not_typed_ids() {
    let renamed = example_artifact()
        .replace(
            "\"component\":\"WorkerComponent\"",
            "\"component\":\"RenamedWorker\"",
        )
        .replace("\"port\":\"WorkerPort\"", "\"port\":\"RenamedWorkerPort\"");
    let summary = admit_component_composition_artifact(&renamed)
        .expect("renamed metadata labels must not change typed admission");

    assert_eq!(
        summary.admission_result,
        ComponentCompositionAdmissionResult::Admitted
    );
    assert_eq!(summary.port_binding_count, summary.authority_edge_count);
}

fn assert_admitted_property_invariants(artifact: &str) {
    let summary = admit_component_composition_artifact(artifact)
        .expect("rendered component composition artifact should admit");
    assert_eq!(summary.rejected_binding_count, 0);
    assert_eq!(summary.rejection_reason_count, 0);
    assert_eq!(summary.unsatisfied_import_count, 0);
    assert_eq!(summary.port_binding_count, summary.authority_edge_count);
    assert!(summary.component_instance_count > 0);
}

fn assert_rejects(artifact: &str, expected: &str) {
    let err = admit_component_composition_artifact(artifact)
        .expect_err("forged component composition artifact should fail closed");
    assert!(
        err.to_string().contains(expected),
        "expected diagnostic containing {expected:?}, got {err}"
    );
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

fn example_artifact() -> String {
    let (composition, _) = example_composition_and_runtime_artifact();
    composition
}

fn example_composition_and_runtime_artifact() -> (String, MantleArtifact) {
    let program = example_program().expect("example source program should parse");
    let source_hash = program.source_provenance_hash();
    let source_hash_input = program.source_hash_input();
    let checked = check_source_program(program).expect("example source program should check");
    let composition = render_component_composition_artifact(
        &checked,
        "examples/component_composition_main.str",
        &source_hash,
        None,
    )
    .expect("example composition artifact should render");
    let artifact = lower_to_artifact(&checked, &source_hash_input)
        .expect("example runtime artifact should lower");
    (composition, artifact)
}

fn example_program() -> crate::language::Result<SourceProgram> {
    let units = [
        include_str!("../../../../../examples/component_composition_main.str"),
        include_str!("../../../../../examples/component_composition_worker.str"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, source)| SourceUnit::parse(SourceUnitId::from_index(index)?, source.to_string()))
    .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(0)?, units)
}

fn duplicate_worker_instance_program() -> crate::language::Result<SourceProgram> {
    let root = include_str!("../../../../../examples/component_composition_main.str").replace(
        "    instance worker component WorkerComponent;",
        "    instance worker component WorkerComponent;\n    instance worker2 component WorkerComponent;",
    );
    let units = [
        root,
        include_str!("../../../../../examples/component_composition_worker.str").to_string(),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, source)| SourceUnit::parse(SourceUnitId::from_index(index)?, source))
    .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(0)?, units)
}
