use mantle_artifact::{
    ArtifactCapabilityDescriptor, ArtifactComponent, ArtifactComponentInstance,
    ArtifactComposition, ArtifactPort, ArtifactPortBinding, ArtifactProtocol, ComponentId,
    ComponentInstanceId, PortId, ProcessId, ProtocolId,
};

use super::support::{MantleArtifact, valid_artifact};
use crate::composition_binding::RuntimeCompositionBinding;
use crate::event::{RuntimeEvent, RuntimeProcessId};

#[test]
fn admits_matching_runtime_composition_binding_and_maps_trace_context() {
    let artifact = composition_artifact();
    let binding = RuntimeCompositionBinding::decode_for_test(&binding_json(), &artifact)
        .expect("matching runtime composition binding should admit");
    let context = binding
        .trace_context_for_event(&RuntimeEvent::ProcessSpawned {
            pid: RuntimeProcessId::from_u64(1).expect("runtime pid should parse"),
            process_id: ProcessId::new(0),
            process: "Main".to_string(),
            state_id: mantle_artifact::StateId::new(0),
            state: "MainState".to_string(),
            mailbox_bound: 1,
            spawned_by_pid: None,
        })
        .expect("mapped process event should produce composition context");

    assert_eq!(context.deployment_id, 0);
    assert_eq!(context.composition_id, 0);
    assert_eq!(
        context.component_instance_id,
        Some(ComponentInstanceId::new(0))
    );
}

