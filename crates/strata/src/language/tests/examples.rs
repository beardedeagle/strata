use super::support::*;

#[test]
fn parses_and_checks_hello() {
    let checked = check_source(HELLO).expect("hello should check");

    assert_eq!(checked.module().name.as_str(), "hello");
    assert_eq!(checked.entry_process(), checked_process_id(0));
    assert_eq!(checked.entry_message(), checked_message_id(0));
    assert_eq!(checked.outputs(), ["hello from Strata"]);
    assert_eq!(checked.processes().len(), 1);
    let transition = only_transition(&checked.processes()[0]);
    assert_eq!(transition.message(), checked_message_id(0));
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert_eq!(transition.next_state(), CheckedNextState::Current);
    assert_eq!(transition.effects(), &[Effect::Emit]);
    assert_eq!(
        transition.actions(),
        [CheckedAction::Emit {
            output: checked_output_id(0)
        }]
    );

    let artifact = lower_to_artifact(&checked, HELLO).expect("hello should lower");
    assert_eq!(
        artifact.processes[0].transitions[0].effects,
        vec![ArtifactEffect::Emit]
    );
}

#[test]
fn parses_checks_and_lowers_init_match_body() {
    let module = parse_source(INIT_MATCH).expect("init match source should parse");
    let main = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Main")
        .expect("Main should parse");
    let Some(FunctionBody::Match(match_body)) = &main.init.body else {
        panic!("Main init should parse as a match body");
    };
    assert_eq!(match_body.scrutinee.as_str(), "Warm");
    assert_eq!(match_body.arms.len(), 2);

    let checked = check_module(module).expect("init match source should check");
    let main = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Main")
        .expect("Main should be checked");
    assert_eq!(
        checked_state_labels(main),
        ["MainState{readiness:WarmReady}"]
    );
    assert_eq!(main.init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(main).next_state(),
        CheckedNextState::Current
    );

    let artifact = lower_to_artifact(&checked, INIT_MATCH).expect("init match should lower");
    let main_artifact = &artifact.processes[0];
    assert_eq!(main_artifact.init_state, mantle_artifact::StateId::new(0));
    assert_eq!(
        artifact_state_labels(main_artifact),
        ["MainState{readiness:WarmReady}"]
    );
}

#[test]
fn parses_and_checks_actor_ping() {
    let checked = check_source(ACTOR_PING).expect("actor ping should check");

    assert_eq!(checked.module().name.as_str(), "actor_ping");
    assert_eq!(checked.entry_process(), checked_process_id(0));
    assert_eq!(checked.entry_message(), checked_message_id(0));
    assert_eq!(checked.outputs(), ["worker handled Ping"]);
    assert_eq!(checked.processes().len(), 2);

    let main = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Main")
        .expect("Main should be checked");
    let main_transition = only_transition(main);
    assert_eq!(main_transition.message(), checked_message_id(0));
    assert_eq!(
        main_transition.actions(),
        [
            CheckedAction::Spawn {
                target: checked_process_id(1),
                process_ref: checked_process_ref_id(0),
                spawn_site: checked_spawn_site_id(0)
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                message: checked_message_id(0),
                payload: None
            }
        ]
    );

    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(worker.init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(worker).next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
}

#[test]
fn parses_and_lowers_panic_step_result() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Panic(Handled);");

    let checked = check_source(&source).expect("panic step result should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(
        only_transition(worker).step_result(),
        CheckedStepResult::Panic
    );
    assert_eq!(
        only_transition(worker).next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );

    let artifact = lower_to_artifact(&checked, &source).expect("panic should lower");
    assert_eq!(
        artifact.processes[1].transitions[0].step_result,
        StepResult::Panic
    );
}

#[test]
fn parses_and_checks_actor_sequence_step_patterns() {
    let checked = check_source(ACTOR_SEQUENCE).expect("actor sequence should check");

    assert_eq!(checked.module().name.as_str(), "actor_sequence");
    assert_eq!(
        checked.outputs(),
        ["worker handled First", "worker handled Second"]
    );
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(
        checked_state_labels(worker),
        ["Waiting", "SawFirst", "Done"]
    );
    assert_eq!(worker.transitions().len(), 2);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
    assert_eq!(
        worker.transitions()[0].step_result(),
        CheckedStepResult::Continue
    );
    assert_eq!(
        worker.transitions()[0].next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
    assert_eq!(worker.transitions()[1].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[1].step_result(),
        CheckedStepResult::Stop
    );
    assert_eq!(
        worker.transitions()[1].next_state(),
        CheckedNextState::Value(checked_state_id(2))
    );

    let artifact = lower_to_artifact(&checked, ACTOR_SEQUENCE)
        .expect("step patterns should lower to transition records");
    assert_eq!(
        artifact.processes[0].transitions[0].effects,
        vec![ArtifactEffect::Spawn, ArtifactEffect::Send]
    );
    let worker_artifact = &artifact.processes[1];
    assert_eq!(worker_artifact.transitions.len(), 2);
    assert_eq!(
        worker_artifact.transitions[0].effects,
        vec![ArtifactEffect::Emit]
    );
    assert_eq!(
        worker_artifact.transitions[1].effects,
        vec![ArtifactEffect::Emit]
    );
    assert_eq!(
        worker_artifact.transitions[0].message,
        mantle_artifact::MessageId::new(0)
    );
    assert_eq!(
        worker_artifact.transitions[1].message,
        mantle_artifact::MessageId::new(1)
    );
    let encoded = artifact.encode();
    assert!(encoded.contains("process.1.transition.0.message=0"));
    assert!(encoded.contains("process.1.transition.1.message=1"));
    assert!(!encoded.contains("transition.0.message=First"));
}

#[test]
fn parses_and_checks_actor_instances_with_distinct_process_refs() {
    let checked = check_source(ACTOR_INSTANCES).expect("actor instances should check");
    let main = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Main")
        .expect("Main should be checked");

    assert_eq!(main.process_refs().len(), 2);
    assert_eq!(main.process_refs()[0].debug_name().as_str(), "first");
    assert_eq!(main.process_refs()[0].target(), checked_process_id(1));
    assert_eq!(main.process_refs()[1].debug_name().as_str(), "second");
    assert_eq!(main.process_refs()[1].target(), checked_process_id(1));
    assert_eq!(
        only_transition(main).actions(),
        [
            CheckedAction::Spawn {
                target: checked_process_id(1),
                process_ref: checked_process_ref_id(0),
                spawn_site: checked_spawn_site_id(0)
            },
            CheckedAction::Spawn {
                target: checked_process_id(1),
                process_ref: checked_process_ref_id(1),
                spawn_site: checked_spawn_site_id(1)
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                message: checked_message_id(0),
                payload: None
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(1)),
                message: checked_message_id(0),
                payload: None
            }
        ]
    );

    let artifact =
        lower_to_artifact(&checked, ACTOR_INSTANCES).expect("actor instances should lower");
    let encoded = artifact.encode();
    assert!(encoded.contains("process.0.process_ref_count=2"));
    assert!(encoded.contains("process.0.process_ref.0.target_process=1"));
    assert!(encoded.contains("process.0.process_ref.1.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.action.2.target_process_ref=0"));
    assert!(encoded.contains("process.0.transition.0.action.3.target_process_ref=1"));
}
