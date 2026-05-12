use super::super::support::*;

#[test]
fn checks_source_function_map_rest_surfaces() {
    let source = r#"
module source_function_map_rest_patterns;

enum Phase {
    Ready,
    Done,
    Unknown,
}
record MainState {
    signature: Map<Phase,Phase,1>,
    body: Map<Phase,Phase,1>,
    ret: Map<Phase,Phase,1>,
}
enum MainMsg {
    Start,
}

fn rest_signature(Map<Phase,Phase,2>[Ready => _, ..rest]) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    return rest;
}

fn rest_body(items: Map<Phase,Phase,2>) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    match items {
        Map[Ready => _, ..rest] => {
            return rest;
        }
        _ => {
            return Map<Phase,Phase,1>[];
        }
    }
}

fn rest_return(items: Map<Phase,Phase,2>) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    return match items {
        Map[Ready => _, ..rest] => {
            return rest;
        }
        _ => {
            return Map<Phase,Phase,1>[];
        }
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            signature: rest_signature(Map<Phase,Phase,2>[Ready => Done, Done => Ready]),
            body: rest_body(Map<Phase,Phase,2>[Ready => Done, Done => Ready]),
            ret: rest_return(Map<Phase,Phase,2>[Ready => Done, Done => Ready]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("source helper map rest patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("map rest helpers should lower");

    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{signature:Map[Done=>Ready],body:Map[Done=>Ready],ret:Map[Done=>Ready]}"]
    );
}

#[test]
fn lowers_map_rest_payload_and_state_templates() {
    let source = r#"
module map_rest_payload_and_state_templates;

record MainState;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}
enum PayloadMsg {
    ReplaceMap(Map<Phase,Phase,2>),
}
enum StateMsg {
    Complete,
}
enum WorkerState {
    Holding(Map<Phase,Phase,2>),
    Done(Map<Phase,Phase,1>),
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let signature_worker: ProcessRef<SignatureWorker> = spawn SignatureWorker;
        let body_worker: ProcessRef<BodyWorker> = spawn BodyWorker;
        let state_worker: ProcessRef<StateWorker> = spawn StateWorker;
        send signature_worker ReplaceMap(Map<Phase,Phase,2>[Ready => Done, Done => Ready]);
        send body_worker ReplaceMap(Map<Phase,Phase,2>[Ready => Done, Done => Ready]);
        send state_worker Complete;
        return Stop(state);
    }
}

proc SignatureWorker mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = PayloadMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[];
    }

    fn step(state: Map<Phase,Phase,1>, ReplaceMap(Map[Ready => _, ..rest])) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return Stop(rest);
    }
}

proc BodyWorker mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = PayloadMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[];
    }

    fn step(state: Map<Phase,Phase,1>, msg: PayloadMsg) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        match msg {
            ReplaceMap(Map[Ready => _, ..rest]) => {
                return Stop(rest);
            }
        }
    }
}

