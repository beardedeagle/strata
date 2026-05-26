use super::super::support::*;

#[test]
fn checks_source_function_list_rest_surfaces() {
    let source = r#"
module source_function_list_rest_patterns;

enum Phase {
    Ready,
    Done,
    Unknown,
}
record MainState {
    signature: List<Phase,1>,
    body: List<Phase,1>,
    ret: List<Phase,1>,
    zero: List<Phase,0>,
    first: Phase,
}
enum MainMsg {
    Start,
}

fn rest_signature(List<Phase,2>[_, ..tail]) -> List<Phase,1> ! [] ~ [] @det {
    return tail;
}

fn rest_body(items: List<Phase,2>) -> List<Phase,1> ! [] ~ [] @det {
    match items {
        List[_, ..tail] => {
            return tail;
        }
        _ => {
            return List<Phase,1>[];
        }
    }
}

fn rest_return(items: List<Phase,2>) -> List<Phase,1> ! [] ~ [] @det {
    return match items {
        List[_, ..tail] => {
            return tail;
        }
        _ => {
            return List<Phase,1>[];
        }
    };
}

fn rest_zero(List<Phase,2>[_, _, ..tail]) -> List<Phase,0> ! [] ~ [] @det {
    return tail;
}

fn first_with_rest(List<Phase,2>[first, ..tail]) -> Phase ! [] ~ [] @det {
    return first;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            signature: rest_signature(List<Phase,2>[Ready, Done]),
            body: rest_body(List<Phase,2>[Ready, Done]),
            ret: rest_return(List<Phase,2>[Ready, Done]),
            zero: rest_zero(List<Phase,2>[Ready, Done]),
            first: first_with_rest(List<Phase,2>[Ready, Done]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("source function list rest patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("list rest functions should lower");

    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{signature:List[Done],body:List[Done],ret:List[Done],zero:List[],first:Ready}"]
    );
}

#[test]
fn lowers_list_rest_payload_and_state_templates() {
    let source = r#"
module list_rest_payload_and_state_templates;

record MainState;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}
enum PayloadMsg {
    ReplaceList(List<Phase,2>),
}
enum StateMsg {
    Complete,
}
enum WorkerState {
    Holding(List<Phase,2>),
    Done(List<Phase,1>),
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let signature: ProcessRef<SignatureWorker> = spawn SignatureWorker;
        let prefix: ProcessRef<PrefixWorker> = spawn PrefixWorker;
        let body: ProcessRef<BodyWorker> = spawn BodyWorker;
        let state_worker: ProcessRef<StateWorker> = spawn StateWorker;
        send signature ReplaceList(List<Phase,2>[Ready, Done]);
        send prefix ReplaceList(List<Phase,2>[Ready, Done]);
        send body ReplaceList(List<Phase,2>[Ready, Done]);
        send state_worker Complete;
        return Stop(state);
    }
}

proc SignatureWorker mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = PayloadMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[];
    }

    fn step(state: List<Phase,1>, ReplaceList(List[_, ..tail])) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(tail);
    }
}

proc PrefixWorker mailbox bounded(1) {
    type State = Phase;
    type Msg = PayloadMsg;

    fn init() -> Phase ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: Phase, ReplaceList(List[first, ..tail])) -> ProcResult<Phase> ! [] ~ [] @det {
        return Stop(first);
    }
}

proc BodyWorker mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = PayloadMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[];
    }

    fn step(state: List<Phase,1>, msg: PayloadMsg) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        match msg {
            ReplaceList(List[_, ..tail]) => {
                return Stop(tail);
            }
        }
    }
}

