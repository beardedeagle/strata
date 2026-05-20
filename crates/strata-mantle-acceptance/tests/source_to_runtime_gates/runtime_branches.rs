use super::support::*;

#[path = "runtime_branches/malformed_and_ordering.rs"]
mod malformed_and_ordering;
#[path = "runtime_branches/payload_projection.rs"]
mod payload_projection;
#[path = "runtime_branches/payload_projection_next_state.rs"]
mod payload_projection_next_state;
#[path = "runtime_branches/state_payload_projection_next_state.rs"]
mod state_payload_projection_next_state;

#[test]
fn runtime_if_else_branches_on_payload_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_if_else");
    gate.check_build_run(
        "examples/runtime_if_else.str",
        "target/strata/runtime_if_else.mta",
    );

    let artifact = gate.read_artifact("target/strata/runtime_if_else.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have a Branch transition");
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::Equal,
                left,
                right,
            },
            ..
        } if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("True")
            )
    ));
    assert!(matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::Equal,
                left,
                right,
            },
            then_actions,
            else_actions,
        }] if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("True")
            )
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));

    let trace = gate.read_trace("runtime_if_else");
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker took warm branch""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker took cold branch""#
    ));
    assert!(trace.contains(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#
    ));
    assert!(trace.contains(r#""result":"Stop","state_id":1,"state":"WarmReady""#));
    assert!(trace.contains(r#""result":"Stop","state_id":2,"state":"ColdReady""#));

    let warm_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action""#,
    );
    let warm_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker""#,
    );
    let cold_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"action""#,
    );
    let cold_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker""#,
    );
    assert!(
        warm_branch < warm_output,
        "then branch trace must precede its effect"
    );
    assert!(
        cold_branch < cold_output,
        "else branch trace must precede its effect"
    );
}

#[test]
fn runtime_guard_noop_branches_emit_selection_traces() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_guard_noop");
    let run = gate.check_build_run(
        "examples/runtime_guard_noop.str",
        "target/strata/runtime_guard_noop.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("guard saw true"));
    assert!(stdout.contains("guard enabled"));
    assert!(stdout.contains("guard saw false"));

    let artifact = gate.read_artifact("target/strata/runtime_guard_noop.mta");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have a Check transition");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::IfElse {
                then_actions: first_then,
                else_actions: first_else,
                ..
            },
            ArtifactAction::IfElse {
                then_actions: second_then,
                else_actions: second_else,
                ..
            },
            ArtifactAction::IfElse {
                then_actions: third_then,
                else_actions: third_else,
                ..
            },
        ] if matches!(first_then.as_slice(), [ArtifactAction::Emit { .. }])
            && first_else.is_empty()
            && matches!(second_then.as_slice(), [ArtifactAction::Emit { .. }])
            && second_else.is_empty()
            && third_then.is_empty()
            && matches!(third_else.as_slice(), [ArtifactAction::Emit { .. }])
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=flag") || line.contains("debug_name=flag")),
        "guard no-op artifact must not dispatch through the source flag binding name"
    );

    let trace = gate.read_trace("runtime_guard_noop");
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""branch_path":[0]"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""branch_path":[0]"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""branch_path":[1]"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""branch_path":[2]"#,
            r#""condition":"True""#,
        ],
    );

    let first_true_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0]"#,
    );
    let true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"guard saw true""#,
    );
    let true_enabled_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[1]"#,
    );
    let true_enabled_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"guard enabled""#,
    );
    let true_noop_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[2]"#,
    );
    assert!(first_true_branch < true_output);
    assert!(true_output < true_enabled_branch);
    assert!(true_enabled_branch < true_enabled_output);
    assert!(true_enabled_output < true_noop_branch);

    let false_omitted_else = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[0]"#,
    );
    let false_explicit_else = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[1]"#,
    );
    let false_effect_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[2]"#,
    );
    let false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":2,"text":"guard saw false""#,
    );
    assert!(false_omitted_else < false_explicit_else);
    assert!(false_explicit_else < false_effect_branch);
    assert!(false_effect_branch < false_output);
}

