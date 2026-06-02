use super::*;

#[test]
fn runtime_spawn_outcome_returns_backend_unavailable_before_acceptance() {
    let artifact = spawn_outcome_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let limits = RunLimits {
        local_spawn_backend: LocalSpawnBackend::Unavailable,
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
        .expect("backend-unavailable spawn outcome should bind a typed failure");

    assert!(handled);
    assert_eq!(
        effect_outcomes[0].payload.label(),
        "Err(BackendUnavailable(Unit))"
    );
    assert_eq!(run.processes.len(), 1);
    assert!(run.host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::EffectOutcomeBound {
            outcome_id,
            action: RuntimeEffectOutcomeAction::Spawn,
            target_process_id: WORKER_PROCESS,
            spawn_site_id: Some(SPAWN_SITE),
            message_id: None,
            port_id: None,
            outcome_result: RuntimeEffectOutcomeResult::BackendUnavailable,
            ..
        } if *outcome_id == EffectOutcomeId::new(0)
    )));
}
