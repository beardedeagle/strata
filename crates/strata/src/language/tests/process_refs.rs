use super::support::*;

#[test]
fn resolves_process_references_to_ids_before_artifact_encoding() {
    let source = r#"
module actor_ping;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("reordered actor ping should check");
    let main = checked
        .processes()
        .get(checked.entry_process().index())
        .expect("Main entry should be present");

    assert_eq!(checked.entry_process(), checked_process_id(1));
    assert_eq!(main.debug_name().as_str(), "Main");
    assert_eq!(
        only_transition(main).actions(),
        [
            CheckedAction::Spawn {
                target: checked_process_id(0),
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

    let artifact = lower_to_artifact(&checked, source).expect("checked program should lower");
    let encoded = artifact.encode();
    assert!(encoded.contains("entry_process=1"));
    assert!(encoded.contains("process.1.transition.0.action.0.target_process=0"));
    assert!(encoded.contains("process.1.transition.0.action.0.process_ref=0"));
    assert!(encoded.contains("process.1.transition.0.action.1.target_process_ref=0"));
    assert!(!encoded.contains("target_process=Worker"));
}

#[test]
fn rejects_duplicate_process_ref_on_same_path() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<Worker> = spawn Worker;\n        let worker: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source).expect_err("duplicate process reference should be rejected");

    assert!(
        err.to_string()
            .contains("duplicates process reference id 0")
    );
}

#[test]
fn allows_multiple_process_refs_for_same_process_definition() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;\n        send worker Ping;",
        "let first: ProcessRef<Worker> = spawn Worker;\n        let second: ProcessRef<Worker> = spawn Worker;\n        send first Ping;\n        send second Ping;",
    );

    check_source(&source).expect("distinct process refs may target the same process definition");
}

#[test]
fn rejects_spawn_without_process_ref() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "spawn Worker;",
    );

    let err = parse_source(&source).expect_err("standalone spawn should be rejected");

    assert!(
        err.to_string()
            .contains("expected emit, for, if, let, send, or return statement")
    );
}

#[test]
fn rejects_send_to_process_definition_name() {
    let source = ACTOR_PING.replace("send worker Ping;", "send Worker Ping;");

    let err = check_source(&source).expect_err("send to process definition should be rejected");

    assert!(
        err.to_string().contains(
            "process Main sends to undeclared process reference or supervisor child Worker"
        )
    );
}

#[test]
fn rejects_process_ref_named_like_step_parameter() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let state: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source)
        .expect_err("step parameter process reference name should be rejected");

    assert!(
        err.to_string()
            .contains("process Main process reference state conflicts with a step parameter name")
    );
}

#[test]
fn rejects_process_ref_named_like_process_declaration() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let Worker: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source)
        .expect_err("process declaration process reference name should be rejected");

    assert!(
        err.to_string()
            .contains("process Main process reference Worker conflicts with a process declaration")
    );
}

#[test]
fn allows_same_spawn_target_in_distinct_terminal_step_patterns() {
    let source = r#"
module spawn_by_message;

record MainState;
enum MainMsg { Start, Restart }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Stop(state);
    }

    fn step(state: MainState, Restart) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(Idle);
    }
}
"#;

    check_source(source).expect("mutually exclusive step patterns may spawn the same process");
}

#[test]
fn rejects_static_self_spawn() {
    let source = ACTOR_PING
        .replace("! [emit] ~ [] @det", "! [spawn] ~ [] @det")
        .replace(
            r#"emit "worker handled Ping";"#,
            "let loopback: ProcessRef<Worker> = spawn Worker;",
        );

    let err = check_source(&source).expect_err("self-spawn should be rejected");

    assert!(err.to_string().contains("process Worker spawns itself"));
}

#[test]
fn rejects_send_before_static_spawn() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;\n        send worker Ping;",
        "send worker Ping;\n        let worker: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source).expect_err("send before spawn should be rejected");

    assert!(
        err.to_string()
            .contains("sends through unbound process reference id 0 within message transition 0")
    );
}

