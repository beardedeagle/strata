pub(crate) use std::fs;
pub(crate) use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(any(unix, windows))]
pub(crate) use mantle_artifact::write_artifact;
pub(crate) use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactAuthority,
    ArtifactCapabilityDescriptor, ArtifactComponent, ArtifactEffect, ArtifactEnumVariant,
    ArtifactLoopElement, ArtifactMessageVariant, ArtifactPayload, ArtifactPort, ArtifactProcess,
    ArtifactProcessRef, ArtifactProtocol, ArtifactSendTarget, ArtifactSpawnKind, ArtifactSpawnSite,
    ArtifactStateValue, ArtifactTransition, ArtifactType, ArtifactTypeField, ArtifactValue,
    ArtifactValueTemplate, AuthorityId, ComponentId, EnumVariantId, LoopElementId, MantleArtifact,
    MessageId, NextState, OutputId, PortId, ProcessId, ProcessRefId, ProtocolId, SpawnSiteId,
    StateId, StepResult, TypeId,
};

pub(crate) use super::super::program::{LoadedProgram, RuntimePayload};
pub(crate) use super::super::*;

pub(crate) const TEST_SOURCE_LANGUAGE: &str = "test_frontend";
pub(crate) const MAIN_STATE: TypeId = TypeId::new(0);
pub(crate) const MAIN_MSG: TypeId = TypeId::new(1);
pub(crate) const WORKER_STATE: TypeId = TypeId::new(2);
pub(crate) const WORKER_MSG: TypeId = TypeId::new(3);
pub(crate) const JOB: TypeId = TypeId::new(4);
pub(crate) const PEER_STATE: TypeId = TypeId::new(5);
pub(crate) const PEER_MSG: TypeId = TypeId::new(6);
pub(crate) const JOB_LIST: TypeId = TypeId::new(7);
pub(crate) const SPAWN_AUTHORITY: AuthorityId = AuthorityId::new(0);
pub(crate) const SPAWN_SITE: SpawnSiteId = SpawnSiteId::new(0);

pub(crate) fn valid_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "actor_ping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: base_types(),
        outputs: vec!["worker handled Ping".to_string()],
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: MAIN_STATE,
                state_values: state_values(MAIN_STATE, &["MainState"]),
                message_type: MAIN_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                authorities: spawn_authorities(ProcessId::new(1)),
                spawn_sites: spawn_sites(ProcessId::new(1)),
                supervisor_plans: Vec::new(),
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
                            spawn_site: SPAWN_SITE,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: WORKER_STATE,
                state_values: state_values(WORKER_STATE, &["Idle", "Handled"]),
                message_type: WORKER_MSG,
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
                    next_state: NextState::Value(StateId::new(1)),
                    effects: vec![ArtifactEffect::Emit],
                    actions: vec![ArtifactAction::Emit {
                        output: OutputId::new(0),
                    }],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

pub(crate) fn panic_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.module = "actor_panic_no_replay".to_string();
    artifact.outputs = Vec::new();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            port: None,
            message: MessageId::new(0),
            payload: None,
        });
    artifact.processes[1].mailbox_bound = 2;
    artifact.processes[1].transitions[0] = ArtifactTransition {
        current_state: None,
        message: MessageId::new(0),
        payload_guard: None,
        step_result: StepResult::Panic,
        next_state: NextState::Value(StateId::new(1)),
        effects: Vec::new(),
        actions: Vec::new(),
    };
    artifact
}

pub(crate) fn payload_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.module = "actor_payloads".to_string();
    artifact.types[WORKER_STATE.index()] = worker_state_type_with_payloads(&[
        ("Idle", None),
        ("Handled", None),
        ("Working", Some(JOB)),
        ("Done", None),
        ("Routed", None),
        ("Ready", None),
        ("Other", None),
        ("Spoofed", None),
    ]);
    artifact.outputs = vec!["worker assigned job".to_string()];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].state_type = WORKER_STATE;
    artifact.processes[1].state_values = state_values(
        WORKER_STATE,
        &["Working(Job{phase:Done})", "Working(Job{phase:Ready})"],
    );
    artifact.processes[1].message_type = WORKER_MSG;
    replace_process_message_variants(
        &mut artifact,
        1,
        vec![ArtifactMessageVariant::payload("Assign", JOB)],
    );
    artifact.processes[1].transitions[0] = ArtifactTransition {
        current_state: None,
        message: MessageId::new(0),
        payload_guard: None,
        step_result: StepResult::Stop,
        next_state: NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: WORKER_STATE,
            variant: EnumVariantId::new(2),
            payload: Box::new(ArtifactValueTemplate::ReceivedPayload { ty: JOB }),
        }),
        effects: vec![ArtifactEffect::Emit],
        actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
    };
    artifact
}