proc StateWorker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = StateMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Map<Phase,Phase,2>[Ready => Done, Done => Ready]);
    }

    fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Holding(Map[Ready => _, ..rest]) => {
                return Stop(Done(rest));
            }
            Done(rest: Map<Phase,Phase,1>) => {
                return Stop(Done(rest));
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("map rest payload and state patterns should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("map rest payload/state patterns should lower");

    assert_eq!(
        checked_state_labels(
            checked
                .processes()
                .iter()
                .find(|process| process.debug_name().as_str() == "SignatureWorker")
                .expect("SignatureWorker should be checked")
        ),
        ["Map[]", "Map[Done=>Ready]"]
    );
    assert_eq!(
        checked_state_labels(
            checked
                .processes()
                .iter()
                .find(|process| process.debug_name().as_str() == "BodyWorker")
                .expect("BodyWorker should be checked")
        ),
        ["Map[]", "Map[Done=>Ready]"]
    );

    assert_process_next_state_map_rest(&artifact, "SignatureWorker");
    assert_process_next_state_map_rest(&artifact, "BodyWorker");
    assert_state_worker_done_payload_map_rest(&artifact);
}

#[test]
fn rejects_map_rest_without_static_key() {
    let source = map_rest_error_source("Map<Phase,Phase,2>[..rest]", "return rest;");
    let err = check_source(&source).expect_err("map rest without static key should fail");

    assert!(
        err.to_string()
            .contains("map rest pattern must declare at least one key"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_map_rest_wildcard_binding() {
    let source = map_rest_error_source("Map<Phase,Phase,2>[Ready => value, .._]", "return value;");
    let err = check_source(&source).expect_err("map rest wildcard should fail");

    assert!(
        err.to_string()
            .contains("map rest binding cannot be a wildcard; use `..` to ignore the remainder"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_map_rest_binding_conflicting_with_existing_binding() {
    let source = r#"
module map_rest_binding_conflict;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn conflict(rest: Map<Phase,Phase,2>) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    return match rest {
        Map[Ready => _, ..rest] => {
            return rest;
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

    let err = check_source(source).expect_err("map rest binding conflict should fail");

    assert!(
        err.to_string().contains(
            "collection pattern binding rest conflicts with an existing source value binding"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_wrong_inferred_map_rest_capacity() {
    let source = map_rest_error_source("Map<Phase,Phase,2>[Ready => _, ..rest]", "return rest;");
    let source = source.replace(
        "fn rest_value(Map<Phase,Phase,2>[Ready => _, ..rest]) -> Map<Phase,Phase,1>",
        "fn rest_value(Map<Phase,Phase,2>[Ready => _, ..rest]) -> Map<Phase,Phase,2>",
    );
    let err = check_source(&source).expect_err("wrong map rest return type should fail");

    assert!(
        err.to_string().contains(
            "value binding rest has type Map<Phase,Phase,1>, expected Map<Phase,Phase,2>"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_runtime_bound_map_rest_key() {
    let source = r#"
module runtime_bound_map_rest_key;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    ReplaceMap(Map<Phase,Phase,2>),
}

proc Main mailbox bounded(1) {
    type State = Phase;
    type Msg = MainMsg;

    fn init() -> Phase ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: Phase, ReplaceMap(Map[state => _, ..rest])) -> ProcResult<Phase> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let err = check_source(source).expect_err("runtime-bound map rest key should fail");

    assert!(
        err.to_string()
            .contains("value state is not a variant of enum Phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_overlapping_map_rest_dispatch() {
    let source = r#"
module overlapping_map_rest_dispatch;

enum Phase {
    Ready,
    Done,
}
record MainState;
enum MainMsg {
    Start,
}

fn pick(Map<Phase,Phase,2>[Ready => _, ..rest]) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    return rest;
}

fn pick(Map<Phase,Phase,2>[Done => _, ..rest]) -> Map<Phase,Phase,1> ! [] ~ [] @det {
    return rest;
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

    let err = check_source(source).expect_err("overlapping map rest dispatch should fail");

    assert!(
        err.to_string()
            .contains("declares overlapping collection patterns"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_malformed_map_rest_syntax() {
    let source = map_rest_error_source(
        "Map<Phase,Phase,2>[Ready => value, ..rest, Done => other]",
        "return value;",
    );
    let err = check_source(&source).expect_err("malformed map rest syntax should fail");

    assert!(
        err.to_string().contains("expected symbol ']'"),
        "unexpected error: {err}"
    );
}

fn assert_process_next_state_map_rest(artifact: &MantleArtifact, process_name: &str) {
    let process = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == process_name)
        .unwrap_or_else(|| panic!("{process_name} should lower"));
    assert!(
        matches!(
            &process.transitions[0].next_state,
            mantle_artifact::NextState::Template(ArtifactValueTemplate::MapRest {
                excluded_keys,
                ..
            }) if excluded_keys.as_slice() == [artifact_value("Ready")]
        ),
        "{process_name} should lower next state to a map-rest template"
    );
}

fn assert_state_worker_done_payload_map_rest(artifact: &MantleArtifact) {
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
        panic!("StateWorker should lower Done(rest) to an enum template");
    };
    assert_eq!(variant, "Done");
    assert!(
        matches!(
            payload.as_ref(),
            ArtifactValueTemplate::MapRest {
                excluded_keys,
                ..
            } if excluded_keys.as_slice() == [artifact_value("Ready")]
        ),
        "Done payload should be a map-rest template"
    );
}

fn map_rest_error_source(pattern: &str, returned: &str) -> String {
    format!(
        r#"
module map_rest_error;

enum Phase {{
    Ready,
    Done,
}}
record MainState;
enum MainMsg {{
    Start,
}}

fn rest_value({pattern}) -> Map<Phase,Phase,1> ! [] ~ [] @det {{
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
