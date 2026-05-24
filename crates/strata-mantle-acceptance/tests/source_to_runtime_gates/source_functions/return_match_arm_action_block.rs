use crate::support::*;

const STEM: &str = "process_return_match_arm_action_block";
const SOURCE: &str = "examples/process_return_match_arm_action_block.str";
const ARTIFACT: &str = "target/strata/process_return_match_arm_action_block.mta";

#[test]
fn process_return_match_arm_action_block_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout
            .matches("return-match action-block uniform prefix")
            .count(),
        2
    );
    assert_eq!(
        stdout
            .matches("return-match ready action block start")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match done action block start")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match ready second if branch")
            .count(),
        1
    );
    assert_eq!(
        stdout.matches("return-match done second if branch").count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match ready branch loop nested ready")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match ready branch loop normal")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match done branch loop nested done")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match done branch loop normal")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match ready second loop item")
            .count(),
        2
    );
    assert_eq!(
        stdout.matches("return-match done second loop item").count(),
        2
    );
    assert_eq!(
        stdout
            .matches("sink received action-block ready notice")
            .count(),
        6
    );
    assert_eq!(
        stdout
            .matches("sink received action-block done notice")
            .count(),
        6
    );

    let artifact = gate.read_artifact(ARTIFACT);
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    for transition in &worker.transitions {
        assert_eq!(
            transition.effects,
            [
                ArtifactEffect::Emit,
                ArtifactEffect::Spawn,
                ArtifactEffect::Send
            ]
        );
        assert!(
            is_selected_arm_action_block_transition(transition),
            "selected return-match arm should lower as ordinary typed action-block sequencing: {transition:?}"
        );
    }
    let encoded = artifact.encode();
    for source_only in [
        "route_phase",
        "route_enabled",
        "route_jobs",
        "item_phase",
        "item_urgent",
    ] {
        assert!(
            !encoded.lines().any(|line| line.contains(source_only)),
            "artifact must not dispatch through source binding name {source_only}"
        );
    }

    let trace = gate.read_trace(STEM);
    let uniform = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match action-block uniform prefix""#,
        ],
    );
    let start = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready action block start""#,
        ],
    );
    let first_if = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
        ],
    );
    let second_if = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready second if branch""#,
        ],
    );
    let first_loop = trace_line_index_with_fields(
        &trace,
        &[r#""event":"loop_started""#, r#""process":"Worker""#],
    );
    let second_loop = trace
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.contains(r#""event":"loop_started""#) && line.contains(r#""process":"Worker""#))
                .then_some(index)
        })
        .nth(1)
        .expect("ready transition should start a top-level arm-local loop after branch loop");
    let third_loop = trace
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.contains(r#""event":"loop_started""#) && line.contains(r#""process":"Worker""#))
                .then_some(index)
        })
        .nth(2)
        .expect("ready transition should start a second top-level arm-local loop");

    assert!(uniform < start);
    assert!(start < first_if);
    assert!(first_if < first_loop);
    assert!(first_loop < second_if);
    assert!(second_if < second_loop);
    assert!(second_loop < third_loop);
}

#[test]
fn process_return_match_arm_action_block_artifact_rejects_bypassed_nested_loop() {
    let gate = GateHarness::new();
    let seed_artifact = "target/strata/process_return_match_arm_action_block_nested_for_seed.mta";
    let invalid_artifact = "target/strata/process_return_match_arm_action_block_nested_for.mta";
    let invalid_trace = "process_return_match_arm_action_block_nested_for";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact);
    gate.remove_artifact(invalid_artifact);
    gate.remove_trace(invalid_trace);

    let mut artifact = gate.read_artifact(seed_artifact);
    insert_nested_for_each_into_worker_action_block(&mut artifact);
    gate.write_unvalidated_encoded_artifact(invalid_artifact, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);

    assert!(
        stderr.contains("nested for loops are not supported"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace));
}

#[test]
fn process_return_match_arm_action_block_artifact_rejects_bypassed_deep_runtime_if() {
    let gate = GateHarness::new();
    let seed_artifact = "target/strata/process_return_match_arm_action_block_deep_if_seed.mta";
    let invalid_artifact = "target/strata/process_return_match_arm_action_block_deep_if.mta";
    let invalid_trace = "process_return_match_arm_action_block_deep_if";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact);
    gate.remove_artifact(invalid_artifact);
    gate.remove_trace(invalid_trace);

    let mut artifact = gate.read_artifact(seed_artifact);
    deepen_worker_action_block_if(&mut artifact);
    gate.write_unvalidated_encoded_artifact(invalid_artifact, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);

    assert!(
        stderr.contains("runtime if action nesting exceeds maximum depth"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace));
}

