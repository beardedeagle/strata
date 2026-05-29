use super::*;
use crate::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAuthority, ArtifactComponent, ArtifactEffect,
    ArtifactMessageVariant, ArtifactPort, ArtifactProcess, ArtifactProcessRef, ArtifactProtocol,
    ArtifactSendTarget, ArtifactSpawnSite, ArtifactStateValue, ArtifactSupervisorChild,
    ArtifactSupervisorChildMode, ArtifactSupervisorPlan, ArtifactSupervisorRestartIntensity,
    ArtifactSupervisorStrategy, ArtifactType, MessageId, NextState, ProcessRefId, SpawnSiteId,
    StateId, StepResult, TypeId,
};

#[test]
fn text_summary_reports_artifact_authority_and_spawn_site_ids() {
    let artifact = artifact();

    let summary =
        render_artifact_authority_summary(&artifact, "summary.mta", AuthoritySummaryFormat::Text)
            .expect("valid artifact authority summary should render");

    assert!(summary.contains("mantle authority summary summary.mta"));
    assert!(
        summary.contains("authority 0 spawn_worker: Cap<Spawn<Worker>> used_by_spawn_sites=[0]")
    );
    assert!(
        summary.contains(
            "authority 1 connect_worker: Cap<PortConnect<WorkerPort>> used_by_port_ids=[0]"
        ),
        "{summary}"
    );
    assert!(summary.contains(
        "spawn_site 0 dynamic_local target_process_id=1 target=Worker authority=0 spawn_worker"
    ));
    assert!(summary.contains("supervisor 0 strategy=one_for_one max_restarts=2 within_ms=1000"));
    assert!(summary.contains(
        "child 0 supervised_worker mode=permanent target_process_id=1 target=Worker spawn_site=1"
    ));
}

#[test]
fn json_summary_reports_artifact_authority_and_spawn_site_ids() {
    let artifact = artifact();

    let summary =
        render_artifact_authority_summary(&artifact, "summary.mta", AuthoritySummaryFormat::Json)
            .expect("valid artifact authority summary should render");

    assert!(summary.contains("\"artifact\":\"summary.mta\""));
    assert!(summary.contains("\"authority_id\":0"));
    assert!(summary.contains(
        "\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Worker\"}"
    ));
    assert!(summary.contains("\"used_by_spawn_site_ids\":[0]"));
    assert!(summary.contains(
        "\"descriptor\":{\"kind\":\"port_connect\",\"port_id\":0,\"port\":\"WorkerPort\"}"
    ));
    assert!(summary.contains("\"used_by_port_ids\":[0]"));
    assert!(summary.contains("\"supervisors\":[{\"supervisor_id\":0"));
    assert!(summary.contains("\"strategy\":\"one_for_one\""));
    assert!(summary.contains("\"max_restarts\":2"));
    assert!(summary.contains("\"within_ms\":1000"));
    assert!(summary.contains("\"child\":\"supervised_worker\""));
    assert!(summary.contains("\"mode\":\"permanent\""));
}

#[test]
fn summary_rejects_invalid_artifact_before_rendering() {
    let mut artifact = artifact();
    artifact.processes[0].spawn_sites[0].target = ProcessId::new(99);

    let err =
        render_artifact_authority_summary(&artifact, "summary.mta", AuthoritySummaryFormat::Text)
            .expect_err("invalid artifact authority summary should fail closed");

    assert!(
        err.to_string()
            .contains("spawn site 0 targets undefined process id 99"),
        "{err}"
    );
}

