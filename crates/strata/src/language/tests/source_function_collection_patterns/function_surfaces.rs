use super::super::support::*;

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

    let checked = check_source(source).expect("collection source function patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("collection source should lower");

    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{first:Ready,body:Done,map_body:Done,ret:Done,mapped:Done}"]
    );
}

#[test]
fn checks_source_function_subset_map_patterns() {
    let source = r#"
module source_function_subset_map_patterns;

enum Phase {
    Ready,
    Done,
    Unknown,
}
record MainState {
    signature: Phase,
    body: Phase,
    ret: Phase,
}
enum MainMsg {
    Start,
}

fn ready_signature(Map<Phase,Phase,2>[Ready => selected, ..,]) -> Phase ! [] ~ [] @det {
    return selected;
}

fn ready_body(items: Map<Phase,Phase,2>) -> Phase ! [] ~ [] @det {
    match items {
        Map[Ready => selected, ..,] => {
            return selected;
        }
        _ => {
            return Unknown;
        }
    }
}

fn ready_return(items: Map<Phase,Phase,2>) -> Phase ! [] ~ [] @det {
    return match items {
        Map[Ready => selected, ..,] => {
            return selected;
        }
        _ => {
            return Unknown;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            signature: ready_signature(Map<Phase,Phase,2>[Ready => Done, Unknown => Unknown]),
            body: ready_body(Map<Phase,Phase,2>[Ready => Done, Unknown => Unknown]),
            ret: ready_return(Map<Phase,Phase,2>[Ready => Done, Unknown => Unknown]),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("subset map source function patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("subset map source should lower");

    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{signature:Done,body:Done,ret:Done}"]
    );
}

#[test]
fn lowers_payload_dependent_list_state_templates() {
    let source = r#"
module payload_dependent_list_state;

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