fn is_selected_arm_action_block_transition(transition: &ArtifactTransition) -> bool {
    matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Spawn { .. },
            ArtifactAction::Emit { .. },
            ArtifactAction::IfElse { then_actions, else_actions, .. },
            ArtifactAction::IfElse { .. },
            ArtifactAction::ForEach { body: first_body, .. },
            ArtifactAction::ForEach { body: second_body, .. },
        ] if (branch_has_nested_if(then_actions) || branch_has_nested_if(else_actions))
            && (branch_has_nested_loop_if(then_actions) || branch_has_nested_loop_if(else_actions))
            && loop_body_has_branch_send(first_body)
            && loop_body_sends_directly(second_body)
    )
}

fn insert_nested_for_each_into_worker_action_block(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    let nested = transition
        .actions
        .iter()
        .find(|action| matches!(action, ArtifactAction::ForEach { .. }))
        .expect("test artifact should contain a selected-arm for_each action")
        .clone();
    let ArtifactAction::ForEach { body, .. } = transition
        .actions
        .iter_mut()
        .find(|action| matches!(action, ArtifactAction::ForEach { .. }))
        .expect("test artifact should contain a mutable selected-arm for_each action")
    else {
        unreachable!("find predicate already matched for_each");
    };
    body.push(nested);
}

fn deepen_worker_action_block_if(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    assert!(
        deepen_first_nested_if(&mut transition.actions),
        "test artifact should contain nested selected-arm if actions"
    );
}

fn first_worker_transition_mut(artifact: &mut MantleArtifact) -> &mut ArtifactTransition {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    worker
        .transitions
        .first_mut()
        .expect("Worker artifact process should have transitions")
}

fn deepen_first_nested_if(actions: &mut [ArtifactAction]) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                if insert_extra_if_inside_first_if(then_actions) {
                    return true;
                }
                if insert_extra_if_inside_first_if(else_actions) {
                    return true;
                }
                if deepen_first_nested_if(then_actions) {
                    return true;
                }
                if deepen_first_nested_if(else_actions) {
                    return true;
                }
            }
            ArtifactAction::ForEach { body, .. } => {
                if deepen_first_nested_if(body) {
                    return true;
                }
            }
            ArtifactAction::Spawn { .. }
            | ArtifactAction::Emit { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}

fn insert_extra_if_inside_first_if(actions: &mut [ArtifactAction]) -> bool {
    let Some(action) = actions
        .iter_mut()
        .find(|action| matches!(action, ArtifactAction::IfElse { .. }))
    else {
        return false;
    };
    let ArtifactAction::IfElse {
        condition,
        then_actions,
        ..
    } = action
    else {
        unreachable!("find predicate already matched if_else");
    };
    let nested_then = std::mem::take(then_actions);
    then_actions.push(ArtifactAction::IfElse {
        condition: condition.clone(),
        then_actions: nested_then,
        else_actions: Vec::new(),
    });
    true
}

fn branch_has_nested_if(actions: &[ArtifactAction]) -> bool {
    actions.iter().any(|action| {
        matches!(
            action,
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } if !then_actions.is_empty() || !else_actions.is_empty()
        )
    })
}

fn branch_has_nested_loop_if(actions: &[ArtifactAction]) -> bool {
    actions.iter().any(|action| {
        matches!(
            action,
            ArtifactAction::ForEach { body, .. } if loop_body_has_nested_if(body)
        )
    })
}

fn loop_body_has_nested_if(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [ArtifactAction::IfElse { then_actions, .. }]
            if then_actions
                .iter()
                .any(|action| matches!(action, ArtifactAction::IfElse { .. }))
    )
}

fn loop_body_has_branch_send(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [ArtifactAction::IfElse {
            then_actions,
            else_actions,
            ..
        }] if branch_sends_notice(then_actions) || branch_sends_notice(else_actions)
    )
}

fn loop_body_sends_directly(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [ArtifactAction::Emit { .. }, ArtifactAction::Send { payload: Some(payload), .. }]
            if loop_element_phase_payload(payload)
    )
}

fn branch_sends_notice(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [ArtifactAction::Emit { .. }, ArtifactAction::Send { payload: Some(payload), .. }]
            if loop_element_phase_payload(payload)
    )
}

fn loop_element_phase_payload(payload: &ArtifactValueTemplate) -> bool {
    matches!(
        payload,
        ArtifactValueTemplate::RecordField { record, field, .. }
            if field == "phase"
                && matches!(record.as_ref(), ArtifactValueTemplate::LoopElement { .. })
    )
}
