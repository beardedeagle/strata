pub(super) const HELLO: &str = r#"
module hello;

record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

pub(super) const INIT_MATCH: &str = r#"
module init_match;

enum StartupMode { Cold, Warm }
enum Readiness { ColdReady, WarmReady }
record MainState { readiness: Readiness }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        match Warm {
            Cold => {
                return MainState { readiness: ColdReady };
            }
            Warm => {
                return MainState { readiness: WarmReady };
            }
        }
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "init match selected WarmReady";
        return Stop(state);
    }
}
"#;

pub(super) const FUNCTION_MATCH: &str = include_str!("../../../../../examples/function_match.str");
pub(super) const FUNCTION_PAYLOAD_MATCH: &str =
    include_str!("../../../../../examples/function_payload_match.str");
pub(super) const STATE_PAYLOAD_ENUM: &str =
    include_str!("../../../../../examples/state_payload_enum.str");
pub(super) const STATE_PAYLOAD_MATCH: &str =
    include_str!("../../../../../examples/state_payload_match.str");
pub(super) const ACTOR_REPLY: &str = include_str!("../../../../../examples/actor_reply.str");
pub(super) const RUNTIME_IF_ELSE: &str =
    include_str!("../../../../../examples/runtime_if_else.str");
pub(super) const RUNTIME_GUARD_NOOP: &str =
    include_str!("../../../../../examples/runtime_guard_noop.str");
pub(super) const RUNTIME_FOR_EACH: &str =
    include_str!("../../../../../examples/runtime_for_each.str");
pub(super) const RUNTIME_FOR_EACH_IF: &str =
    include_str!("../../../../../examples/runtime_for_each_if.str");
pub(super) const RUNTIME_FINAL_IF_NESTED_IF_ACTIONS: &str =
    include_str!("../../../../../examples/runtime_final_if_nested_if_actions.str");
pub(super) const RUNTIME_FINAL_IF_NESTED_TERMINAL_IF: &str =
    include_str!("../../../../../examples/runtime_final_if_nested_terminal_if.str");
pub(super) const RUNTIME_FOR_EACH_NESTED_IF_ACTIONS: &str =
    include_str!("../../../../../examples/runtime_for_each_nested_if_actions.str");
pub(super) const RUNTIME_GUARDED_FOR_EACH: &str =
    include_str!("../../../../../examples/runtime_guarded_for_each.str");
pub(super) const RUNTIME_GUARDED_REF_LOOP: &str =
    include_str!("../../../../../examples/runtime_guarded_ref_loop.str");

pub(super) const ACTOR_PING: &str = r#"
module actor_ping;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

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

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
"#;

pub(super) const ACTOR_SEQUENCE: &str = r#"
module actor_sequence;

record MainState;
enum MainMsg { Start }
enum WorkerState { Waiting, SawFirst, Done }
enum WorkerMsg { First, Second }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker First;
        send worker Second;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Waiting;
    }

    fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled First";
        return Continue(SawFirst);
    }

    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }
}
"#;

pub(super) const ACTOR_INSTANCES: &str = r#"
module actor_instances;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let first: ProcessRef<Worker> = spawn Worker;
        let second: ProcessRef<Worker> = spawn Worker;
        send first Ping;
        send second Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker instance handled Ping";
        return Stop(Handled);
    }
}
"#;
