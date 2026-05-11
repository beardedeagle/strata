use super::super::support::*;

#[test]
fn rejects_duplicate_static_map_template_keys_with_runtime_value() {
    let source = r#"
module duplicate_static_map_template_keys_with_runtime_value;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,2>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,2> ! [] ~ [] @det {
        return Map<Phase,Phase,2>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,2>, Start) -> ProcResult<Map<Phase,Phase,2>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: Map<Phase,Phase,2>, Replace(next: Phase)) -> ProcResult<Map<Phase,Phase,2>> ! [] ~ [] @det {
        return Continue(Map<Phase,Phase,2>[Ready => next, Ready => Done]);
    }
}
"#;

    let err = check_source(source).expect_err("duplicate static template keys should fail");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,2> duplicates key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_runtime_dependent_map_key_state_template() {
    let source = r#"
module runtime_dependent_map_key_state_template;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,1>, Start) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: Map<Phase,Phase,1>, Replace(next: Phase)) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Continue(Map<Phase,Phase,1>[next => Ready]);
    }
}
"#;

    let err = check_source(source).expect_err("runtime-dependent map keys should fail");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_source_helper_runtime_dependent_map_key_state_template() {
    let source = r#"
module source_helper_runtime_dependent_map_key_state_template;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

fn keyed(next: Phase) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    return Map<Phase,Phase,1>[next => Ready];
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,1>, Start) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: Map<Phase,Phase,1>, Replace(next: Phase)) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Continue(keyed(next));
    }
}
"#;

    let err = check_source(source).expect_err("helper-bound runtime map keys should fail");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_runtime_dependent_map_key_send_payload_template() {
    let source = r#"
module runtime_dependent_map_key_send_payload_template;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}
enum WorkerMsg {
    Replace(Phase),
}
enum MapWorkerMsg {
    ReplaceMap(Map<Phase,Phase,1>),
}

proc Main mailbox bounded(1) {
    type State = Unit;
    type Msg = MainMsg;

    fn init() -> Unit ! [] ~ [] @det {
        return Unit;
    }

    fn step(state: Unit, Start) -> ProcResult<Unit> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Replace(Done);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = Unit;
    type Msg = WorkerMsg;

    fn init() -> Unit ! [] ~ [] @det {
        return Unit;
    }

    fn step(state: Unit, Replace(next: Phase)) -> ProcResult<Unit> ! [spawn, send] ~ [] @det {
        let map_worker: ProcessRef<MapWorker> = spawn MapWorker;
        send map_worker ReplaceMap(Map<Phase,Phase,1>[next => Ready]);
        return Stop(state);
    }
}

proc MapWorker mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MapWorkerMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,1>, ReplaceMap(next: Map<Phase,Phase,1>)) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(next);
    }
}
"#;

    let err =
        check_source(source).expect_err("runtime-dependent send payload map keys should fail");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}