pub(crate) fn for_each_artifact(items: &str, max_items: usize) -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.module = "runtime_for_each".to_string();
    artifact.outputs = vec!["loop worker handled item".to_string()];
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_SITE,
        },
        ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: JOB,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: JOB_LIST,
                value: artifact_value(items),
            },
            max_items,
            body: vec![ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                port: None,
                message: MessageId::new(0),
                payload: Some(ArtifactValueTemplate::LoopElement {
                    ty: JOB,
                    element: LoopElementId::new(0),
                }),
            }],
        },
    ];
    replace_process_message_variants(
        &mut artifact,
        1,
        vec![ArtifactMessageVariant::payload("Ping", JOB)],
    );
    artifact.processes[1].mailbox_bound = max_items.max(1);
    artifact.processes[1].transitions[0] = ArtifactTransition {
        current_state: None,
        message: MessageId::new(0),
        payload_guard: None,
        step_result: StepResult::Continue,
        next_state: NextState::Current,
        effects: vec![ArtifactEffect::Emit],
        actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
    };
    artifact
}

pub(crate) fn looping_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "looping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: base_types(),
        outputs: Vec::new(),
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: MAIN_STATE,
                state_values: state_values(MAIN_STATE, &["MainState"]),
                message_type: MAIN_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                authorities: spawn_authorities(ProcessId::new(1)),
                spawn_sites: spawn_sites(ProcessId::new(1)),
                supervisor_plans: Vec::new(),
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
                            spawn_site: SPAWN_SITE,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: WORKER_STATE,
                state_values: state_values(WORKER_STATE, &["Idle"]),
                message_type: WORKER_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                authorities: spawn_authorities(ProcessId::new(2)),
                spawn_sites: spawn_sites(ProcessId::new(2)),
                supervisor_plans: Vec::new(),
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "peer".to_string(),
                    target: ProcessId::new(2),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    payload_guard: None,
                    step_result: StepResult::Continue,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(2),
                            process_ref: ProcessRefId::new(0),
                            spawn_site: SPAWN_SITE,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Peer".to_string(),
                state_type: PEER_STATE,
                state_values: state_values(PEER_STATE, &["PeerState"]),
                message_type: PEER_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                authorities: spawn_authorities(ProcessId::new(1)),
                spawn_sites: spawn_sites(ProcessId::new(1)),
                supervisor_plans: Vec::new(),
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
                    step_result: StepResult::Continue,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                            spawn_site: SPAWN_SITE,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

pub(crate) fn sequence_artifact() -> MantleArtifact {
    let mut artifact = MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "actor_sequence".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: types_with_worker_state(&["Waiting", "SawFirst", "Done"]),
        outputs: vec![
            "worker handled First".to_string(),
            "worker handled Second".to_string(),
        ],
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: MAIN_STATE,
                state_values: state_values(MAIN_STATE, &["MainState"]),
                message_type: MAIN_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                authorities: spawn_authorities(ProcessId::new(1)),
                spawn_sites: spawn_sites(ProcessId::new(1)),
                supervisor_plans: Vec::new(),
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
                            spawn_site: SPAWN_SITE,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(0),
                            payload: None,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            port: None,
                            message: MessageId::new(1),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: WORKER_STATE,
                state_values: state_values(WORKER_STATE, &["Waiting", "SawFirst", "Done"]),
                message_type: WORKER_MSG,
                message_variants: vec![
                    ArtifactMessageVariant::unit("First"),
                    ArtifactMessageVariant::unit("Second"),
                ],
                authorities: Vec::new(),
                spawn_sites: Vec::new(),
                supervisor_plans: Vec::new(),
                process_refs: Vec::new(),
                mailbox_bound: 2,
                init_state: StateId::new(0),
                transitions: vec![
                    ArtifactTransition {
                        current_state: None,
                        message: MessageId::new(0),
                        payload_guard: None,
                        step_result: StepResult::Continue,
                        next_state: NextState::Value(StateId::new(1)),
                        effects: vec![ArtifactEffect::Emit],
                        actions: vec![ArtifactAction::Emit {
                            output: OutputId::new(0),
                        }],
                    },
                    ArtifactTransition {
                        current_state: None,
                        message: MessageId::new(1),
                        payload_guard: None,
                        step_result: StepResult::Stop,
                        next_state: NextState::Value(StateId::new(2)),
                        effects: vec![ArtifactEffect::Emit],
                        actions: vec![ArtifactAction::Emit {
                            output: OutputId::new(1),
                        }],
                    },
                ],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    };
    align_process_message_type(&mut artifact, 1);
    artifact
}

