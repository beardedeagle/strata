use super::support::*;
use crate::event::{RuntimeAuthorityResult, RuntimeEvent, RuntimeProcessId, RuntimeSpawnKind};
use crate::{LocalSpawnBackend, SpawnAuthorityPolicy};
use mantle_artifact::{ArtifactAction, ArtifactEffect, EffectOutcomeId};

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const UNIT: TypeId = TypeId::new(10);
const SPAWN_ERROR: TypeId = TypeId::new(11);
const SPAWN_RESULT: TypeId = TypeId::new(12);

#[test]
fn runtime_spawn_outcome_returns_denied_before_acceptance() {
    let artifact = spawn_outcome_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(
        &program,
        &executable,
        &mut host,
        deny_spawn_authority_limits(),
    );
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
        .expect("denied spawn outcome should bind a typed failure");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), "Err(Denied(Unit))");
    assert_eq!(run.processes.len(), 1);
    assert_spawn_authority_event(
        host.events(),
        SPAWN_SITE,
        SPAWN_AUTHORITY,
        RuntimeAuthorityResult::Denied,
    );
}

#[test]
fn runtime_bare_spawn_denial_fails_closed_before_acceptance() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    grant_main_spawn_authority(&mut artifact);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::Spawn {
        target: WORKER_PROCESS,
        process_ref: ProcessRefId::new(0),
        spawn_site: SPAWN_SITE,
    }];
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(
        &program,
        &executable,
        &mut host,
        deny_spawn_authority_limits(),
    );
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let step = main_step(main_pid);
    let action = LoadedAction::Spawn {
        target: WORKER_PROCESS,
        process_ref: ProcessRefId::new(0),
        spawn_site: SPAWN_SITE,
    };
    let mut process_refs = LocalProcessRefs::empty();
    let mut effect_outcomes = Vec::new();

    let err = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect_err("bare spawn denial must fail closed");

    assert!(
        err.to_string()
            .contains("process Main spawn authority denied for process id 1"),
        "expected denied authority error, got {err}"
    );
    assert!(process_refs.get(ProcessRefId::new(0)).is_none());
    assert!(effect_outcomes.is_empty());
    assert_eq!(run.processes.len(), 1);
    assert_spawn_authority_event(
        host.events(),
        SPAWN_SITE,
        SPAWN_AUTHORITY,
        RuntimeAuthorityResult::Denied,
    );
}

#[test]
fn runtime_bare_spawn_backend_unavailable_fails_closed_before_acceptance() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    grant_main_spawn_authority(&mut artifact);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::Spawn {
        target: WORKER_PROCESS,
        process_ref: ProcessRefId::new(0),
        spawn_site: SPAWN_SITE,
    }];
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(
        &program,
        &executable,
        &mut host,
        RunLimits {
            local_spawn_backend: LocalSpawnBackend::Unavailable,
            ..RunLimits::default()
        },
    );
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let step = main_step(main_pid);
    let action = LoadedAction::Spawn {
        target: WORKER_PROCESS,
        process_ref: ProcessRefId::new(0),
        spawn_site: SPAWN_SITE,
    };
    let mut process_refs = LocalProcessRefs::empty();
    let mut effect_outcomes = Vec::new();

    let err = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect_err("bare spawn backend unavailability must fail closed");

    assert!(
        err.to_string()
            .contains("process Main local spawn backend unavailable for process id 1"),
        "expected backend-unavailable error, got {err}"
    );
    assert!(process_refs.get(ProcessRefId::new(0)).is_none());
    assert!(effect_outcomes.is_empty());
    assert_eq!(run.processes.len(), 1);
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

fn deny_spawn_authority_limits() -> RunLimits {
    RunLimits {
        spawn_authority_policy: SpawnAuthorityPolicy::DenyDeclared,
        ..RunLimits::default()
    }
}

fn assert_spawn_authority_event(
    events: &[RuntimeEvent],
    expected_spawn_site: SpawnSiteId,
    expected_authority: AuthorityId,
    expected_result: RuntimeAuthorityResult,
) {
    let event = events
        .iter()
        .find_map(|event| match event {
            RuntimeEvent::SpawnAuthorityChecked {
                target_process_id,
                spawn_site_id,
                authority_id,
                spawn_kind,
                authority_result,
                ..
            } => Some((
                target_process_id,
                spawn_site_id,
                authority_id,
                spawn_kind,
                authority_result,
            )),
            _ => None,
        })
        .expect("spawn authority check event should be recorded");

    assert_eq!(*event.0, WORKER_PROCESS);
    assert_eq!(*event.1, expected_spawn_site);
    assert_eq!(*event.2, expected_authority);
    assert_eq!(*event.3, RuntimeSpawnKind::DynamicLocal);
    assert_eq!(*event.4, expected_result);
}
