use super::super::support::*;

pub(crate) fn nested_worker_pattern_source(pattern: &str) -> String {
    format!(
        r#"
module nested_pattern_rejection;

record MainState;
record Job {{ phase: Phase }}
enum Phase {{ Ready, Done, Other }}
enum Routed {{
    Assign(Job),
    Hold(List<Job,1>),
    Lookup(Map<Phase,Job,1>),
}}
enum MainMsg {{ Start }}
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
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, {pattern}) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

pub(crate) fn assert_nested_worker_pattern_rejected(pattern: &str, expected: &str) {
    let source = nested_worker_pattern_source(pattern);
    let err = check_source(&source).expect_err("nested worker pattern should fail");
    assert!(
        err.to_string().contains(expected),
        "expected diagnostic containing {expected:?}, got {err}"
    );
}

pub(crate) fn payload_sensitive_function_case(route: &str, init_value: &str) -> String {
    format!(
        r#"
module payload_sensitive_function_case;

record MainState {{ phase: Phase }}
enum Phase {{ Ready, Done, Other }}
enum Routed {{ Assign(Phase) }}
enum Packet {{ Envelope(Routed) }}
enum MainMsg {{ Start }}

{route}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return {init_value};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

pub(crate) fn same_message_step_split_case(worker_step: &str) -> String {
    format!(
        r#"
module same_message_step_split_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

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
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

{worker_step}
}}
"#
    )
}

pub(crate) fn same_message_step_split_case_with_other(worker_step: &str) -> String {
    format!(
        r#"
module same_message_step_split_case_with_other;

record MainState;
enum Phase {{ Ready, Done, Other }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

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
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

{worker_step}
}}
"#
    )
}

pub(crate) fn same_message_step_split_without_discovered_payload_case(worker_step: &str) -> String {
    format!(
        r#"
module same_message_step_split_without_payload_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
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
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

{worker_step}
}}
"#
    )
}

pub(crate) fn fieldless_function_mismatch_source(selected_call: &str) -> String {
    format!(
        r#"
module fieldless_function_mismatch;

record MainState {{ selected: Phase }}
enum Phase {{ Ready, Done }}
enum RoutedKind {{ Mark(Phase) }}
enum MainMsg {{ Start }}

fn fieldless_signature(Mark(Ready)) -> Phase ! [] ~ [] @det {{
    return Ready;
}}

fn fieldless_body(route: RoutedKind) -> Phase ! [] ~ [] @det {{
    match route {{
        Mark(Ready) => {{
            return Ready;
        }}
    }}
}}

fn fieldless_return(route: RoutedKind) -> Phase ! [] ~ [] @det {{
    return match route {{
        Mark(Ready) => {{
            return Ready;
        }}
    }};
}}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{ selected: {selected_call} }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}