#[test]
fn rejects_process_ref_type_that_does_not_match_spawn_target() {
    let source = ACTOR_PING
        .replace(
            "enum WorkerMsg { Ping }",
            "enum WorkerMsg { Ping }\nenum PeerState { Idle }\nenum PeerMsg { Ping }",
        )
        .replace(
            "let worker: ProcessRef<Worker> = spawn Worker;",
            "let worker: ProcessRef<Peer> = spawn Worker;",
        )
        .replace(
            r#"
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
"#,
            r#"
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}

proc Peer mailbox bounded(1) {
    type State = PeerState;
    type Msg = PeerMsg;

    fn init() -> PeerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: PeerState, Ping) -> ProcResult<PeerState> ! [] ~ [] @det {
        return Stop(Idle);
    }
}
"#,
        );

    let err = check_source(&source).expect_err("mismatched process ref type should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker has type ProcessRef<Peer> but spawns Worker"
    ));
}

#[test]
fn rejects_process_ref_binding_with_non_process_ref_type() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: WorkerState = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("non-ProcessRef spawn binding type should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker must be typed as ProcessRef<ProcessName>"
    ));
}

#[test]
fn rejects_process_ref_binding_with_wrong_type_constructor() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: WorkerRef<Worker> = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("wrong process reference constructor should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker must be typed as ProcessRef<ProcessName>"
    ));
}

#[test]
fn rejects_process_ref_binding_with_wrong_type_arity() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<Worker, Worker> = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("wrong process reference type arity should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker must be typed as ProcessRef<ProcessName>"
    ));
}

#[test]
fn rejects_process_ref_binding_with_nested_target_type() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<ProcessRef<Worker>> = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("nested process reference target should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker has nested process reference target type ProcessRef<Worker>"
    ));
}

#[test]
fn rejects_process_ref_type_with_undeclared_process_target() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<Unknown> = spawn Worker;",
    );

    let err = check_source(&source).expect_err("undeclared process ref target should be rejected");

    assert!(
        err.to_string()
            .contains("process Main process reference worker targets undeclared process Unknown")
    );
}

#[test]
fn rejects_send_without_static_spawn() {
    let source = ACTOR_PING
        .replace("! [spawn, send] ~ [] @det", "! [send] ~ [] @det")
        .replace(
            "        let worker: ProcessRef<Worker> = spawn Worker;\n",
            "",
        );

    let err = check_source(&source).expect_err("send without spawn should be rejected");

    assert!(
        err.to_string()
            .contains("sends to undeclared process reference or supervisor child worker")
    );
}

#[test]
fn rejects_mailbox_overflow_through_process_ref() {
    let source = ACTOR_PING.replace(
        "send worker Ping;",
        "send worker Ping;\n        send worker Ping;",
    );

    let err = check_source(&source).expect_err("mailbox overflow should be rejected");

    assert!(
        err.to_string()
            .contains("sends to Worker, but its mailbox would exceed bound 1")
    );
}

#[test]
fn rejects_unhandled_message_after_process_ref_target_stops() {
    let source = ACTOR_SEQUENCE.replace("return Continue(SawFirst);", "return Stop(SawFirst);");

    let err = check_source(&source).expect_err("message left after stop should be rejected");

    assert!(
        err.to_string()
            .contains("process Worker would retain 1 unhandled message(s)")
    );
}

#[test]
fn rejects_send_to_unknown_message() {
    let source = ACTOR_PING.replace("send worker Ping;", "send worker Unknown;");

    let err = check_source(&source).expect_err("unknown message should be rejected");

    assert!(
        err.to_string()
            .contains("sends message Unknown not accepted by Worker")
    );
}

#[test]
fn rejects_unbounded_cross_spawn_loop() {
    let source = r#"
module spawn_loop;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }
enum PeerState { Idle }
enum PeerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    authority spawn_peer: Cap<Spawn<Peer>>;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let peer: ProcessRef<Peer> = spawn Peer;
        send peer Ping;
        return Continue(Idle);
    }
}

proc Peer mailbox bounded(1) {
    type State = PeerState;
    type Msg = PeerMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> PeerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: PeerState, Ping) -> ProcResult<PeerState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Continue(Idle);
    }
}
"#;

    let err = check_source(source).expect_err("spawn loop should be rejected");

    assert!(
        err.to_string()
            .contains("static runtime process instance limit exceeded")
    );
}
