use super::support::*;

const LOCAL_SUPERVISION_SOURCE: &str = r#"
module local_supervision;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Work }

proc Main mailbox bounded(2) {
    type State = MainState;
    type Msg = MainMsg;

    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child worker: Worker = spawn Worker as permanent;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send worker Work;
        return Continue(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[test]
fn accepts_typed_local_one_for_one_supervision() {
    let checked =
        check_source(LOCAL_SUPERVISION_SOURCE).expect("local one_for_one supervisor should check");
    let main = &checked.processes()[0];

    assert_eq!(main.supervisor_plans().len(), 1);
    assert_eq!(main.spawn_sites().len(), 1);
    assert_eq!(
        main.spawn_sites()[0].kind(),
        CheckedSpawnKind::LexicalSupervisorChild
    );
    assert_eq!(main.spawn_sites()[0].authority(), None);
    assert_eq!(main.supervisor_plans()[0].children().len(), 1);
    assert_eq!(
        main.supervisor_plans()[0].children()[0].mode(),
        CheckedSupervisorChildMode::Permanent
    );
    assert_eq!(
        main.supervisor_plans()[0].children()[0].target(),
        checked_process_id(1)
    );

    let send = main.transitions()[0]
        .actions()
        .iter()
        .find_map(|action| match action {
            CheckedAction::Send { target, .. } => Some(target),
            _ => None,
        })
        .expect("step should lower lexical child send");
    assert!(matches!(
        send,
        CheckedSendTarget::SupervisorChild {
            supervisor,
            child,
            target
        } if supervisor.as_u32() == 0 && child.as_u32() == 0 && *target == checked_process_id(1)
    ));
}

#[test]
fn accepts_send_outcome_to_inactive_supervisor_child() {
    let checked = check_source(include_str!(
        "../../../../../examples/local_supervision_inactive_send_outcome.str"
    ))
    .expect("inactive supervisor child send outcome should check");
    let supervisor = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Supervisor")
        .expect("Supervisor process should be checked");

    assert!(supervisor.transitions().iter().any(|transition| {
        transition.actions().iter().any(|action| {
            matches!(
                action,
                CheckedAction::SendOutcome {
                    target: CheckedSendTarget::SupervisorChild { .. },
                    ..
                }
            )
        })
    }));
}

#[test]
fn rejects_supervised_panic_with_retained_old_child_message() {
    let source = LOCAL_SUPERVISION_SOURCE
        .replace("enum WorkerMsg { Work }", "enum WorkerMsg { Crash }")
        .replace(
            "send worker Work;",
            "send worker Crash;\n        send worker Crash;",
        )
        .replace(
            r#"    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }"#,
            r#"    fn step(state: WorkerState, Crash) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Panic(state);
    }"#,
        );
    let err = check_source(&source)
        .expect_err("retained message on old supervised child should fail source checks");

    assert!(
        err.to_string()
            .contains("process Worker would retain 1 unhandled message(s)"),
        "{err}"
    );
}

#[test]
fn rejects_owner_stop_with_retained_supervised_message() {
    let source = LOCAL_SUPERVISION_SOURCE.replace("return Continue(state);", "return Stop(state);");
    let err = check_source(&source)
        .expect_err("owner stop must stop supervised children before they handle messages");

    assert!(
        err.to_string()
            .contains("process Worker would retain 1 unhandled message(s)"),
        "{err}"
    );
}

#[test]
fn rejects_duplicate_supervisor_child_names() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "child worker: Worker = spawn Worker as permanent;",
        "child worker: Worker = spawn Worker as permanent;\n        child worker: Worker = spawn Worker as permanent;",
    );
    let err = check_source(&source).expect_err("duplicate supervisor child should fail");

    assert!(
        err.to_string()
            .contains("process Main declares duplicate supervisor child worker"),
        "{err}"
    );
}

#[test]
fn rejects_supervisor_child_target_mismatch() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "child worker: Worker = spawn Worker as permanent;",
        "child worker: Worker = spawn Main as permanent;",
    );
    let err = check_source(&source).expect_err("target mismatch should fail");

    assert!(
        err.to_string()
            .contains("supervisor child worker declares target Worker but spawns Main"),
        "{err}"
    );
}

