use super::super::support::*;

#[test]
fn rejects_duplicate_map_pattern_keys() {
    let source = r#"
module duplicate_map_pattern_keys;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn ready_value(items: Map<Phase,Phase,2>) -> Phase ! [] ~ [] @det {
    return match items {
        Map[Ready => first, Ready => second] => {
            return first;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("duplicate map pattern keys should fail");

    assert!(
        err.to_string().contains("map pattern duplicates key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_duplicate_map_value_keys_in_source_function() {
    let source = r#"
module duplicate_map_value_keys_in_source_function;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn duplicate(input: Phase) -> Map<Phase,Phase,2> ! [] ~ [] @det {
    return Map<Phase,Phase,2>[Ready => Done, Ready => Ready];
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("duplicate map value keys should fail");

    assert!(
        err.to_string().contains("map value duplicates key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_mismatched_list_value_type_argument() {
    let source = r#"
module mismatched_list_value_type_argument;

record Unit;
enum Phase {
    Ready,
}
enum Other {
    Different,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Other,1>[Ready];
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("mismatched list type argument should fail");

    assert!(
        err.to_string()
            .contains("list value has element type Other, expected Phase for List<Phase,1>"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_mismatched_map_value_type_argument() {
    let source = r#"
module mismatched_map_value_type_argument;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum Other {
    Different,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Other,Phase,1>[Ready => Done];
    }

    fn step(state: Map<Phase,Phase,1>, Start) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("mismatched map type argument should fail");

    assert!(
        err.to_string()
            .contains("map value has key type Other, expected Phase for Map<Phase,Phase,1>"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unbounded_list_type() {
    let source = r#"
module unbounded_list_type;

record Unit;
enum Phase {
    Ready,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = List<Phase>;
    type Msg = MainMsg;

    fn init() -> List<Phase> ! [] ~ [] @det {
        return List[Ready];
    }

    fn step(state: List<Phase>, Start) -> ProcResult<List<Phase>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("unbounded list types should fail");

    assert!(
        err.to_string().contains(
            "list type List<Phase> must declare exactly one element type and one numeric capacity"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unbounded_list_record_field_with_collection_diagnostic() {
    let source = r#"
module unbounded_list_record_field;

record Box {
    items: List<Phase>,
}
enum Phase {
    Ready,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = Box;
    type Msg = MainMsg;

    fn init() -> Box ! [] ~ [] @det {
        return Box{items: List<Phase,1>[Ready]};
    }

    fn step(state: Box, Start) -> ProcResult<Box> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("unbounded record field list type should fail");

    assert!(
        err.to_string().contains(
            "list type List<Phase> must declare exactly one element type and one numeric capacity"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unbounded_map_type() {
    let source = r#"
module unbounded_map_type;

record Unit;
enum Phase {
    Ready,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase> ! [] ~ [] @det {
        return Map[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase>, Start) -> ProcResult<Map<Phase,Phase>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("unbounded map types should fail");

    assert!(
        err.to_string().contains(
            "map type Map<Phase,Phase> must declare exactly two type arguments and one numeric capacity"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unbounded_map_payload_with_collection_diagnostic() {
    let source = r#"
module unbounded_map_payload;

record Unit;
enum Phase {
    Ready,
}
enum MainMsg {
    Start,
    Replace(Map<Phase,Phase>),
}

proc Main mailbox bounded(1) {
    type State = Unit;
    type Msg = MainMsg;

    fn init() -> Unit ! [] ~ [] @det {
        return Unit;
    }

    fn step(state: Unit, Start) -> ProcResult<Unit> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("unbounded payload map type should fail");

    assert!(
        err.to_string().contains(
            "map type Map<Phase,Phase> must declare exactly two type arguments and one numeric capacity"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_capacity_overflow_with_parser_byte_offset() {
    let source = r#"
module collection_capacity_overflow;

record Unit;
enum Phase {
    Ready,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = List<Phase,999999999999999999999999999999999999999999999999999999999999999>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = parse_source(source).expect_err("oversized collection capacity should fail parsing");
    let message = err.to_string();

    assert!(
        message.contains("type List numeric capacity must fit in usize at byte"),
        "unexpected error: {message}"
    );
}

#[test]
fn rejects_list_value_above_declared_capacity() {
    let source = r#"
module list_value_above_declared_capacity;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready, Done];
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("over-capacity list values should fail");

    assert!(
        err.to_string()
            .contains("list value length 2 exceeds capacity 1 for List<Phase,1>"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_map_value_above_declared_capacity() {
    let source = r#"
module map_value_above_declared_capacity;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready, Done => Done];
    }

    fn step(state: Map<Phase,Phase,1>, Start) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("over-capacity map values should fail");

    assert!(
        err.to_string()
            .contains("map value entry count 2 exceeds capacity 1 for Map<Phase,Phase,1>"),
        "unexpected error: {err}"
    );
}
