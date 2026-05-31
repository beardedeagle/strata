use super::*;

use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactAuthority,
    ArtifactCapabilityDescriptor, ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef,
    ArtifactSpawnKind, ArtifactSpawnSite, ArtifactStateValue, ArtifactTargetRequirements,
    ArtifactTransition, ArtifactType, ArtifactValue, AuthorityId, MantleArtifact, MessageId,
    NextState, ProcessId, ProcessRefId, RuntimeFeature, SpawnSiteId, StateId, StepResult, TypeId,
};

use crate::program::LoadedProgram;

const TEST_SOURCE_LANGUAGE: &str = "test_frontend";
const MAIN_STATE: TypeId = TypeId::new(0);
const MAIN_MSG: TypeId = TypeId::new(1);
const WORKER_STATE: TypeId = TypeId::new(2);
const WORKER_MSG: TypeId = TypeId::new(3);
const SPAWN_AUTHORITY: AuthorityId = AuthorityId::new(0);
const SPAWN_SITE: SpawnSiteId = SpawnSiteId::new(0);

#[test]
fn executable_plan_rejects_invalid_loaded_references() {
    let mut program = loaded_program(&[MessageId::new(0), MessageId::new(1)]);
    program.processes[0].transitions[0].message = MessageId::new(9);

    let err = ExecutableProgram::from_admitted(&program)
        .expect_err("plan construction must fail through loaded admission");

    assert!(
        err.to_string().contains("no transition for message id 0"),
        "unexpected plan-construction error: {err}"
    );
}

#[test]
fn executable_plan_orders_dispatch_deterministically() {
    let program_a = loaded_program(&[MessageId::new(1), MessageId::new(0)]);
    let program_b = loaded_program(&[MessageId::new(0), MessageId::new(1)]);

    let plan_a = ExecutableProgram::from_admitted(&program_a).expect("first plan should construct");
    let plan_b =
        ExecutableProgram::from_admitted(&program_b).expect("second plan should construct");

    assert_eq!(plan_a.transition_signature(), plan_b.transition_signature());
}

#[test]
fn executable_dispatch_uses_typed_message_ids_not_labels() {
    let mut artifact = artifact_with_transition_order(&[MessageId::new(0), MessageId::new(1)]);
    artifact.processes[0].message_variants[0].label = "Pong".to_string();
    artifact.processes[0].message_variants[1].label = "Ping".to_string();
    artifact.types[MAIN_MSG.index()] =
        ArtifactType::enum_value("MainMsg", vec!["Pong".to_string(), "Ping".to_string()]);
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let plan = ExecutableProgram::from_admitted(&program).expect("plan should construct");

    let executable_transition = plan
        .transition_for_dispatch(ProcessId::new(0), MessageId::new(0), StateId::new(0), None)
        .expect("typed message id should dispatch");

    assert_eq!(executable_transition.message, MessageId::new(0));
}

#[test]
fn executable_plan_precomputes_prestate_action_prefix() {
    let artifact = spawn_prefix_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let plan = ExecutableProgram::from_admitted(&program).expect("plan should construct");

    let transition = &plan
        .process(ProcessId::new(0))
        .expect("main process planned")
        .transitions[0];

    assert_eq!(transition.actions.prestate_actions(plan.actions()).len(), 1);
}

fn loaded_program(transition_order: &[MessageId]) -> LoadedProgram {
    let artifact = artifact_with_transition_order(transition_order);
    LoadedProgram::from_artifact(&artifact).expect("artifact should load")
}

fn artifact_with_transition_order(transition_order: &[MessageId]) -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.into(),
        schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        source_language: TEST_SOURCE_LANGUAGE.into(),
        target_requirements: target_requirements(vec![]),
        module: "executable_plan".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::enum_value("MainMsg", vec!["Ping".to_string(), "Pong".to_string()]),
        ],
        outputs: Vec::new(),
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![ArtifactProcess {
            debug_name: "Main".to_string(),
            state_type: MAIN_STATE,
            state_values: state_values(MAIN_STATE, &["Idle", "Done"]),
            message_type: MAIN_MSG,
            message_variants: vec![
                ArtifactMessageVariant::unit("Ping"),
                ArtifactMessageVariant::unit("Pong"),
            ],
            authorities: Vec::new(),
            spawn_sites: Vec::new(),
            supervisor_plans: Vec::new(),
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: StateId::new(0),
            transitions: transition_order
                .iter()
                .map(|message| ArtifactTransition {
                    current_state: None,
                    message: *message,
                    payload_guard: None,
                    step_result: StepResult::Stop,
                    next_state: NextState::Value(StateId::new(1)),
                    effects: Vec::new(),
                    actions: Vec::new(),
                })
                .collect(),
        }],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn spawn_prefix_artifact() -> MantleArtifact {
    let mut artifact = artifact_with_transition_order(&[MessageId::new(0), MessageId::new(1)]);
    artifact.module = "executable_spawn_prefix".to_string();
    artifact.target_requirements =
        target_requirements(vec![RuntimeFeature::LocalSpawn, RuntimeFeature::LocalSend]);
    artifact.types.extend([
        ArtifactType::value("WorkerState"),
        ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]),
        ArtifactType::process_ref("ProcessRef_Worker", ProcessId::new(1)),
    ]);
    artifact.processes[0].authorities = vec![ArtifactAuthority {
        debug_name: "spawn_worker".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn {
            target: ProcessId::new(1),
        },
    }];
    artifact.processes[0].spawn_sites = vec![ArtifactSpawnSite {
        target: ProcessId::new(1),
        authority: Some(SPAWN_AUTHORITY),
        supervisor: None,
        child: None,
        kind: ArtifactSpawnKind::DynamicLocal,
    }];
    artifact.processes[0].process_refs = vec![ArtifactProcessRef {
        debug_name: "worker".to_string(),
        target: ProcessId::new(1),
    }];
    artifact.processes[0].transitions[0].effects = vec![mantle_artifact::ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::Spawn {
        target: ProcessId::new(1),
        process_ref: ProcessRefId::new(0),
        spawn_site: SPAWN_SITE,
    }];
    artifact.processes.push(ArtifactProcess {
        debug_name: "Worker".to_string(),
        state_type: WORKER_STATE,
        state_values: state_values(WORKER_STATE, &["WorkerIdle"]),
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
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        }],
    });
    artifact
}

fn target_requirements(extra: Vec<RuntimeFeature>) -> ArtifactTargetRequirements {
    let mut features = vec![
        RuntimeFeature::BoundedMailbox,
        RuntimeFeature::JsonlTrace,
        RuntimeFeature::LocalExecution,
    ];
    features.extend(extra);
    ArtifactTargetRequirements::new(TEST_SOURCE_LANGUAGE, features)
}

fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values
        .iter()
        .map(|value| {
            ArtifactStateValue::new(ty, artifact_value(value))
                .expect("test state value should be valid")
        })
        .collect()
}

fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}
