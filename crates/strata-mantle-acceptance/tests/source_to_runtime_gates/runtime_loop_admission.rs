use super::support::*;

#[test]
fn runtime_guarded_ref_loop_rejects_malformed_received_target_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_guarded_ref_loop_bad_target_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_guarded_ref_loop_bad_target.mta";
    let invalid_trace_stem = "runtime_guarded_ref_loop_bad_target";

    gate.check("examples/runtime_guarded_ref_loop.str");
    gate.build("examples/runtime_guarded_ref_loop.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    let worker_process_id = artifact_process_id(&artifact, "Worker");
    let invalid_target_process_id = artifact_process_id(&artifact, "BatchWorker");
    let worker_ref_type = process_ref_type_id(&artifact, worker_process_id);
    {
        let batch_worker = artifact
            .processes
            .iter_mut()
            .find(|process| process.debug_name == "BatchWorker")
            .expect("artifact process BatchWorker should exist");
        let route_transition = batch_worker
            .transitions
            .iter_mut()
            .find(|transition| transition.message == MessageId::new(1))
            .expect("BatchWorker Route transition should exist");
        let [
            ArtifactAction::IfElse {
                then_actions: outer_then_actions,
                ..
            },
        ] = route_transition.actions.as_mut_slice()
        else {
            panic!("Route transition should contain only the outer guarded branch");
        };
        let [ArtifactAction::ForEach { body, .. }] = outer_then_actions.as_mut_slice() else {
            panic!("Route enabled branch should contain only the bounded loop");
        };
        let [
            ArtifactAction::IfElse {
                then_actions: inner_then_actions,
                ..
            },
        ] = body.as_mut_slice()
        else {
            panic!("Route loop body should contain only the item guard");
        };
        let [
            ArtifactAction::Emit { .. },
            ArtifactAction::Send {
                target: ArtifactSendTarget::ReceivedPayload { target_process, .. },
                ..
            },
        ] = inner_then_actions.as_mut_slice()
        else {
            panic!("Route loop item guard should emit and send through received process ref");
        };
        *target_process = invalid_target_process_id;
    }
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    let expected = format!(
        "mantle: error: artifact field send target payload type type id {} targets process id {}, expected {}",
        worker_ref_type.as_u32(),
        worker_process_id.as_u32(),
        invalid_target_process_id.as_u32()
    );
    assert!(
        stderr.contains(&expected),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_if_rejects_inactive_branch_condition_loop_element_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_if_bad_condition_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_if_bad_condition.mta";
    let invalid_trace_stem = "runtime_for_each_if_bad_condition";

    gate.check("examples/runtime_for_each_if.str");
    gate.build("examples/runtime_for_each_if.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action.0.condition.left.left.loop_element=0\n",
        "process.1.transition.0.action.1.body_action.0.condition.left.left.loop_element=1\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process BatchWorker transition 0 if condition.left.left references inactive loop element id 1"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_if_rejects_malformed_branch_send_target_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_if_bad_target_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_if_bad_target.mta";
    let invalid_trace_stem = "runtime_for_each_if_bad_target";

    gate.check("examples/runtime_for_each_if.str");
    gate.build("examples/runtime_for_each_if.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action.0.then_action.1.target_process_ref=0\n",
        "process.1.transition.0.action.1.body_action.0.then_action.1.target_process_ref=1\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process BatchWorker references undefined process reference id 1"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_if_preflights_malformed_loop_bool_before_branch_effects() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_if_bad_loop_bool_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_if_bad_loop_bool.mta";
    let invalid_trace_stem = "runtime_for_each_if_bad_loop_bool";

    gate.check("examples/runtime_for_each_if.str");
    gate.build("examples/runtime_for_each_if.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.0.transition.0.action.1.payload_template.value=List[True,False]\n",
        "process.0.transition.0.action.1.payload_template.value=List[True,Maybe]\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("mantle: error: process Main transition 0 send payload.item.1 value Maybe is not a member of enum type Bool"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_rejects_direct_process_ref_payload_before_build() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_for_each_ref_payload";
    const ARTIFACT: &str = "target/strata/runtime_for_each_ref_payload.mta";
    let source = gate.write_target_source(
        STEM,
        r#"
module runtime_for_each_ref_payload;

record MainState;
record HubState;
record SinkState;
enum MainMsg { Start }
enum WorkerState { Holding(List<Bool,2>) }
enum WorkerMsg { Work(ProcessRef<Sink>) }
enum HubMsg { Route(ProcessRef<Sink>) }
enum SinkMsg { Done }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority spawn_sink: Cap<Spawn<Sink>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Work(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    authority spawn_hub: Cap<Spawn<Hub>>;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(List<Bool,2>[True, False]);
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        match state {
            Holding(items: List<Bool,2>) => {
                let hub: ProcessRef<Hub> = spawn Hub;
                for item in items {
                    send hub Route(reply_to);
                }
                return Stop(Holding(items));
            }
        }
    }
}

proc Hub mailbox bounded(2) {
    type State = HubState;
    type Msg = HubMsg;

    fn init() -> HubState ! [] ~ [] @det {
        return HubState;
    }

    fn step(state: HubState, Route(reply_to: ProcessRef<Sink>)) -> ProcResult<HubState> ! [send] ~ [] @det {
        send reply_to Done;
        return Continue(state);
    }
}

proc Sink mailbox bounded(2) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#,
    );
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_artifact(ARTIFACT);

    let check = gate.check_failure(source);
    let stderr = String::from_utf8_lossy(&check.stderr);

    assert!(
        stderr.contains("process reference payload templates must be direct message payloads"),
        "unexpected diagnostic\nstderr:\n{stderr}"
    );
    assert!(
        !gate.root.join(ARTIFACT).exists(),
        "source check failure must not create {ARTIFACT}"
    );
}

#[test]
fn runtime_for_each_empty_collection_runs_zero_body_iterations() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_for_each_empty");
    let run = gate.check_build_run(
        "examples/runtime_for_each_empty.str",
        "target/strata/runtime_for_each_empty.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Batch(List[]) to BatchWorker"));
    assert!(!stdout.contains("mantle: delivered Branch("));
    assert!(!stdout.contains("worker handled"));

    let artifact = gate.read_artifact("target/strata/runtime_for_each_empty.mta");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                max_items: 0,
                body,
                ..
            },
        ] if matches!(body.as_slice(), [ArtifactAction::Send { .. }])
    ));

    let trace = gate.read_trace("runtime_for_each_empty");
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""process":"BatchWorker""#,
            r#""max_items":0"#,
            r#""item_count":0"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""process":"BatchWorker""#,
            r#""iteration_count":0"#,
        ],
    );
    assert!(
        !trace.contains(r#""event":"loop_iteration""#),
        "empty runtime collection must not execute loop body"
    );
    assert!(
        !trace.contains(r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker""#),
        "empty runtime collection must not send loop body messages"
    );
}

#[test]
fn runtime_for_each_rejects_missing_artifact_body_block_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_missing_body_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_missing_body.mta";
    let invalid_trace_stem = "runtime_for_each_missing_body";

    gate.check("examples/runtime_for_each.str");
    gate.build("examples/runtime_for_each.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action_count=1\n",
        "",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: missing artifact field process.1.transition.0.action.1.body_action_count"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_rejects_inactive_artifact_loop_element_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_bad_element_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_bad_element.mta";
    let invalid_trace_stem = "runtime_for_each_bad_element";

    gate.check("examples/runtime_for_each.str");
    gate.build("examples/runtime_for_each.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.1.transition.0.action.1.body_action.0.payload_template.loop_element=0\n",
        "process.1.transition.0.action.1.body_action.0.payload_template.loop_element=1\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process BatchWorker transition 0 send payload references inactive loop element id 1"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_for_each_rejects_malformed_runtime_collection_value_fail_closed() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_malformed_collection_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_malformed_collection.mta";
    let invalid_trace_stem = "runtime_for_each_malformed_collection";

    gate.check("examples/runtime_for_each.str");
    gate.build("examples/runtime_for_each.str", seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let artifact = gate.read_artifact(seed_artifact_path);
    let encoded = replace_exactly_once(
        &artifact.encode(),
        "process.0.transition.0.action.1.payload_template.value=List[True,False]\n",
        "process.0.transition.0.action.1.payload_template.value=True\n",
    );
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &encoded);

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(
            "mantle: error: process Main transition 0 send payload value True does not match list type"
        ),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}
