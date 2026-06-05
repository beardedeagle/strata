use std::path::Path;

use mantle_artifact::{
    ArtifactSupervisorChild, ArtifactSupervisorChildMode, ArtifactSupervisorPlan,
    ArtifactSupervisorRestartIntensity, ArtifactSupervisorStrategy, SupervisorChildId,
    SupervisorId,
};

use super::support::{
    ArtifactAction, ArtifactCapabilityDescriptor, ArtifactComponent, ArtifactEffect, ArtifactPort,
    ArtifactProtocol, ArtifactSendTarget, ArtifactSpawnKind, ArtifactSpawnSite, ComponentId,
    MantleArtifact, MessageId, PortId, ProcessId, ProtocolId, RunLimits, SpawnAuthorityPolicy,
    SpawnSiteId, valid_artifact,
};
use crate::authority_effect_binding::RuntimeAuthorityEffectBinding;

#[test]
fn admits_matching_runtime_authority_effect_binding_policy() {
    let artifact = authority_artifact();
    let binding = RuntimeAuthorityEffectBinding::decode_for_test(&binding_json(), &artifact)
        .expect("matching runtime authority/effect binding should admit");

    assert_eq!(
        binding.spawn_authority_policy(),
        SpawnAuthorityPolicy::DenyDeclared
    );
}

#[test]
fn rejects_hardcoded_foreign_checked_schema_for_nonmatching_frontend() {
    assert_binding_rejects(
        binding_json().replace(
            "\"authority_effect_schema_id\":\"test_frontend.checked_authority_effects\"",
            "\"authority_effect_schema_id\":\"foreign_frontend.checked_authority_effects\"",
        ),
        "must match source language and authority/effect schema",
    );
}

#[test]
fn admits_matching_runtime_authority_effect_binding_admitted_policy() {
    let artifact = authority_artifact();
    let binding = RuntimeAuthorityEffectBinding::decode_for_test(
        &binding_json().replace("deny_declared", "admit_declared"),
        &artifact,
    )
    .expect("matching admitted runtime authority/effect binding should admit");

    assert_eq!(
        binding.spawn_authority_policy(),
        SpawnAuthorityPolicy::AdmitDeclared
    );
}

#[test]
fn admits_matching_runtime_authority_effect_binding_component_surfaces() {
    let artifact = component_authority_artifact();
    let binding =
        RuntimeAuthorityEffectBinding::decode_for_test(&component_binding_json(), &artifact)
            .expect("matching component authority/effect binding should admit");

    assert_eq!(
        binding.spawn_authority_policy(),
        SpawnAuthorityPolicy::DenyDeclared
    );
}

#[test]
fn admits_matching_runtime_authority_effect_binding_lexical_supervisor_child() {
    let artifact = lexical_supervisor_artifact();
    let binding = RuntimeAuthorityEffectBinding::decode_for_test(
        &lexical_supervisor_binding_json(),
        &artifact,
    )
    .expect("matching lexical supervisor-child authority/effect binding should admit");

    assert_eq!(
        binding.spawn_authority_policy(),
        SpawnAuthorityPolicy::AdmitDeclared
    );
}

#[test]
fn rejects_mismatched_authority_descriptor() {
    assert_binding_rejects(
        binding_json().replace(
            "{\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1}}",
            "{\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":0}}",
        ),
        "authority_id 0 descriptor does not match",
    );
}

#[test]
fn rejects_forged_spawn_site_authority() {
    assert_binding_rejects(
        binding_json().replace("\"spawn_site_id\":0,\"kind\":\"dynamic_local\",\"target_process_id\":1,\"authority_id\":0", "\"spawn_site_id\":0,\"kind\":\"dynamic_local\",\"target_process_id\":1,\"authority_id\":null"),
        "spawn_site_id 0 does not match runtime artifact",
    );
}

