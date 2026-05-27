use super::super::support::*;

#[test]
fn runtime_if_else_rejects_malformed_equality_operand_type_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_if_else_bad_equality_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_if_else_bad_equality.mta";
    let invalid_trace_stem = "runtime_if_else_bad_equality";

    gate.check("examples/runtime_if_else.str");
    gate.build("examples/runtime_if_else.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let bool_type = value_type_id(&artifact, "Bool");
    let main_state_type = value_type_id(&artifact, "MainState");
    let encoded = replace_exactly_once(
        &artifact.encode(),
        &format!(
            "process.1.transition.0.action.0.condition.operand_type_id={}\n",
            bool_type.as_u32()
        ),
        &format!(
            "process.1.transition.0.action.0.condition.operand_type_id={}\n",
            main_state_type.as_u32()
        ),
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process Worker transition 0 if condition.operand_type_id must be Bool, a scalar value type, or a fieldless enum value type"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_if_else_rejects_malformed_composed_predicate_operand_before_runtime() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_if_else_bad_composed_predicate";
    let seed_artifact_path = "target/strata/runtime_if_else_bad_composed_predicate_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_if_else_bad_composed_predicate.mta";
    let source = include_str!("../../../../../examples/runtime_if_else.str")
        .replace(
            "module runtime_if_else;",
            "module runtime_if_else_bad_composed_predicate;",
        )
        .replace(
            "if (flag == True)",
            "if ((flag == True) && !(flag == False))",
        );
    let source = gate.write_target_source(STEM, &source);
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");

    gate.check(source);
    gate.build(source, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(STEM);

    let artifact = gate.read_artifact(seed_artifact_path);
    let bool_type = value_type_id(&artifact, "Bool");
    let main_state_type = value_type_id(&artifact, "MainState");
    let encoded = replace_exactly_once(
        &artifact.encode(),
        &format!(
            "process.1.transition.0.next_state_condition.right.operand.right.type_id={}\n",
            bool_type.as_u32()
        ),
        &format!(
            "process.1.transition.0.next_state_condition.right.operand.right.type_id={}\n",
            main_state_type.as_u32()
        ),
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(&format!(
            "mantle: error: process Worker message id 0 next_state_condition.right.operand.right has type id {}, expected {}",
            main_state_type.as_u32(),
            bool_type.as_u32()
        )),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(STEM));
}

#[test]
fn statement_if_before_final_runtime_if_traces_branch_at_action_position() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_if_statement_trace_order";
    const ARTIFACT: &str = "target/strata/runtime_if_statement_trace_order.mta";
    let source = gate.write_target_source(
        STEM,
        r#"
module runtime_if_statement_trace_order;

record MainState;
enum Bool { False, True }
enum MainMsg { Start }
enum WorkerState { Idle, WarmReady, ColdReady }
enum WorkerMsg { Branch(Bool) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let warm: ProcessRef<Worker> = spawn Worker;
        let cold: ProcessRef<Worker> = spawn Worker;
        send warm Branch(True);
        send cold Branch(False);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "prefix";
        if (flag) {
            emit "statement true";
        } else {
            emit "statement false";
        }
        if (flag) {
            return Stop(WarmReady);
        } else {
            return Stop(ColdReady);
        }
    }
}
"#,
    );
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    let run = gate.check_build_run(source, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("prefix"));
    assert!(stdout.contains("statement true"));
    assert!(stdout.contains("statement false"));

    let trace = gate.read_trace(STEM);
    let warm_next_state_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"next_state""#,
    );
    let warm_action_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action""#,
    );
    let cold_next_state_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"next_state""#,
    );
    let cold_action_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"action""#,
    );

    let warm_prefix = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"prefix""#,
    );
    let warm_statement = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"statement true""#,
    );
    assert!(warm_next_state_branch < warm_prefix);
    assert!(warm_prefix < warm_action_branch);
    assert!(warm_action_branch < warm_statement);

    let cold_prefix = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"prefix""#,
    );
    let cold_statement = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":2,"text":"statement false""#,
    );
    assert!(cold_next_state_branch < cold_prefix);
    assert!(cold_prefix < cold_action_branch);
    assert!(cold_action_branch < cold_statement);
}
