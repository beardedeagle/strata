use super::support::*;
use crate::RuntimeProcessId;
use mantle_artifact::{
    ArtifactAction, ArtifactSupervisorChild, ArtifactSupervisorChildMode, ArtifactSupervisorPlan,
    ArtifactSupervisorRestartIntensity, ArtifactSupervisorStrategy, EffectOutcomeId,
    SupervisorChildId, SupervisorId,
};

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const HELPER_PROCESS: ProcessId = ProcessId::new(2);
const UNIT: TypeId = TypeId::new(10);
const SPAWN_ERROR: TypeId = TypeId::new(11);
const SPAWN_RESULT: TypeId = TypeId::new(12);

#[test]
fn runtime_spawn_outcome_returns_exhausted_before_supervised_subtree_partial_spawn() {
    let mut artifact = spawn_outcome_artifact();
    add_worker_supervised_helper(&mut artifact);
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let limits = RunLimits {
        max_runtime_processes: 2,
        ..RunLimits::default()
    };
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, limits);
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let step = main_step(main_pid);
    let action = LoadedAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SPAWN_RESULT,
        target: WORKER_PROCESS,
        spawn_site: SPAWN_SITE,
    };
    let mut process_refs = LocalProcessRefs::empty();
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("subtree exhaustion should bind a typed failure");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), "Err(Exhausted(Unit))");
    assert_eq!(run.processes.len(), 1);
    assert_eq!(run.spawned_processes.len(), 1);
}

fn spawn_outcome_artifact() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    grant_main_spawn_authority(&mut artifact);
    push_spawn_outcome_types(&mut artifact);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].next_state = NextState::Current;
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SPAWN_RESULT,
        target: WORKER_PROCESS,
        spawn_site: SPAWN_SITE,
    }];
    artifact
}

fn add_worker_supervised_helper(artifact: &mut MantleArtifact) {
    let mut helper = artifact.processes[1].clone();
    helper.debug_name = "Helper".to_string();
    helper.spawn_sites.clear();
    helper.supervisor_plans.clear();
    helper.authorities.clear();
    helper.process_refs.clear();
    artifact.processes[1].spawn_sites = vec![ArtifactSpawnSite {
        target: HELPER_PROCESS,
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    }];
    artifact.processes[1].supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "helper".to_string(),
            target: HELPER_PROCESS,
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SPAWN_SITE,
        }],
    }];
    artifact.processes.push(helper);
}

fn push_spawn_outcome_types(artifact: &mut MantleArtifact) {
    assert_eq!(push_type(artifact, ArtifactType::value("Unit")), UNIT);
    assert_eq!(push_type(artifact, spawn_error_type()), SPAWN_ERROR);
    assert_eq!(
        push_type(artifact, result_type(PROCESS_REF_WORKER, SPAWN_ERROR)),
        SPAWN_RESULT
    );
}

fn push_type(artifact: &mut MantleArtifact, ty: ArtifactType) -> TypeId {
    let id = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ty);
    id
}

fn result_type(ok: TypeId, err: TypeId) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "Result",
        vec![
            ArtifactEnumVariant {
                label: "Ok".to_string(),
                payload_type: Some(ok),
            },
            ArtifactEnumVariant {
                label: "Err".to_string(),
                payload_type: Some(err),
            },
        ],
    )
}

fn spawn_error_type() -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "SpawnError",
        ["Denied", "Exhausted", "BackendUnavailable"]
            .into_iter()
            .map(|label| ArtifactEnumVariant {
                label: label.to_string(),
                payload_type: Some(UNIT),
            })
            .collect(),
    )
}

fn main_step(main_pid: RuntimeProcessId) -> ActiveStep {
    ActiveStep {
        pid: main_pid,
        process_id: MAIN_PROCESS,
        process_name: "Main".to_string(),
        current_state: StateId::new(0),
        message: MessageId::new(0),
        message_label: "Start".to_string(),
        payload: None,
    }
}