proc StateWorker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = StateMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(List<Phase,2>[Ready, Done]);
    }

    fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Holding(List[_, ..tail]) => {
                return Stop(Done(tail));
            }
            Done(tail: List<Phase,1>) => {
                return Stop(Done(tail));
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("list rest payload and state patterns should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("list rest payload/state patterns should lower");

    assert_eq!(
        checked_state_labels(
            checked
                .processes()
                .iter()
                .find(|process| process.debug_name().as_str() == "SignatureWorker")
                .expect("SignatureWorker should be checked")
        ),
        ["List[]", "List[Done]"]
    );
    assert_eq!(
        checked_state_labels(
            checked
                .processes()
                .iter()
                .find(|process| process.debug_name().as_str() == "BodyWorker")
                .expect("BodyWorker should be checked")
        ),
        ["List[]", "List[Done]"]
    );

    assert_process_next_state_list_rest(&artifact, "SignatureWorker");
    assert_process_next_state_list_prefix_element(&artifact, "PrefixWorker");
    assert_process_next_state_list_rest(&artifact, "BodyWorker");
    assert_state_worker_done_payload_list_rest(&artifact);
}

#[test]
fn rejects_list_rest_without_prefix() {
    let source = list_rest_error_source("List<Phase,2>[..tail]", "return tail;");
    let err = check_source(&source).expect_err("list rest without prefix should fail");

    assert!(
        err.to_string()
            .contains("list rest pattern must declare at least one prefix element"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_list_rest_wildcard_binding() {
    let source =
        list_rest_error_source("List<Phase,2>[value, .._]", "return List<Phase,1>[value];");
    let err = check_source(&source).expect_err("list rest wildcard should fail");

    assert!(
        err.to_string()
            .contains("list rest binding cannot be a wildcard; bind the suffix with `..name`"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_duplicate_list_rest_binding() {
    let source = list_rest_error_source("List<Phase,2>[tail, ..tail]", "return tail;");
    let err = check_source(&source).expect_err("duplicate list rest binding should fail");

    assert!(
        err.to_string()
            .contains("list pattern binding tail is declared more than once"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_list_rest_binding_conflicting_with_existing_binding() {
    let source = r#"
module list_rest_binding_conflict;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn conflict(tail: List<Phase,2>) -> List<Phase,1> ! [] ~ [] @det {
    return match tail {
        List[_, ..tail] => {
            return tail;
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

    let err = check_source(source).expect_err("list rest binding conflict should fail");

    assert!(
        err.to_string().contains(
            "collection pattern binding tail conflicts with an existing source value binding"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_list_rest_binding_conflicting_with_declared_value() {
    let source = list_rest_error_source("List<Phase,2>[_, ..Ready]", "return List<Phase,1>[];");
    let err = check_source(&source).expect_err("declared-value list rest binding should fail");

    assert!(
        err.to_string()
            .contains("pattern binding Ready conflicts with a declared type or value constructor"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_wrong_inferred_list_rest_capacity() {
    let source = list_rest_error_source("List<Phase,2>[_, ..tail]", "return tail;");
    let source = source.replace(
        "fn rest_value(List<Phase,2>[_, ..tail]) -> List<Phase,1>",
        "fn rest_value(List<Phase,2>[_, ..tail]) -> List<Phase,2>",
    );
    let err = check_source(&source).expect_err("wrong list rest return type should fail");

    assert!(
        err.to_string()
            .contains("value binding tail has type List<Phase,1>, expected List<Phase,2>"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_overlapping_list_rest_dispatch() {
    let source = r#"
module overlapping_list_rest_dispatch;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn pick(List<Phase,2>[_, ..tail]) -> List<Phase,1> ! [] ~ [] @det {
    return tail;
}

fn pick(List<Phase,2>[_, item]) -> List<Phase,1> ! [] ~ [] @det {
    return List<Phase,1>[item];
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

    let err = check_source(source).expect_err("overlapping list rest dispatch should fail");

    assert!(
        err.to_string()
            .contains("declares overlapping collection patterns"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_overlapping_list_rest_function_body_match() {
    let source = r#"
module overlapping_list_rest_body_match;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn pick(items: List<Phase,2>) -> List<Phase,1> ! [] ~ [] @det {
    match items {
        List[_, ..tail] => {
            return tail;
        }
        List[_, item] => {
            return List<Phase,1>[item];
        }
    }
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

    let err = check_source(source).expect_err("overlapping function body match should fail");

    assert!(
        err.to_string()
            .contains("declares overlapping collection patterns"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_malformed_list_rest_syntax() {
    let source = list_rest_error_source(
        "List<Phase,2>[value, ..tail, other]",
        "return List<Phase,1>[value];",
    );
    let err = check_source(&source).expect_err("malformed list rest syntax should fail");

    assert!(
        err.to_string().contains("expected symbol ']'"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_source_function_list_rest_non_concrete_argument() {
    let source = r#"
module list_rest_non_concrete_argument;

enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
    Replace(List<Phase,2>),
}

fn suffix(List<Phase,2>[_, ..tail]) -> List<Phase,1> ! [] ~ [] @det {
    return tail;
}

proc Main mailbox bounded(1) {
    type State = List<Phase,1>;
    type Msg = MainMsg;

    fn init() -> List<Phase,1> ! [] ~ [] @det {
        return List<Phase,1>[Ready];
    }

    fn step(state: List<Phase,1>, Replace(next: List<Phase,2>)) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Continue(suffix(next));
    }

    fn step(state: List<Phase,1>, Start) -> ProcResult<List<Phase,1>> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("non-concrete list rest dispatch should fail");

    assert!(
        err.to_string()
            .contains("function suffix pattern dispatch requires a concrete list value argument"),
        "unexpected error: {err}"
    );
}

fn assert_process_next_state_list_rest(artifact: &MantleArtifact, process_name: &str) {
    let process = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == process_name)
        .unwrap_or_else(|| panic!("{process_name} should lower"));
    assert!(
        matches!(
            &process.transitions[0].next_state,
            mantle_artifact::NextState::Template(ArtifactValueTemplate::ListRest {
                prefix_len: 1,
                ..
            })
        ),
        "{process_name} should lower next state to a list-rest template"
    );
}

fn assert_process_next_state_list_prefix_element(artifact: &MantleArtifact, process_name: &str) {
    let process = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == process_name)
        .unwrap_or_else(|| panic!("{process_name} should lower"));
    assert!(
        matches!(
            &process.transitions[0].next_state,
            mantle_artifact::NextState::Template(ArtifactValueTemplate::ListPrefixElement {
                index: 0,
                prefix_len: 1,
                ..
            })
        ),
        "{process_name} should lower next state to a list-prefix-element template"
    );
}

fn assert_state_worker_done_payload_list_rest(artifact: &MantleArtifact) {
    let process = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "StateWorker")
        .expect("StateWorker should lower");
    let transition = process
        .transitions
        .iter()
        .find(|transition| transition.current_state == Some(mantle_artifact::StateId::new(0)))
        .expect("Holding state transition should lower");
    let mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
        variant,
        payload,
        ..
    }) = &transition.next_state
    else {
        panic!("StateWorker should lower Done(tail) to an enum template");
    };
    assert_eq!(*variant, mantle_artifact::EnumVariantId::new(1));
    assert!(
        matches!(
            payload.as_ref(),
            ArtifactValueTemplate::ListRest { prefix_len: 1, .. }
        ),
        "Done payload should be a list-rest template"
    );
}

fn list_rest_error_source(pattern: &str, returned: &str) -> String {
    format!(
        r#"
module list_rest_error;

enum Phase {{
    Ready,
    Done,
}}
record MainState;
enum MainMsg {{
    Start,
}}

fn rest_value({pattern}) -> List<Phase,1> ! [] ~ [] @det {{
    {returned}
}}

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
"#
    )
}
