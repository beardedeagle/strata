use super::*;

#[test]
fn checks_source_function_body_match_record_destructuring_pattern() {
    let source = r#"
module source_function_body_match_record_pattern;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Job { phase } => {
            return phase;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: phase_of(Job { phase: Ready }) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source).expect("source function body match record pattern should check");
}

#[test]
fn checks_source_function_record_patterns_after_payload_expansion() {
    let source = r#"
module source_function_record_patterns_after_payload_expansion;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
enum Work {
    Empty,
    Assigned(Job),
}
record MainState {
    signature: JobPhase,
    body: JobPhase,
    returned: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_signature(Job { phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
}

fn phase_body(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Job { phase } => {
            return phase;
        }
    }
}

fn phase_return(job: Job) -> JobPhase ! [] ~ [] @det {
    return match job {
        Job { phase } => {
            return phase;
        }
    };
}

fn state_for(Assigned(job: Job)) -> MainState ! [] ~ [] @det {
    return MainState {
        signature: phase_signature(job),
        body: phase_body(job),
        returned: phase_return(job),
    };
}

fn state_for(Empty) -> MainState ! [] ~ [] @det {
    return MainState {
        signature: Done,
        body: Done,
        returned: Done,
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return state_for(Assigned(Job { phase: Ready }));
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source)
        .expect("record patterns should destructure concrete payload records after expansion");
}

#[test]
fn rejects_source_function_record_pattern_non_concrete_function_call_argument() {
    let source = r#"
module source_function_record_pattern_non_concrete_function_call;

enum JobPhase {
    Ready,
    Done,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
    Assign(MainState),
}

fn same_state(state: MainState) -> MainState ! [] ~ [] @det {
    return state;
}

fn phase_of(MainState { phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: Ready };
    }

    fn step(state: MainState, Assign(next: MainState)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(MainState { phase: phase_of(same_state(next)) });
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_record_value_argument_error(
        source,
        "function phase_of record pattern MainState",
    );
}

#[test]
fn rejects_source_function_body_match_non_concrete_record_scrutinee() {
    let source = r#"
module source_function_body_match_non_concrete_record_scrutinee;

enum JobPhase {
    Ready,
    Done,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
    Assign(MainState),
}

fn phase_of(state: MainState) -> JobPhase ! [] ~ [] @det {
    match state {
        MainState { phase } => {
            return phase;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: Ready };
    }

    fn step(state: MainState, Assign(next: MainState)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(MainState { phase: phase_of(next) });
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_record_value_argument_error(source, "function phase_of match dispatch");
}

#[test]
fn rejects_source_function_return_match_non_concrete_record_scrutinee() {
    let source = r#"
module source_function_return_match_non_concrete_record_scrutinee;

enum JobPhase {
    Ready,
    Done,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
    Assign(MainState),
}

fn phase_of(state: MainState) -> JobPhase ! [] ~ [] @det {
    return match state {
        MainState { phase } => {
            return phase;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: Ready };
    }

    fn step(state: MainState, Assign(next: MainState)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(MainState { phase: phase_of(next) });
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_record_value_argument_error(source, "function phase_of return match");
}

#[test]
fn rejects_source_function_body_match_record_unknown_field() {
    let source = r#"
module source_function_body_match_record_unknown_field;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Job { missing: phase } => {
            return phase;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: phase_of(Job { phase: Ready }) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("unknown body match record field should fail");

    assert!(
        err.to_string()
            .contains("module function phase_of match record pattern Job has no field missing")
    );
}

#[test]
fn rejects_source_function_body_match_record_binding_conflict() {
    let source = r#"
module source_function_body_match_record_binding_conflict;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Job { phase: job } => {
            return job;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: phase_of(Job { phase: Ready }) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("body match record binding conflict should fail");

    assert!(err.to_string().contains(
        "function phase_of match record pattern binding job conflicts with an existing source value binding"
    ));
}

#[test]
fn rejects_source_function_body_match_record_wildcard_pattern() {
    let source = r#"
module source_function_body_match_record_wildcard;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        _ => {
            return Ready;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: phase_of(Job { phase: Ready }) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("body match record wildcard should fail");

    assert!(
        err.to_string().contains(
            "module function phase_of match over record Job cannot use a wildcard pattern"
        )
    );
}

#[test]
fn rejects_source_function_body_match_record_constructor_pattern() {
    let source = r#"
module source_function_body_match_record_constructor;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_of(job: Job) -> JobPhase ! [] ~ [] @det {
    match job {
        Ready => {
            return Ready;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: phase_of(Job { phase: Ready }) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("body match record constructor arm should fail");

    assert!(err.to_string().contains(
        "module function phase_of match pattern Ready expects an enum constructor, but scrutinee is record Job"
    ));
}