#[test]
fn rejects_mismatched_runtime_artifact_identity() {
    let mut artifact = composition_artifact();
    artifact.source_hash_fnv1a64 = "1111111111111111".to_string();
    let err = RuntimeCompositionBinding::decode_for_test(&binding_json(), &artifact)
        .expect_err("mismatched source hash must fail closed");

    assert!(
        err.to_string()
            .contains("field \"source_fingerprint\" must be"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_binding_component_instance_that_does_not_match_runtime_artifact() {
    let artifact = composition_artifact();
    let forged = binding_json().replace(
        "{\"component_instance_id\":1,\"component_id\":1,\"process_id\":1}",
        "{\"component_instance_id\":1,\"component_id\":0,\"process_id\":1}",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged component id must fail closed");

    assert!(
        err.to_string().contains("component_id does not match"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_duplicate_component_instance_process_correlation() {
    let mut artifact = composition_artifact();
    artifact.ports[1].target_process = ProcessId::new(0);
    let forged = binding_json().replace(
        "{\"component_instance_id\":1,\"component_id\":1,\"process_id\":1}",
        "{\"component_instance_id\":1,\"component_id\":1,\"process_id\":0}",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("duplicate process correlation must fail closed");

    assert!(
        err.to_string().contains("process_id 0 is duplicated"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_sparse_runtime_process_correlation() {
    let mut artifact = composition_artifact();
    artifact.processes.push(artifact.processes[0].clone());
    let err = RuntimeCompositionBinding::decode_for_test(&binding_json(), &artifact)
        .expect_err("sparse process correlation must fail closed");

    assert!(
        err.to_string().contains("process_id 2 is unbound"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_noncanonical_binding_schema_identity() {
    let artifact = composition_artifact();
    let forged = binding_json().replace(
        "\"schema_id\":\"mantle.runtime_composition_binding\"",
        "\"schema_id\":\"checked.test_component_composition\"",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("noncanonical runtime binding schema must fail closed");

    assert!(
        err.to_string().contains("field \"schema_id\" must be"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_forged_deployment_id() {
    let artifact = composition_artifact();
    let forged = binding_json().replace("\"deployment_id\":0", "\"deployment_id\":7");
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged deployment id must fail closed");

    assert!(
        err.to_string()
            .contains("field \"deployment_id\" must be 0"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_noncanonical_source_fingerprint_algorithm() {
    let artifact = composition_artifact();
    let forged = binding_json().replace(
        "\"source_fingerprint_algorithm\":\"fnv1a64-diagnostic\"",
        "\"source_fingerprint_algorithm\":\"sha256\"",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged fingerprint algorithm must fail closed");

    assert!(
        err.to_string()
            .contains("field \"source_fingerprint_algorithm\" must be"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_empty_checked_composition_schema_identity() {
    let artifact = composition_artifact();
    let forged = binding_json().replace(
        "\"composition_schema_id\":\"test_frontend.checked_component_composition\"",
        "\"composition_schema_id\":\"\"",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("empty checked composition schema must fail closed");

    assert!(
        err.to_string()
            .contains("field \"composition_schema_id\" has invalid metadata length"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_forged_checked_composition_schema_identity() {
    let artifact = composition_artifact();
    let forged = binding_json().replace(
        "\"composition_schema_id\":\"test_frontend.checked_component_composition\"",
        "\"composition_schema_id\":\"forged.schema\"",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged checked composition schema must fail closed");

    assert!(
        err.to_string()
            .contains("field \"composition_schema_id\" must match"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_unsupported_checked_composition_schema_version() {
    let artifact = composition_artifact();
    let forged = binding_json().replace(
        "\"composition_schema_version_major\":1",
        "\"composition_schema_version_major\":2",
    );
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged checked composition schema version must fail closed");

    assert!(
        err.to_string()
            .contains("field \"composition_schema_version_major\" must be 1"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_duplicate_json_field() {
    assert_binding_rejects(
        binding_json().replace(
            "\"schema_version_major\":1",
            "\"schema_id\":\"mantle.runtime_composition_binding\",\"schema_version_major\":1",
        ),
        "field \"schema_id\" is duplicated",
    );
}

#[test]
fn rejects_unknown_json_field() {
    assert_binding_rejects(
        binding_json().replace("\"extensions\":{}}", "\"extensions\":{},\"unexpected\":0}"),
        "unknown field \"unexpected\"",
    );
}

#[test]
fn rejects_escaped_json_metadata_string() {
    assert_binding_rejects(
        binding_json().replace(
            "\"source_module\":\"component_composition_main\"",
            "\"source_module\":\"component\\u005fcomposition_main\"",
        ),
        "object field value is malformed",
    );
}

#[test]
fn rejects_leading_zero_json_number() {
    assert_binding_rejects(
        binding_json().replace("\"deployment_id\":0", "\"deployment_id\":00"),
        "field \"deployment_id\" must be a JSON unsigned integer",
    );
}

#[test]
fn rejects_trailing_json_object_separator() {
    assert_binding_rejects(
        binding_json().replace("\"extensions\":{}}", "\"extensions\":{},}"),
        "object has a trailing separator",
    );
}

#[test]
fn rejects_non_object_component_instance_item() {
    assert_binding_rejects(
        binding_json().replace(
            "{\"component_instance_id\":0,\"component_id\":0,\"process_id\":0}",
            "0",
        ),
        "component_instances must be a JSON object",
    );
}

fn composition_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.source_language = "test_frontend".into();
    artifact.module = "component_composition_main".to_string();
    artifact.source_hash_fnv1a64 = "fd0a28ca5ed2ba8d".to_string();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: super::support::WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![
        ArtifactPort {
            debug_name: "MainPort".to_string(),
            protocol: ProtocolId::new(0),
            target_process: ProcessId::new(0),
            required_authority: ArtifactCapabilityDescriptor::PortConnect {
                port: PortId::new(0),
            },
        },
        ArtifactPort {
            debug_name: "WorkerPort".to_string(),
            protocol: ProtocolId::new(0),
            target_process: ProcessId::new(1),
            required_authority: ArtifactCapabilityDescriptor::PortConnect {
                port: PortId::new(1),
            },
        },
    ];
    artifact.components = vec![
        ArtifactComponent {
            debug_name: "MainComponent".to_string(),
            export_port: PortId::new(0),
            import_ports: vec![PortId::new(1)],
            required_authority: ArtifactCapabilityDescriptor::ComponentExport {
                component: ComponentId::new(0),
            },
        },
        ArtifactComponent {
            debug_name: "WorkerComponent".to_string(),
            export_port: PortId::new(1),
            import_ports: Vec::new(),
            required_authority: ArtifactCapabilityDescriptor::ComponentExport {
                component: ComponentId::new(1),
            },
        },
    ];
    artifact.compositions = vec![ArtifactComposition {
        debug_name: "AppComposition".to_string(),
        component_instances: vec![
            ArtifactComponentInstance {
                debug_name: "main".to_string(),
                component: ComponentId::new(0),
            },
            ArtifactComponentInstance {
                debug_name: "worker".to_string(),
                component: ComponentId::new(1),
            },
        ],
        port_bindings: vec![ArtifactPortBinding {
            importer: ComponentInstanceId::new(0),
            imported_port: PortId::new(1),
            exporter: ComponentInstanceId::new(1),
            exported_port: PortId::new(1),
        }],
    }];
    artifact
}

fn assert_binding_rejects(forged: String, expected: &str) {
    let artifact = composition_artifact();
    let err = RuntimeCompositionBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged runtime composition binding must fail closed");

    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn binding_json() -> String {
    concat!(
        "{\"schema_id\":\"mantle.runtime_composition_binding\",",
        "\"schema_version_major\":1,\"schema_version_minor\":0,",
        "\"artifact_kind\":\"runtime_composition_binding\",\"deployment_id\":0,",
        "\"source_language\":\"test_frontend\",\"source_module\":\"component_composition_main\",",
        "\"source_fingerprint\":\"fd0a28ca5ed2ba8d\",",
        "\"source_fingerprint_algorithm\":\"fnv1a64-diagnostic\",",
        "\"mantle_artifact_format\":\"mantle-target-artifact\",",
        "\"mantle_artifact_schema_version\":\"6\",",
        "\"mantle_artifact_module\":\"component_composition_main\",",
        "\"mantle_artifact_source_hash_fnv1a64\":\"fd0a28ca5ed2ba8d\",",
        "\"composition_schema_id\":\"test_frontend.checked_component_composition\",",
        "\"composition_schema_version_major\":1,\"composition_schema_version_minor\":0,",
        "\"composition_id\":0,",
        "\"component_instances\":[",
        "{\"component_instance_id\":0,\"component_id\":0,\"process_id\":0},",
        "{\"component_instance_id\":1,\"component_id\":1,\"process_id\":1}],",
        "\"port_bindings\":[{\"port_binding_id\":0,\"importer_instance_id\":0,",
        "\"imported_port_id\":1,\"exporter_instance_id\":1,\"exported_port_id\":1}],",
        "\"admission_result\":\"admitted\",\"extensions\":{}}"
    )
    .to_string()
}
