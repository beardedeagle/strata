use super::super::support::*;

#[test]
fn rejects_overlapping_subset_map_source_function_patterns() {
    let source = r#"
module overlapping_subset_map_source_function_patterns;

enum Phase {
    Ready,
    Done,
}
record MainState {
    selected: Phase,
}
enum MainMsg {
    Start,
}

fn pick(Map<Phase,Phase,2>[Ready => selected, ..]) -> Phase ! [] ~ [] @det {
    return selected;
}

fn pick(Map<Phase,Phase,2>[Done => selected, ..]) -> Phase ! [] ~ [] @det {
    return selected;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            selected: pick(Map<Phase,Phase,2>[Ready => Ready, Done => Done]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("overlapping subset patterns should fail");

    assert!(
        err.to_string()
            .contains("declares overlapping collection patterns"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_overlapping_exact_and_subset_map_source_function_patterns() {
    let source = r#"
module overlapping_exact_and_subset_map_source_function_patterns;

enum Phase {
    Ready,
    Done,
}
record MainState {
    selected: Phase,
}
enum MainMsg {
    Start,
}

fn pick(Map<Phase,Phase,2>[Ready => selected]) -> Phase ! [] ~ [] @det {
    return selected;
}

fn pick(Map<Phase,Phase,2>[Ready => selected, ..]) -> Phase ! [] ~ [] @det {
    return selected;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            selected: Ready,
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("exact/subset overlap should fail");

    assert!(
        err.to_string()
            .contains("declares overlapping collection patterns"),
        "unexpected error: {err}"
    );
}

#[test]
fn keeps_exact_map_source_function_patterns_exact() {
    let source = r#"
module exact_map_source_function_patterns;

enum Phase {
    Ready,
    Done,
}
record MainState {
    selected: Phase,
}
enum MainMsg {
    Start,
}

fn pick(Map<Phase,Phase,2>[Ready => selected]) -> Phase ! [] ~ [] @det {
    return selected;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            selected: pick(Map<Phase,Phase,2>[Ready => Ready, Done => Done]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("exact map pattern should reject extra keys");

    assert!(
        err.to_string()
            .contains("function pick has no collection pattern for concrete Map"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_empty_subset_map_source_function_pattern() {
    let source = r#"
module empty_subset_map_source_function_pattern;

enum Phase {
    Ready,
    Done,
}
record MainState {
    selected: Phase,
}
enum MainMsg {
    Start,
}

fn pick(Map<Phase,Phase,2>[..,]) -> Phase ! [] ~ [] @det {
    return Ready;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            selected: pick(Map<Phase,Phase,2>[Ready => Ready]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("empty subset map pattern should fail");

    assert!(
        err.to_string()
            .contains("subset map pattern must declare at least one key"),
        "unexpected error: {err}"
    );
}
