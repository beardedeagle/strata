use super::support::*;

fn state_match_payload_split_case(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_payload_split_case_with_other(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_case_with_other;

record MainState;
enum Phase {{ Ready, Done, Other }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Other));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_payload_split_without_discovered_payload_case(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_without_discovered_payload_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_payload_split_with_unit_message(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_with_unit_message;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Routed), Flush }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_payload_split_payload_derived_state_case(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_payload_derived_state_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase), Cancel(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, Saw(Phase) }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Cancel(Done));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_body_for(result: &str) -> String {
    format!(
        r#"{{
        match state {{
            Idle => {{
                return {result};
            }}
            SawReady => {{
                return {result};
            }}
            Done => {{
                return Stop(Done);
            }}
        }}
    }}"#
    )
}

fn payload_derived_state_match_body() -> String {
    r#"{
        match state {
            Idle => {
                return Continue(Saw(phase));
            }
            Saw(current: Phase) => {
                return Continue(Saw(phase));
            }
        }
    }"#
    .to_string()
}

mod disjoint_routing;
mod overlap_rejections;
mod wildcard_coverage;
mod wildcard_rejections;