pub(crate) fn spawn_authorities(target: ProcessId) -> Vec<ArtifactAuthority> {
    vec![ArtifactAuthority {
        debug_name: "spawn_target".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn { target },
    }]
}

pub(crate) fn spawn_sites(target: ProcessId) -> Vec<ArtifactSpawnSite> {
    vec![ArtifactSpawnSite {
        target,
        authority: Some(SPAWN_AUTHORITY),
        supervisor: None,
        child: None,
        kind: ArtifactSpawnKind::DynamicLocal,
    }]
}

pub(crate) fn base_types() -> Vec<ArtifactType> {
    vec![
        ArtifactType::value("MainState"),
        ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
        worker_state_type(&[
            "Idle", "Handled", "Working", "Done", "Routed", "Ready", "Other", "Spoofed",
        ]),
        ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]),
        job_record_type(),
        ArtifactType::value("PeerState"),
        ArtifactType::enum_value("PeerMsg", vec!["Ping".to_string()]),
        ArtifactType::list("JobList", JOB, 16),
    ]
}

pub(crate) fn types_with_worker_state(variants: &[&str]) -> Vec<ArtifactType> {
    let mut types = base_types();
    types[WORKER_STATE.index()] = worker_state_type(variants);
    types
}

pub(crate) fn worker_state_type(variants: &[&str]) -> ArtifactType {
    ArtifactType::enum_value(
        "WorkerState",
        variants
            .iter()
            .map(|variant| (*variant).to_string())
            .collect(),
    )
}

pub(crate) fn worker_state_type_with_payloads(variants: &[(&str, Option<TypeId>)]) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "WorkerState",
        variants
            .iter()
            .map(|(label, payload_type)| ArtifactEnumVariant {
                label: (*label).to_string(),
                payload_type: *payload_type,
            })
            .collect(),
    )
}

pub(crate) fn job_record_type() -> ArtifactType {
    ArtifactType::record(
        "Job",
        vec![ArtifactTypeField {
            name: "phase".to_string(),
            ty: WORKER_STATE,
        }],
    )
}

pub(crate) fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values.iter().map(|value| state_value(ty, value)).collect()
}

pub(crate) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

pub(crate) fn replace_process_message_variants(
    artifact: &mut MantleArtifact,
    process_index: usize,
    variants: Vec<ArtifactMessageVariant>,
) {
    artifact.processes[process_index].message_variants = variants;
    align_process_message_type(artifact, process_index);
}

pub(crate) fn align_process_message_type(artifact: &mut MantleArtifact, process_index: usize) {
    let message_type = artifact.processes[process_index].message_type;
    let label = artifact.types[message_type.index()].label.clone();
    let variants = artifact.processes[process_index]
        .message_variants
        .iter()
        .map(|variant| ArtifactEnumVariant {
            label: variant.label.clone(),
            payload_type: variant.payload_type,
        })
        .collect();
    artifact.types[message_type.index()] = ArtifactType::enum_value_with_payloads(label, variants);
}

pub(crate) fn state_value(ty: TypeId, value: &str) -> ArtifactStateValue {
    ArtifactStateValue::new(ty, artifact_value(value)).expect("test state value should be valid")
}

pub(crate) fn artifact_payload(ty: TypeId, value: &str) -> ArtifactPayload {
    ArtifactPayload::value(ty, artifact_value(value))
        .expect("test artifact payload should be valid")
}

pub(crate) fn event_index(
    events: &[RuntimeEvent],
    predicate: impl Fn(&RuntimeEvent) -> bool,
) -> usize {
    events
        .iter()
        .position(predicate)
        .expect("expected runtime event should be recorded")
}

pub(crate) fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(unique_artifact_name(name))
}

pub(crate) fn unique_current_dir_artifact_path(name: &str) -> PathBuf {
    PathBuf::from(unique_artifact_name(name))
}

pub(crate) fn unique_artifact_name(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    format!("mantle-{name}-{}-{nanos}.mta", std::process::id())
}