#[test]
fn rejects_forged_lexical_supervisor_child_target_process() {
    assert_lexical_supervisor_binding_rejects(
        lexical_supervisor_binding_json().replace(
            "\"kind\":\"lexical_supervisor_child\",\"target_process_id\":1",
            "\"kind\":\"lexical_supervisor_child\",\"target_process_id\":0",
        ),
        "spawn_site_id 0 does not match runtime artifact",
    );
}

#[test]
fn rejects_forged_lexical_supervisor_child_supervisor_id() {
    assert_lexical_supervisor_binding_rejects(
        lexical_supervisor_binding_json().replace(
            "\"supervisor_id\":0,\"supervisor_child_id\":0",
            "\"supervisor_id\":1,\"supervisor_child_id\":0",
        ),
        "spawn_site_id 0 does not match runtime artifact",
    );
}

#[test]
fn rejects_forged_lexical_supervisor_child_id() {
    assert_lexical_supervisor_binding_rejects(
        lexical_supervisor_binding_json().replace(
            "\"supervisor_id\":0,\"supervisor_child_id\":0",
            "\"supervisor_id\":0,\"supervisor_child_id\":1",
        ),
        "spawn_site_id 0 does not match runtime artifact",
    );
}

#[test]
fn rejects_forged_transition_effects() {
    assert_binding_rejects(
        binding_json().replacen("\"effect\":\"spawn\"", "\"effect\":\"send\"", 1),
        "transition_id 0 effects do not match runtime artifact",
    );
}

#[test]
fn rejects_forged_component_authority_surface() {
    assert_component_binding_rejects(
        component_binding_json().replacen(
            "\"component_authority\":{\"kind\":\"component_export\",\"component_id\":0}",
            "\"component_authority\":{\"kind\":\"component_export\",\"component_id\":1}",
            1,
        ),
        "component_id 0 authority surface does not match runtime artifact",
    );
}

#[test]
fn rejects_forged_export_port_authority_surface() {
    assert_component_binding_rejects(
        component_binding_json().replacen(
            "\"export_port_authority\":{\"kind\":\"port_connect\",\"port_id\":0}",
            "\"export_port_authority\":{\"kind\":\"port_connect\",\"port_id\":1}",
            1,
        ),
        "component_id 0 export port authority does not match runtime artifact",
    );
}

#[test]
fn rejects_forged_import_port_authority_surface() {
    assert_component_binding_rejects(
        component_binding_json().replacen(
            "{\"port_id\":1,\"port_authority\":{\"kind\":\"port_connect\",\"port_id\":1}}",
            "{\"port_id\":1,\"port_authority\":{\"kind\":\"port_connect\",\"port_id\":0}}",
            1,
        ),
        "component_id 0 import port id 1 authority does not match runtime artifact",
    );
}