#[test]
fn runtime_guard_noop_rejects_malformed_both_empty_branch_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_guard_noop_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_guard_noop_both_empty.mta";
    let invalid_trace_stem = "runtime_guard_noop_both_empty";

    gate.check("examples/runtime_guard_noop.str");
    gate.build("examples/runtime_guard_noop.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let action = worker
        .transitions
        .first_mut()
        .expect("Worker should have a Check transition")
        .actions
        .first_mut()
        .expect("Worker transition should have a guard action");
    let ArtifactAction::IfElse {
        then_actions,
        else_actions,
        ..
    } = action
    else {
        panic!("first Worker action should be if_else");
    };
    then_actions.clear();
    else_actions.clear();
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("mantle: error: process Worker transition 0 runtime if action branches cannot both be empty"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_if_else_not_equal_branches_on_payload_at_mantle_runtime() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_if_else_not_equal";
    const ARTIFACT: &str = "target/strata/runtime_if_else_not_equal.mta";
    let source = include_str!("../../../../examples/runtime_if_else.str")
        .replace(
            "module runtime_if_else;",
            "module runtime_if_else_not_equal;",
        )
        .replace("if (flag == True)", "if (flag != False)");
    let source = gate.write_target_source(STEM, &source);
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    gate.check_build_run(source, ARTIFACT);

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have a Branch transition");
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::NotEqual,
                left,
                right,
            },
            ..
        } if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("False")
            )
    ));

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker took warm branch""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker took cold branch""#
    ));
}

#[test]
fn runtime_if_else_composed_predicate_branches_on_payload_at_mantle_runtime() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_if_else_composed_predicate";
    const ARTIFACT: &str = "target/strata/runtime_if_else_composed_predicate.mta";
    let source = include_str!("../../../../examples/runtime_if_else.str")
        .replace(
            "module runtime_if_else;",
            "module runtime_if_else_composed_predicate;",
        )
        .replace(
            "if (flag == True)",
            "if (((flag == True) && !(flag == False)) || (flag != False))",
        );
    let source = gate.write_target_source(STEM, &source);
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    gate.check_build_run(source, ARTIFACT);

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have a Branch transition");
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::BooleanBinary {
                ty,
                operator: ArtifactValueBooleanOperator::Or,
                left,
                right,
            },
            ..
        } if *ty == bool_type
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::BooleanBinary {
                    ty,
                    operator: ArtifactValueBooleanOperator::And,
                    ..
                } if *ty == bool_type
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Equality {
                    ty,
                    operand_ty,
                    operator: ArtifactValueEqualityOperator::NotEqual,
                    ..
                } if *ty == bool_type && *operand_ty == bool_type
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=flag") || line.contains("debug_name=flag")),
        "composed predicate artifact must not dispatch through the source binding name"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
}

#[test]
fn runtime_fieldless_enum_equality_branches_at_mantle_runtime() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_enum_equality";
    const ARTIFACT: &str = "target/strata/runtime_enum_equality.mta";
    let source = r#"
module runtime_enum_equality;

record MainState;

enum Bool {
    False,
    True,
}

enum MainMsg {
    Start,
}

enum Status {
    Open,
    Done,
}

enum WorkerState {
    Idle,
    StillOpen,
    Complete,
}

enum WorkerMsg {
    Check(Status),
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let open_worker: ProcessRef<Worker> = spawn Worker;
        let done_worker: ProcessRef<Worker> = spawn Worker;
        send open_worker Check(Open);
        send done_worker Check(Done);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Check(status: Status)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        if (status != Done) {
            emit "worker still open";
            return Stop(StillOpen);
        } else {
            emit "worker complete";
            return Stop(Complete);
        }
    }
}
"#;
    let source = gate.write_target_source(STEM, source);
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    let run = gate.check_build_run(source, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("worker still open"));
    assert!(stdout.contains("worker complete"));

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let status_type = value_type_id(&artifact, "Status");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have a Check transition");
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::NotEqual,
                left,
                right,
            },
            ..
        } if *ty == bool_type
            && *operand_ty == status_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == status_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == status_type && value == &artifact_value("Done")
            )
    ));
    assert!(matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::NotEqual,
                left,
                right,
            },
            then_actions,
            else_actions,
        }] if *ty == bool_type
            && *operand_ty == status_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == status_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == status_type && value == &artifact_value("Done")
            )
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker still open""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker complete""#
    ));
}
