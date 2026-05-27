use super::*;

#[test]
fn rejects_shape_only_list_payload_step_pattern() {
    let source = r#"
module shape_only_list_payload_step_pattern;

enum Phase {
    Ready,
}
record MainState;
enum MainMsg {
    Start,
    Items(List<Phase,1>),
}

proc Main mailbox bounded(2) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send Main Items(List<Phase,1>[Ready]);
        return Continue(state);
    }

    fn step(state: MainState, Items(List[_])) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("shape-only list payload pattern should fail");

    assert!(
        err.to_string().contains(
            "process Main step pattern list payload pattern must bind at least one value"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_shape_only_nested_constructor_in_list_payload_step_pattern() {
    let source = r#"
module shape_only_nested_constructor_in_list_payload_step_pattern;

enum Phase {
    Ready,
    Done,
}
enum Routed {
    Assign(Phase),
}
record MainState;
enum MainMsg {
    Start,
    Items(List<Routed,2>),
}

proc Main mailbox bounded(2) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send Main Items(List<Routed,2>[Assign(Ready), Assign(Done)]);
        return Continue(state);
    }

    fn step(state: MainState, Items(List[Assign(Ready), ..tail])) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source)
        .expect_err("shape-only nested constructor in list payload pattern should fail");

    assert!(
        err.to_string().contains(
            "process Main step pattern list payload nested pattern must bind at least one value"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_shape_only_map_payload_step_pattern() {
    let source = r#"
module shape_only_map_payload_step_pattern;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
    Lookup(Map<Phase,Phase,1>),
}

proc Main mailbox bounded(2) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send Main Lookup(Map<Phase,Phase,1>[Ready => Done]);
        return Continue(state);
    }

    fn step(state: MainState, Lookup(Map[Ready => _])) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("shape-only map payload pattern should fail");

    assert!(
        err.to_string()
            .contains("process Main step pattern map payload pattern must bind at least one value"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_shape_only_subset_map_payload_step_pattern() {
    let source = r#"
module shape_only_subset_map_payload_step_pattern;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
    Lookup(Map<Phase,Phase,1>),
}

proc Main mailbox bounded(2) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send Main Lookup(Map<Phase,Phase,1>[Ready => Done]);
        return Continue(state);
    }

    fn step(state: MainState, Lookup(Map[..])) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("shape-only subset map pattern should fail");

    assert!(
        err.to_string().contains(
            "process Main step pattern subset map payload pattern must declare at least one key"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_dynamic_key_map_payload_step_pattern() {
    let source = r#"
module dynamic_key_map_payload_step_pattern;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Lookup(Map<Phase,Phase,1>),
}

proc Main mailbox bounded(1) {
    type State = Phase;
    type Msg = MainMsg;

    fn init() -> Phase ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: Phase, Lookup(Map[state => phase])) -> ProcResult<Phase> ! [] ~ [] @det {
        return Stop(phase);
    }
}
"#;

    let err = check_source(source).expect_err("dynamic map pattern key should fail");

    assert!(
        err.to_string().contains(
            "process Main step pattern map payload pattern keys must be static source values of type Phase"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_list_payload_step_pattern_length_mismatch() {
    let source = r#"
module list_payload_step_pattern_length_mismatch;

enum Phase {
    Ready,
}
record MainState;
record WorkerState;
enum MainMsg {
    Start,
}
enum WorkerMsg {
    Items(List<Phase,2>),
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
        send worker Items(List<Phase,2>[Ready]);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Items(List[phase, _])) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("list payload shape mismatch should fail");

    assert!(
        err.to_string()
            .contains("message payload List[Ready] does not match pattern binding phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_map_payload_step_pattern_key_set_mismatch() {
    let source = r#"
module map_payload_step_pattern_key_set_mismatch;

enum Phase {
    Ready,
    Done,
}
record MainState;
record WorkerState;
enum MainMsg {
    Start,
}
enum WorkerMsg {
    Lookup(Map<Phase,Phase,2>),
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
        send worker Lookup(Map<Phase,Phase,2>[Done => Ready]);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Lookup(Map[Ready => phase])) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("map payload key-set mismatch should fail");

    assert!(
        err.to_string()
            .contains("message payload Map[Done=>Ready] does not match pattern binding phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_exact_map_payload_step_pattern_extra_keys() {
    let source = r#"
module exact_map_payload_step_pattern_extra_keys;

enum Phase {
    Ready,
    Done,
}
record MainState;
record WorkerState;
enum MainMsg {
    Start,
}
enum WorkerMsg {
    Lookup(Map<Phase,Phase,2>),
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
        send worker Lookup(Map<Phase,Phase,2>[Ready => Done, Done => Ready]);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Lookup(Map[Ready => phase])) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("exact map payload extra keys should fail");

    assert!(
        err.to_string().contains(
            "message payload Map[Ready=>Done,Done=>Ready] does not match pattern binding phase"
        ),
        "unexpected error: {err}"
    );
}