#[test]
fn rejects_wrong_mta_identity() {
    let mut artifact = authority_artifact();
    artifact.source_hash_fnv1a64 = "1111111111111111".to_string();
    let err = RuntimeAuthorityEffectBinding::decode_for_test(&binding_json(), &artifact)
        .expect_err("wrong artifact identity must fail closed");

    assert!(
        err.to_string()
            .contains("field \"source_fingerprint\" must be"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_unsupported_policy_value() {
    assert_binding_rejects(
        binding_json().replace("deny_declared", "grant_all"),
        "unsupported spawn_authority_policy",
    );
}

#[test]
fn rejects_nonzero_deployment_id() {
    assert_binding_rejects(
        binding_json().replace("\"deployment_id\":0", "\"deployment_id\":1"),
        "field \"deployment_id\" must be 0, got 1",
    );
}

#[test]
fn rejects_programmatic_deny_policy_with_authority_effect_binding() {
    let artifact = authority_artifact();
    let binding = RuntimeAuthorityEffectBinding::decode_for_test(
        &binding_json().replace("deny_declared", "admit_declared"),
        &artifact,
    )
    .expect("matching admitted binding should decode");
    let limits = RunLimits {
        spawn_authority_policy: SpawnAuthorityPolicy::DenyDeclared,
        ..RunLimits::default()
    };

    let err = crate::run_artifact_with_limits_and_bindings(
        Path::new("target/tests/authority-effect-conflict.mta"),
        &artifact,
        limits,
        None,
        Some(binding),
    )
    .expect_err("direct deny policy must not be relaxed by a binding");

    assert!(
        err.to_string()
            .contains("cannot be combined with an authority/effect binding"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn rejects_label_injection_field() {
    assert_binding_rejects(
        binding_json().replace(
            "{\"kind\":\"spawn\",\"target_process_id\":1}",
            "{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Main\"}",
        ),
        "unknown field \"target_process\"",
    );
}

fn assert_binding_rejects(forged: String, expected: &str) {
    let artifact = authority_artifact();
    let err = RuntimeAuthorityEffectBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged binding should fail closed");

    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn assert_lexical_supervisor_binding_rejects(forged: String, expected: &str) {
    let artifact = lexical_supervisor_artifact();
    let err = RuntimeAuthorityEffectBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged lexical supervisor-child binding should fail closed");

    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn assert_component_binding_rejects(forged: String, expected: &str) {
    let artifact = component_authority_artifact();
    let err = RuntimeAuthorityEffectBinding::decode_for_test(&forged, &artifact)
        .expect_err("forged component binding should fail closed");

    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn authority_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.source_language = "test_frontend".into();
    artifact.module = "actor_ping".to_string();
    artifact.source_hash_fnv1a64 = "0000000000000000".to_string();
    artifact.processes[0].spawn_sites[0].target = ProcessId::new(1);
    artifact
}

fn lexical_supervisor_artifact() -> MantleArtifact {
    let mut artifact = authority_artifact();
    artifact.source_language = "test_frontend".into();
    artifact.module = "local_supervision_restart".to_string();
    artifact.source_hash_fnv1a64 = "2bac33f1a6db8805".to_string();

    let main = &mut artifact.processes[0];
    main.authorities = Vec::new();
    main.process_refs = Vec::new();
    main.spawn_sites = vec![ArtifactSpawnSite {
        target: ProcessId::new(1),
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    }];
    main.supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "worker".to_string(),
            target: ProcessId::new(1),
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SpawnSiteId::new(0),
        }],
    }];
    main.transitions[0].effects = vec![ArtifactEffect::Send];
    main.transitions[0].actions = vec![ArtifactAction::Send {
        target: ArtifactSendTarget::SupervisorChild {
            supervisor: SupervisorId::new(0),
            child: SupervisorChildId::new(0),
            target_process: ProcessId::new(1),
        },
        port: None,
        message: MessageId::new(0),
        payload: None,
    }];
    artifact.processes[1].transitions[0].effects = Vec::new();

    artifact
}

fn component_authority_artifact() -> MantleArtifact {
    let mut artifact = authority_artifact();
    artifact.module = "authority_effect_components".to_string();
    artifact.source_hash_fnv1a64 = "2222222222222222".to_string();
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
    artifact
}

fn binding_json() -> String {
    r#"{"schema_id":"mantle.runtime_authority_effect_binding","schema_version_major":1,"schema_version_minor":0,"artifact_kind":"runtime_authority_effect_binding","deployment_id":0,"source_language":"test_frontend","source_module":"actor_ping","source_fingerprint":"0000000000000000","source_fingerprint_algorithm":"fnv1a64-diagnostic","mantle_artifact_format":"mantle-target-artifact","mantle_artifact_schema_version":"6","mantle_artifact_module":"actor_ping","mantle_artifact_source_hash_fnv1a64":"0000000000000000","authority_effect_schema_id":"test_frontend.checked_authority_effects","authority_effect_schema_version_major":1,"authority_effect_schema_version_minor":0,"processes":[{"process_id":0,"authorities":[{"authority_id":0,"descriptor":{"kind":"spawn","target_process_id":1}}],"spawn_sites":[{"spawn_site_id":0,"kind":"dynamic_local","target_process_id":1,"authority_id":0,"supervisor_id":null,"supervisor_child_id":null}],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[{"effect_id":0,"effect":"spawn"},{"effect_id":1,"effect":"send"}]}]},{"process_id":1,"authorities":[],"spawn_sites":[],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[{"effect_id":0,"effect":"emit"}]}]}],"component_authority_surfaces":[],"policy":{"spawn_authority_policy":"deny_declared"},"admission_result":"admitted","extensions":{}}"#.to_string()
}

fn lexical_supervisor_binding_json() -> String {
    r#"{"schema_id":"mantle.runtime_authority_effect_binding","schema_version_major":1,"schema_version_minor":0,"artifact_kind":"runtime_authority_effect_binding","deployment_id":0,"source_language":"test_frontend","source_module":"local_supervision_restart","source_fingerprint":"2bac33f1a6db8805","source_fingerprint_algorithm":"fnv1a64-diagnostic","mantle_artifact_format":"mantle-target-artifact","mantle_artifact_schema_version":"6","mantle_artifact_module":"local_supervision_restart","mantle_artifact_source_hash_fnv1a64":"2bac33f1a6db8805","authority_effect_schema_id":"test_frontend.checked_authority_effects","authority_effect_schema_version_major":1,"authority_effect_schema_version_minor":0,"processes":[{"process_id":0,"authorities":[],"spawn_sites":[{"spawn_site_id":0,"kind":"lexical_supervisor_child","target_process_id":1,"authority_id":null,"supervisor_id":0,"supervisor_child_id":0}],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[{"effect_id":0,"effect":"send"}]}]},{"process_id":1,"authorities":[],"spawn_sites":[],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[]}]}],"component_authority_surfaces":[],"policy":{"spawn_authority_policy":"admit_declared"},"admission_result":"admitted","extensions":{}}"#.to_string()
}

fn component_binding_json() -> String {
    r#"{"schema_id":"mantle.runtime_authority_effect_binding","schema_version_major":1,"schema_version_minor":0,"artifact_kind":"runtime_authority_effect_binding","deployment_id":0,"source_language":"test_frontend","source_module":"authority_effect_components","source_fingerprint":"2222222222222222","source_fingerprint_algorithm":"fnv1a64-diagnostic","mantle_artifact_format":"mantle-target-artifact","mantle_artifact_schema_version":"6","mantle_artifact_module":"authority_effect_components","mantle_artifact_source_hash_fnv1a64":"2222222222222222","authority_effect_schema_id":"test_frontend.checked_authority_effects","authority_effect_schema_version_major":1,"authority_effect_schema_version_minor":0,"processes":[{"process_id":0,"authorities":[{"authority_id":0,"descriptor":{"kind":"spawn","target_process_id":1}}],"spawn_sites":[{"spawn_site_id":0,"kind":"dynamic_local","target_process_id":1,"authority_id":0,"supervisor_id":null,"supervisor_child_id":null}],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[{"effect_id":0,"effect":"spawn"},{"effect_id":1,"effect":"send"}]}]},{"process_id":1,"authorities":[],"spawn_sites":[],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[{"effect_id":0,"effect":"emit"}]}]}],"component_authority_surfaces":[{"component_id":0,"export_port_id":0,"component_authority":{"kind":"component_export","component_id":0},"export_port_authority":{"kind":"port_connect","port_id":0},"import_port_authorities":[{"port_id":1,"port_authority":{"kind":"port_connect","port_id":1}}]},{"component_id":1,"export_port_id":1,"component_authority":{"kind":"component_export","component_id":1},"export_port_authority":{"kind":"port_connect","port_id":1},"import_port_authorities":[]}],"policy":{"spawn_authority_policy":"deny_declared"},"admission_result":"admitted","extensions":{}}"#.to_string()
}
