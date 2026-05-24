use super::*;

#[test]
fn checks_source_function_return_match_record_destructuring_pattern() {
    let source = r#"
module source_function_return_match_record_pattern;

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
    return match job {
        Job { phase } => {
            return phase;
        }
    };
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

    check_source(source).expect("source function return match record pattern should check");
}

#[test]
fn rejects_source_function_return_match_record_unknown_field() {
    let source = r#"
module source_function_return_match_record_unknown_field;

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
    return match job {
        Job { missing: phase } => {
            return phase;
        }
    };
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

    let err = check_source(source).expect_err("unknown return match record field should fail");

    assert!(
        err.to_string()
            .contains("function phase_of return match record pattern Job has no field missing")
    );
}

#[test]
fn rejects_source_function_return_match_record_binding_conflict() {
    let source = r#"
module source_function_return_match_record_binding_conflict;

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
    return match job {
        Job { phase: job } => {
            return job;
        }
    };
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

    let err = check_source(source).expect_err("record return match binding conflict should fail");

    assert!(err.to_string().contains(
        "function phase_of return match record pattern binding job conflicts with an existing source value binding"
    ));
}