fn artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: "example_lang".to_string(),
        module: "summary".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
            ArtifactType::enum_value("WorkerState", vec!["Idle".to_string()]),
            ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]),
        ],
        outputs: Vec::new(),
        protocols: vec![ArtifactProtocol {
            debug_name: "WorkerProtocol".to_string(),
            message_type: TypeId::new(3),
            required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
                protocol: ProtocolId::new(0),
            },
        }],
        ports: vec![ArtifactPort {
            debug_name: "WorkerPort".to_string(),
            protocol: ProtocolId::new(0),
            target_process: ProcessId::new(1),
            required_authority: ArtifactCapabilityDescriptor::PortConnect {
                port: PortId::new(0),
            },
        }],
        components: vec![ArtifactComponent {
            debug_name: "WorkerComponent".to_string(),
            export_port: PortId::new(0),
            required_authority: ArtifactCapabilityDescriptor::ComponentExport {
                component: ComponentId::new(0),
            },
        }],
        processes: vec![main_process(), worker_process()],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn main_process() -> ArtifactProcess {
    ArtifactProcess {
        debug_name: "Main".to_string(),
        state_type: TypeId::new(0),
        state_values: vec![
            ArtifactStateValue::new(
                TypeId::new(0),
                crate::ArtifactValue::Atom("MainState".into()),
            )
            .expect("state value should be valid"),
        ],
        message_type: TypeId::new(1),
        message_variants: vec![ArtifactMessageVariant::unit("Start")],
        authorities: vec![
            ArtifactAuthority {
                debug_name: "spawn_worker".to_string(),
                descriptor: ArtifactCapabilityDescriptor::Spawn {
                    target: ProcessId::new(1),
                },
            },
            ArtifactAuthority {
                debug_name: "connect_worker".to_string(),
                descriptor: ArtifactCapabilityDescriptor::PortConnect {
                    port: PortId::new(0),
                },
            },
        ],
        spawn_sites: vec![
            ArtifactSpawnSite {
                target: ProcessId::new(1),
                authority: Some(AuthorityId::new(0)),
                supervisor: None,
                child: None,
                kind: ArtifactSpawnKind::DynamicLocal,
            },
            ArtifactSpawnSite {
                target: ProcessId::new(1),
                authority: None,
                supervisor: Some(crate::SupervisorId::new(0)),
                child: Some(crate::SupervisorChildId::new(0)),
                kind: ArtifactSpawnKind::LexicalSupervisorChild,
            },
        ],
        supervisor_plans: vec![ArtifactSupervisorPlan {
            strategy: ArtifactSupervisorStrategy::OneForOne,
            intensity: ArtifactSupervisorRestartIntensity {
                max_restarts: 2,
                within_ms: 1000,
            },
            children: vec![ArtifactSupervisorChild {
                debug_name: "supervised_worker".to_string(),
                target: ProcessId::new(1),
                mode: ArtifactSupervisorChildMode::Permanent,
                spawn_site: SpawnSiteId::new(1),
            }],
        }],
        process_refs: vec![ArtifactProcessRef {
            debug_name: "worker".to_string(),
            target: ProcessId::new(1),
        }],
        mailbox_bound: 1,
        init_state: StateId::new(0),
        transitions: vec![ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: None,
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
            actions: vec![
                ArtifactAction::Spawn {
                    target: ProcessId::new(1),
                    process_ref: ProcessRefId::new(0),
                    spawn_site: SpawnSiteId::new(0),
                },
                ArtifactAction::Send {
                    target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                    port: Some(PortId::new(0)),
                    message: MessageId::new(0),
                    payload: None,
                },
            ],
        }],
    }
}

fn worker_process() -> ArtifactProcess {
    ArtifactProcess {
        debug_name: "Worker".to_string(),
        state_type: TypeId::new(2),
        state_values: vec![
            ArtifactStateValue::new(TypeId::new(2), crate::ArtifactValue::Atom("Idle".into()))
                .expect("state value should be valid"),
        ],
        message_type: TypeId::new(3),
        message_variants: vec![ArtifactMessageVariant::unit("Ping")],
        authorities: Vec::new(),
        spawn_sites: Vec::new(),
        supervisor_plans: Vec::new(),
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: StateId::new(0),
        transitions: vec![ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: None,
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        }],
    }
}
