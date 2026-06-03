#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactCapabilityDescriptor, ArtifactComponent,
    ArtifactComponentInstance, ArtifactComposition, ArtifactMessageVariant, ArtifactPort,
    ArtifactPortBinding, ArtifactProcess, ArtifactProtocol, ArtifactStateValue,
    ArtifactTargetRequirements, ArtifactTransition, ArtifactType, ArtifactValue, ComponentId,
    ComponentInstanceId, MantleArtifact, MessageId, NextState, PortId, ProcessId, ProtocolId,
    RuntimeFeature, StateId, StepResult, TypeId,
};

static ARTIFACT: LazyLock<MantleArtifact> = LazyLock::new(artifact);

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = mantle_runtime::validate_runtime_composition_binding_text(text, &ARTIFACT);
});

fn artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.into(),
        schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        source_language: "strata".into(),
        target_requirements: ArtifactTargetRequirements::new(
            "strata",
            vec![
                RuntimeFeature::BoundedMailbox,
                RuntimeFeature::ComponentCompositionMetadata,
                RuntimeFeature::JsonlTrace,
                RuntimeFeature::LocalExecution,
                RuntimeFeature::TypedBoundaryTables,
            ],
        ),
        module: "component_composition_main".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("State"),
            ArtifactType::enum_value("Msg", vec!["Start".to_string()]),
        ],
        outputs: Vec::new(),
        protocols: vec![ArtifactProtocol {
            debug_name: "WorkerProtocol".to_string(),
            message_type: TypeId::new(1),
            required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
                protocol: ProtocolId::new(0),
            },
        }],
        ports: vec![
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
        ],
        components: vec![
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
        ],
        compositions: vec![ArtifactComposition {
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
        }],
        processes: vec![process("Main"), process("Worker")],
        source_hash_fnv1a64: "fd0a28ca5ed2ba8d".to_string(),
    }
}

fn process(name: &str) -> ArtifactProcess {
    ArtifactProcess {
        debug_name: name.to_string(),
        state_type: TypeId::new(0),
        state_values: vec![
            ArtifactStateValue::new(TypeId::new(0), ArtifactValue::Atom("Ready".to_string()))
                .expect("fuzz fixture state value should be valid"),
        ],
        message_type: TypeId::new(1),
        message_variants: vec![ArtifactMessageVariant::unit("Start")],
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
