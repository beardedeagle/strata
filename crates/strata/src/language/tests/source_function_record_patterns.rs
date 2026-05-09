use super::support::*;

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

    check_source(source).expect("source helper return match record pattern should check");
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

    check_source(source).expect("source helper body match record pattern should check");
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

#[test]
fn checks_source_function_record_destructuring_pattern() {
    let source = r#"
module source_function_record_pattern;

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

fn phase_of(Job { phase: phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
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

    check_source(source).expect("source helper record destructuring pattern should check");
}

#[test]
fn checks_source_function_record_destructuring_shorthand_pattern() {
    let source = r#"
module source_function_record_shorthand_pattern;

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

fn phase_of(Job { phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
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

    check_source(source).expect("source helper shorthand record pattern should check");
}

#[test]
fn rejects_source_function_record_pattern_unknown_field() {
    let source = r#"
module source_function_record_pattern_unknown_field;

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

fn phase_of(Job { missing: phase }) -> JobPhase ! [] ~ [] @det {
    return phase;
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

    let err = check_source(source).expect_err("unknown record pattern field should fail");

    assert!(
        err.to_string()
            .contains("module function phase_of record pattern Job has no field missing")
    );
}

#[test]
fn rejects_source_function_record_pattern_duplicate_field() {
    let source = r#"
module source_function_record_pattern_duplicate_field;

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

fn phase_of(Job { phase: current, phase: duplicate }) -> JobPhase ! [] ~ [] @det {
    return current;
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

    let err = check_source(source).expect_err("duplicate record pattern field should fail");

    assert!(
        err.to_string().contains(
            "module function phase_of record pattern Job binds field phase more than once"
        )
    );
}

#[test]
fn rejects_source_function_record_pattern_binding_constructor_conflict() {
    let source = r#"
module source_function_record_pattern_binding_conflict;

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

fn phase_of(Job { phase: Ready }) -> JobPhase ! [] ~ [] @det {
    return Ready;
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

    let err = check_source(source).expect_err("record pattern binding conflict should fail");

    assert!(err.to_string().contains(
        "module function phase_of record pattern binding Ready conflicts with a declared type or value constructor"
    ));
}

#[test]
fn rejects_source_function_record_pattern_in_match_arm() {
    let source = r#"
module source_function_record_pattern_match_arm;

enum JobPhase {
    Ready,
    Done,
}
record Job {
    phase: JobPhase,
}
enum Mode {
    Cold,
    Warm,
}
record MainState {
    phase: JobPhase,
}
enum MainMsg {
    Start,
}

fn phase_of(mode: Mode) -> JobPhase ! [] ~ [] @det {
    match mode {
        Job { phase } => {
            return phase;
        }
        Cold => {
            return Ready;
        }
        Warm => {
            return Done;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: phase_of(Cold) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("record pattern match arm should fail");

    assert!(err.to_string().contains(
        "module function phase_of match pattern Job destructures a record, but this match expects enum constructors"
    ));
}
