use super::assert_concrete_collection_value_argument_error;

#[test]
fn rejects_source_function_list_signature_non_concrete_argument() {
    let source = r#"
module list_signature_non_concrete_argument;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(List<Phase,1>),
}

fn first(List<Phase,1>[phase]) -> Phase ! [] ~ [] @det {
    return phase;
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: List<Phase,1>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[first(next)]);
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_collection_value_argument_error(
        source,
        "function first pattern dispatch",
        "requires a concrete list value argument",
    );
}

#[test]
fn rejects_source_function_map_signature_non_concrete_argument() {
    let source = r#"
module map_signature_non_concrete_argument;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Map<Phase,Phase,1>),
}

fn ready_value(Map<Phase,Phase,1>[Ready => selected]) -> Phase ! [] ~ [] @det {
    return selected;
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: Map<Phase,Phase,1>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[ready_value(next)]);
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_collection_value_argument_error(
        source,
        "function ready_value pattern dispatch",
        "requires a concrete map value argument",
    );
}

#[test]
fn rejects_source_function_list_body_match_non_concrete_scrutinee() {
    let source = r#"
module list_body_match_non_concrete_scrutinee;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(List<Phase,1>),
}

fn first(items: List<Phase,1>) -> Phase ! [] ~ [] @det {
    match items {
        List[phase] => {
            return phase;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: List<Phase,1>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[first(next)]);
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_collection_value_argument_error(
        source,
        "function first match dispatch",
        "requires a concrete list value argument",
    );
}

#[test]
fn rejects_source_function_map_body_match_non_concrete_scrutinee() {
    let source = r#"
module map_body_match_non_concrete_scrutinee;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Map<Phase,Phase,1>),
}

fn ready_value(items: Map<Phase,Phase,1>) -> Phase ! [] ~ [] @det {
    match items {
        Map[Ready => selected] => {
            return selected;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: Map<Phase,Phase,1>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[ready_value(next)]);
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_collection_value_argument_error(
        source,
        "function ready_value match dispatch",
        "requires a concrete map value argument",
    );
}

#[test]
fn rejects_source_function_list_return_match_non_concrete_scrutinee() {
    let source = r#"
module list_return_match_non_concrete_scrutinee;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(List<Phase,1>),
}

fn first(items: List<Phase,1>) -> Phase ! [] ~ [] @det {
    return match items {
        List[phase] => {
            return phase;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: List<Phase,1>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[first(next)]);
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_collection_value_argument_error(
        source,
        "function first return match",
        "requires a concrete list value argument",
    );
}

#[test]
fn rejects_source_function_map_return_match_non_concrete_scrutinee() {
    let source = r#"
module map_return_match_non_concrete_scrutinee;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Map<Phase,Phase,1>),
}

fn ready_value(items: Map<Phase,Phase,1>) -> Phase ! [] ~ [] @det {
    return match items {
        Map[Ready => selected] => {
            return selected;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: Map<Phase,Phase,1>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[ready_value(next)]);
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    assert_concrete_collection_value_argument_error(
        source,
        "function ready_value return match",
        "requires a concrete map value argument",
    );
}
