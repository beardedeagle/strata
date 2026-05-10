use super::support::*;

fn assert_concrete_collection_value_argument_error(source: &str, context: &str, expected: &str) {
    let err = check_source(source).expect_err("non-concrete collection dispatch should fail");
    let message = err.to_string();

    assert!(
        message.contains(context),
        "expected error context `{context}` in `{message}`"
    );
    assert!(
        message.contains(expected),
        "expected concrete collection diagnostic `{expected}` in `{message}`"
    );
}

#[test]
fn checks_source_function_list_and_map_patterns() {
    let source = r#"
module source_function_collection_patterns;

enum Phase {
    Ready,
    Done,
    Unknown,
}
record MainState {
    first: Phase,
    body: Phase,
    map_body: Phase,
    ret: Phase,
    mapped: Phase,
}
enum MainMsg {
    Start,
}

fn pick(List<Phase,2>[phase, _]) -> Phase ! [] ~ [] @det {
    return phase;
}

fn pick(List<Phase,2>[]) -> Phase ! [] ~ [] @det {
    return Unknown;
}

fn body_pick(items: List<Phase,1>) -> Phase ! [] ~ [] @det {
    match items {
        List[item] => {
            return item;
        }
        _ => {
            return Unknown;
        }
    }
}

fn map_body_pick(items: Map<Phase,Phase,1>) -> Phase ! [] ~ [] @det {
    match items {
        Map[Ready => selected] => {
            return selected;
        }
        _ => {
            return Unknown;
        }
    }
}

fn return_pick(items: List<Phase,2>) -> Phase ! [] ~ [] @det {
    return match items {
        List[_, item] => {
            return item;
        }
        _ => {
            return Unknown;
        }
    };
}

fn ready_value(Map<Phase,Phase,1>[Ready => selected]) -> Phase ! [] ~ [] @det {
    return selected;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            first: pick(List<Phase,2>[Ready, Done]),
            body: body_pick(List<Phase,1>[Done]),
            map_body: map_body_pick(Map<Phase,Phase,1>[Ready => Done]),
            ret: return_pick(List<Phase,2>[Ready, Done]),
            mapped: ready_value(Map<Phase,Phase,1>[Ready => Done]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("collection source helper patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("collection source should lower");

    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{first:Ready,body:Done,map_body:Done,ret:Done,mapped:Done}"]
    );
}

#[test]
fn lowers_payload_dependent_list_state_templates() {
    let source = r#"
module payload_dependent_list_state;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: List<Phase,1>, Replace(next: Phase)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(List<Phase,1>[next]);
    }
}
"#;

    let checked = check_source(source).expect("payload-dependent list state should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("payload-dependent list state should lower");
    let has_list_template = artifact.processes[0].transitions.iter().any(|transition| {
        matches!(
            &transition.next_state,
            mantle_artifact::NextState::Template(ArtifactValueTemplate::List { items, .. })
                if matches!(items.as_slice(), [ArtifactValueTemplate::ReceivedPayload { .. }])
        )
    });

    assert!(
        has_list_template,
        "expected a list next-state template using the received payload"
    );
}

#[test]
fn lowers_payload_dependent_map_state_templates() {
    let source = r#"
module payload_dependent_map_state;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,1>, Start) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: Map<Phase,Phase,1>, Replace(next: Phase)) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Continue(Map<Phase,Phase,1>[Ready => next]);
    }
}
"#;

    let checked = check_source(source).expect("payload-dependent map state should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("payload-dependent map state should lower");
    let has_map_template = artifact.processes[0].transitions.iter().any(|transition| {
        matches!(
            &transition.next_state,
            mantle_artifact::NextState::Template(ArtifactValueTemplate::Map { entries, .. })
                if matches!(
                    entries.as_slice(),
                    [mantle_artifact::ArtifactValueTemplateMapEntry {
                        key: ArtifactValueTemplate::Literal { .. },
                        value: ArtifactValueTemplate::ReceivedPayload { .. },
                    }]
                )
        )
    });

    assert!(
        has_map_template,
        "expected a map next-state template using the received payload"
    );
}

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
fn rejects_duplicate_map_value_keys_in_source_helper() {
    let source = r#"
module duplicate_map_value_keys_in_source_helper;

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
fn rejects_duplicate_static_map_template_keys_with_runtime_value() {
    let source = r#"
module duplicate_static_map_template_keys_with_runtime_value;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,2>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,2> ! [] ~ [] @det {
        return Map<Phase,Phase,2>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,2>, Start) -> ProcResult<Map<Phase,Phase,2>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: Map<Phase,Phase,2>, Replace(next: Phase)) -> ProcResult<Map<Phase,Phase,2>> ! [] ~ [] @det {
        return Continue(Map<Phase,Phase,2>[Ready => next, Ready => Done]);
    }
}
"#;

    let err = check_source(source).expect_err("duplicate static template keys should fail");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,2> duplicates key Ready"),
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

#[test]
fn rejects_runtime_dependent_map_key_state_template() {
    let source = r#"
module runtime_dependent_map_key_state_template;

record Unit;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(Phase),
}

proc Main mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = MainMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,1>, Start) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: Map<Phase,Phase,1>, Replace(next: Phase)) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Continue(Map<Phase,Phase,1>[next => Ready]);
    }
}
"#;

    let err = check_source(source).expect_err("runtime-dependent map keys should fail");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}
