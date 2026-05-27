use super::support::*;

const SPAWN_SOURCE: &str = r#"
module spawn_authority;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }
enum PeerState { Idle }
enum PeerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

AUTHORITY_DECLS

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! EFFECTS ~ [] @det {
        let worker: ProcessRef<SPAWN_TARGET> = spawn SPAWN_TARGET;
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
        return Stop(state);
    }
}

proc Peer mailbox bounded(1) {
    type State = PeerState;
    type Msg = PeerMsg;

    fn init() -> PeerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: PeerState, Ping) -> ProcResult<PeerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[test]
fn accepts_exact_spawn_authority_for_dynamic_local_spawn() {
    let checked = check_source(&spawn_source(
        "    authority spawn_worker: Cap<Spawn<Worker>>;",
        "[spawn]",
        "Worker",
    ))
    .expect("exact spawn authority should check");
    let main = checked
        .processes()
        .first()
        .expect("Main process should be checked");

    assert_eq!(main.authorities().len(), 1);
    assert_eq!(main.spawn_sites().len(), 1);
    assert!(matches!(
        main.authorities()[0].descriptor(),
        CheckedCapabilityDescriptor::Spawn { target }
            if target == checked_process_id(1)
    ));
    assert_eq!(main.spawn_sites()[0].authority(), checked_authority_id(0));
    assert_eq!(main.spawn_sites()[0].target(), checked_process_id(1));
}

#[test]
fn rejects_spawn_without_matching_authority() {
    let err = check_source(&spawn_source("", "[spawn]", "Worker"))
        .expect_err("spawn without matching authority should fail");

    assert!(
        err.to_string()
            .contains("process Main spawn target Worker requires authority Cap<Spawn<Worker>>"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_with_authority_for_different_target() {
    let err = check_source(&spawn_source(
        "    authority spawn_peer: Cap<Spawn<Peer>>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("wrong target authority should fail");

    assert!(
        err.to_string()
            .contains("process Main spawn target Worker requires authority Cap<Spawn<Worker>>"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_authority_without_capability_wrapper() {
    let err = check_source(&spawn_source(
        "    authority spawn_worker: Spawn<Worker>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("authority must be a capability descriptor");

    assert!(
        err.to_string()
            .contains("process Main authority type must be Cap<Spawn<ProcessName>>"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_authority_without_spawn_descriptor() {
    let err = check_source(&spawn_source(
        "    authority spawn_worker: Cap<Worker>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("capability descriptor must be Spawn<ProcessName>");

    assert!(
        err.to_string()
            .contains("process Main authority descriptor must be Spawn<ProcessName>"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_authority_target_that_is_not_a_process_name() {
    let err = check_source(&spawn_source(
        "    authority spawn_worker: Cap<Spawn<ProcessRef<Worker>>>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("authority target must be a process name");

    assert!(
        err.to_string()
            .contains("process Main spawn authority target must be a process name"),
        "{err}"
    );
}

#[test]
fn rejects_duplicate_spawn_authority_name() {
    let err = check_source(&spawn_source(
        "    authority spawn_target: Cap<Spawn<Worker>>;\n    authority spawn_target: Cap<Spawn<Peer>>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("duplicate authority names should fail");

    assert!(
        err.to_string()
            .contains("process Main declares duplicate authority spawn_target"),
        "{err}"
    );
}

#[test]
fn rejects_duplicate_spawn_authority_descriptor() {
    let err = check_source(&spawn_source(
        "    authority spawn_worker: Cap<Spawn<Worker>>;\n    authority spawn_worker_alias: Cap<Spawn<Worker>>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("duplicate authority descriptors should fail");

    assert!(
        err.to_string()
            .contains("process Main declares duplicate spawn authority descriptor"),
        "{err}"
    );
}

#[test]
fn rejects_unused_spawn_authority_descriptor() {
    let err = check_source(&spawn_source(
        "    authority spawn_worker: Cap<Spawn<Worker>>;\n    authority spawn_peer: Cap<Spawn<Peer>>;",
        "[spawn]",
        "Worker",
    ))
    .expect_err("unused authority descriptors should fail");

    assert!(
        err.to_string()
            .contains("process Main declares unused spawn authority spawn_peer"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_authority_targeting_entry_process() {
    let source = r#"
module spawn_authority_entry_target;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;
    authority spawn_main: Cap<Spawn<Main>>;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let err = check_source(source).expect_err("entry process spawn authority should fail");

    assert!(
        err.to_string().contains(
            "process Worker spawn authority targets entry process Main, which is already started"
        ),
        "{err}"
    );
}

#[test]
fn rejects_authorized_spawn_without_spawn_effect_usage() {
    let err = check_source(&spawn_source(
        "    authority spawn_worker: Cap<Spawn<Worker>>;",
        "[]",
        "Worker",
    ))
    .expect_err("spawn still requires explicit effect usage");

    assert!(
        err.to_string()
            .contains("step uses effect spawn but does not declare it"),
        "{err}"
    );
}

#[test]
fn rejects_source_types_named_like_capability_surface() {
    for (replacement, expected) in [
        ("record Cap;", "type name Cap is reserved"),
        ("record Spawn;", "type name Spawn is reserved"),
    ] {
        let source = spawn_source(
            "    authority spawn_worker: Cap<Spawn<Worker>>;",
            "[spawn]",
            "Worker",
        )
        .replace("record MainState;", replacement);
        let err = check_source(&source).expect_err("capability surface type name should fail");

        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in `{err}`"
        );
    }

    for (replacement, expected) in [
        ("enum Cap { Start }", "type name Cap is reserved"),
        ("enum Spawn { Start }", "type name Spawn is reserved"),
    ] {
        let source = spawn_source(
            "    authority spawn_worker: Cap<Spawn<Worker>>;",
            "[spawn]",
            "Worker",
        )
        .replace("enum MainMsg { Start }", replacement);
        let err = check_source(&source).expect_err("capability surface type name should fail");

        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in `{err}`"
        );
    }
}

#[test]
fn bounded_spawn_authority_models_require_exact_target_descriptor() {
    struct Case {
        authority: &'static str,
        target: &'static str,
        accepted: bool,
    }

    let cases = [
        Case {
            authority: "    authority spawn_worker: Cap<Spawn<Worker>>;",
            target: "Worker",
            accepted: true,
        },
        Case {
            authority: "    authority spawn_worker: Cap<Spawn<Worker>>;\n    authority spawn_peer: Cap<Spawn<Peer>>;",
            target: "Worker",
            accepted: false,
        },
        Case {
            authority: "",
            target: "Worker",
            accepted: false,
        },
        Case {
            authority: "    authority spawn_peer: Cap<Spawn<Peer>>;",
            target: "Worker",
            accepted: false,
        },
        Case {
            authority: "    authority spawn_worker: Cap<Spawn<Worker>>;",
            target: "Peer",
            accepted: false,
        },
    ];

    for case in cases {
        let checked = check_source(&spawn_source(case.authority, "[spawn]", case.target));
        assert_eq!(
            checked.is_ok(),
            case.accepted,
            "authority {:?} targeting {} produced {:?}",
            case.authority,
            case.target,
            checked.err()
        );
    }
}

fn spawn_source(authority_decls: &str, effects: &str, target: &str) -> String {
    SPAWN_SOURCE
        .replace("AUTHORITY_DECLS", authority_decls)
        .replace("EFFECTS", effects)
        .replace("SPAWN_TARGET", target)
}
