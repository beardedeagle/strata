use super::support::*;

const STEM: &str = "effect_outcomes";
const SOURCE: &str = "examples/effect_outcomes.str";
const ARTIFACT: &str = "target/strata/effect_outcomes.mta";

#[test]
fn effect_outcomes_check_build_run_and_bind_typed_commit_results() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: spawned Worker pid=2"));
    assert!(stdout.contains("mantle: spawned Worker pid=3"));
    assert!(stdout.contains("mantle: delivered Work to Worker"));
    assert!(stdout.contains("spawn accepted"));
    assert!(stdout.contains("send accepted"));

    let artifact = gate.read_artifact(ARTIFACT);
    let encoded = artifact.encode();
    let main = artifact_process(&artifact, "Main");
    let transition = main
        .transitions
        .first()
        .expect("Main should have a Start transition");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::SpawnOutcome {
                outcome: spawn_outcome,
                ..
            },
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome {
                outcome: send_outcome,
                ..
            },
            ArtifactAction::IfElse { .. },
            ArtifactAction::IfElse { .. }
        ] if *spawn_outcome == EffectOutcomeId::new(0)
            && *send_outcome == EffectOutcomeId::new(1)
    ));
    assert!(matches!(
        &transition.next_state,
        NextState::Template(ArtifactValueTemplate::Record { fields, .. })
            if fields.iter().any(|field| field.name == "sent"
                && matches!(
                    &field.value,
                    ArtifactValueTemplate::EffectOutcome {
                        outcome,
                        ..
                    } if *outcome == EffectOutcomeId::new(1)
                ))
    ));
    let spawn_outcome_ty = match &transition.actions[0] {
        ArtifactAction::SpawnOutcome { outcome_ty, .. } => *outcome_ty,
        _ => panic!("first action should bind spawn outcome"),
    };
    let ArtifactValueShape::Enum { variants } = artifact.types[spawn_outcome_ty.index()]
        .shape
        .as_ref()
        .expect("spawn outcome should be a value enum")
    else {
        panic!("spawn outcome type should be an enum");
    };
    let spawn_ok_ty = variants[0]
        .payload_type
        .expect("spawn Ok variant should carry process reference");
    assert!(matches!(
        artifact.types[spawn_ok_ty.index()].kind,
        ArtifactTypeKind::ProcessRef {
            target
        } if target == ProcessId::new(1)
    ));
    assert!(
        !encoded.contains("send_result") && !encoded.contains("spawn_result"),
        "effect outcome binding names must not lower as runtime dispatch meaning"
    );

    let trace = gate.read_trace(STEM);
    assert!(trace.contains(r#""event":"message_accepted","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Work","queue_depth":1,"sender_pid":1"#));
    assert!(trace.contains(r#""event":"state_updated","pid":1,"process_id":0,"process":"Main""#));
    assert!(
        trace.contains(r#""from":"MainState{sent:Err(MailboxClosed(Work))}""#)
            && trace.contains(r#""to":"MainState{sent:Ok(Unit)}""#)
    );
}

#[test]
fn effect_outcomes_check_build_run_and_return_mailbox_full_before_acceptance() {
    let gate = GateHarness::new();
    gate.remove_trace("effect_outcome_mailbox_full");
    let run = gate.check_build_run(
        "examples/effect_outcome_mailbox_full.str",
        "target/strata/effect_outcome_mailbox_full.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mailbox full"));

    let trace = gate.read_trace("effect_outcome_mailbox_full");
    assert!(trace.contains(r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Work","queue_depth":1,"sender_pid":1"#));
    assert!(
        trace.contains(r#""from":"MainState{sent:Err(Stopped(Work))}""#)
            && trace.contains(r#""to":"MainState{sent:Err(Full(Work))}""#)
    );
}

#[test]
fn effect_outcomes_check_build_run_and_return_stopped_target_before_acceptance() {
    let gate = GateHarness::new();
    gate.remove_trace("effect_outcome_stopped_target");
    let run = gate.check_build_run(
        "examples/effect_outcome_stopped_target.str",
        "target/strata/effect_outcome_stopped_target.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("target stopped"));

    let trace = gate.read_trace("effect_outcome_stopped_target");
    assert!(
        trace.contains(r#""from":"SenderState{sent:Err(Full(Work))}""#)
            && trace.contains(r#""to":"SenderState{sent:Err(Stopped(Work))}""#)
    );
}

#[test]
fn effect_outcomes_check_build_run_and_return_denied_before_spawn_acceptance() {
    let gate = GateHarness::new();
    gate.check("examples/effect_outcome_spawn_denied.str");
    gate.build(
        "examples/effect_outcome_spawn_denied.str",
        "target/strata/effect_outcome_spawn_denied.mta",
    );
    gate.remove_trace("effect_outcome_spawn_denied");

    let run = gate.run_mantle_success_with_args(
        "target/strata/effect_outcome_spawn_denied.mta",
        &["--deny-spawn-authority"],
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("spawn denied"));
    assert!(!stdout.contains("mantle: spawned Worker pid=2"));

    let trace = gate.read_trace("effect_outcome_spawn_denied");
    assert_trace_event(
        &trace,
        &[
            r#""event":"spawn_authority_checked""#,
            r#""target_process_id":1"#,
            r#""spawn_site_id":0"#,
            r#""authority_id":0"#,
            r#""spawn_kind":"dynamic_local""#,
            r#""authority_result":"denied""#,
        ],
    );
    assert!(!trace.contains(r#""event":"process_spawned","pid":2"#));
}

#[test]
fn effect_outcomes_check_build_and_fail_closed_when_source_creates_crashed_target() {
    let gate = GateHarness::new();
    gate.check("examples/effect_outcome_crashed_target.str");
    gate.build(
        "examples/effect_outcome_crashed_target.str",
        "target/strata/effect_outcome_crashed_target.mta",
    );
    gate.remove_trace("effect_outcome_crashed_target");

    let run = gate.run_mantle_failure("target/strata/effect_outcome_crashed_target.mta");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains(
        "mantle: error: process Worker panicked after consuming message Work; message will not be replayed"
    ));

    let trace = gate.read_trace("effect_outcome_crashed_target");
    assert!(
        trace.contains(r#""event":"process_failed","pid":2,"process_id":1,"process":"Worker""#)
    );
    assert!(!trace.contains(r#""to":"SenderState{sent:Err(Crashed(Work))}""#));
}