#[test]
fn rejects_indirect_supervisor_cycles() {
    let source = r#"
module cyclic_supervision;

record MainState;
record WorkerState;
record HelperState;
enum MainMsg { Start }
enum WorkerMsg { Work }
enum HelperMsg { Help }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child worker: Worker = spawn Worker as permanent;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child helper: Helper = spawn Helper as permanent;
    }

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
}

proc Helper mailbox bounded(1) {
    type State = HelperState;
    type Msg = HelperMsg;

    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child worker: Worker = spawn Worker as permanent;
    }

    fn init() -> HelperState ! [] ~ [] @det {
        return HelperState;
    }

    fn step(state: HelperState, Help) -> ProcResult<HelperState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;
    let err = check_source(source).expect_err("indirect supervisor cycle should fail");

    assert!(
        err.to_string()
            .contains("local supervisor graph contains cycle Worker -> Helper -> Worker"),
        "{err}"
    );
}

#[test]
fn rejects_missing_supervisor_restart_intensity() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64)",
        "supervise local one_for_one(max_restarts: 2_u32)",
    );
    let err = parse_source(&source).expect_err("missing restart intensity should fail");

    assert!(err.to_string().contains("expected symbol ','"), "{err}");
}

#[test]
fn rejects_zero_supervisor_max_restarts() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64)",
        "supervise local one_for_one(max_restarts: 0_u32, within_ms: 1000_u64)",
    );
    let err = check_source(&source).expect_err("zero max_restarts should fail");

    assert!(
        err.to_string()
            .contains("supervisor restart intensity max_restarts must be greater than zero"),
        "{err}"
    );
}

#[test]
fn rejects_zero_supervisor_within_ms() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64)",
        "supervise local one_for_one(max_restarts: 2_u32, within_ms: 0_u64)",
    );
    let err = check_source(&source).expect_err("zero within_ms should fail");

    assert!(
        err.to_string()
            .contains("supervisor restart intensity within_ms must be greater than zero"),
        "{err}"
    );
}

#[test]
fn rejects_invalid_supervisor_child_mode() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "child worker: Worker = spawn Worker as permanent;",
        "child worker: Worker = spawn Worker as ephemeral;",
    );
    let err = parse_source(&source).expect_err("invalid supervisor child mode should fail");

    assert!(
        err.to_string()
            .contains("expected child mode permanent, transient, or temporary"),
        "{err}"
    );
}

#[test]
fn rejects_supervisor_child_name_conflicting_with_step_payload_binding() {
    let source = LOCAL_SUPERVISION_SOURCE
        .replace(
            "enum MainMsg { Start }",
            "enum MainMsg { Start, Route(ProcessRef<Worker>) }",
        )
        .replace(
            r#"    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send worker Work;
        return Continue(state);
    }"#,
            r#"    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Route(worker: ProcessRef<Worker>)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }"#,
        );
    let err = check_source(&source)
        .expect_err("supervisor child name should not shadow step payload binding");

    assert!(
        err.to_string()
            .contains("payload binding worker conflicts with a supervisor child binding"),
        "{err}"
    );
}

#[test]
fn rejects_supervisor_child_name_conflicting_with_effect_outcome_binding() {
    let source = LOCAL_SUPERVISION_SOURCE.replace(
        "send worker Work;",
        "let worker: Result<Unit,SendError<WorkerMsg>> = send worker Work;",
    );
    let err = check_source(&source)
        .expect_err("supervisor child name should not shadow effect outcome binding");

    assert!(
        err.to_string()
            .contains("effect outcome binding worker conflicts with a supervisor child binding"),
        "{err}"
    );
}

#[test]
fn rejects_supervisor_child_name_conflicting_with_loop_binding() {
    let source = LOCAL_SUPERVISION_SOURCE
        .replace(
            "enum MainMsg { Start }",
            "enum MainMsg { Start, Route(List<WorkerMsg,1>) }",
        )
        .replace(
            r#"    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send worker Work;
        return Continue(state);
    }"#,
            r#"    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Route(items: List<WorkerMsg,1>)) -> ProcResult<MainState> ! [] ~ [] @det {
        for worker in items {
        }
        return Continue(state);
    }"#,
        );
    let err =
        check_source(&source).expect_err("supervisor child name should not shadow loop binding");

    assert!(
        err.to_string()
            .contains("loop element binding worker conflicts with a supervisor child binding"),
        "{err}"
    );
}
