use super::super::support::*;

#[test]
fn source_functions_bind_nested_patterns_in_signature_body_and_return_match() {
    let source = r#"
module nested_function_patterns;

record MainState {
    signature: Phase,
    body: Phase,
    ret: Phase,
    list: Phase,
    fieldless_signature: Phase,
    fieldless_body: Phase,
    fieldless_ret: Phase,
}
record Job { phase: Phase }
enum Phase { Ready, Done }
enum Routed { Assign(Job) }
enum RoutedKind { Mark(Phase) }
enum MainMsg { Start }

fn phase_signature(Assign(Job { phase })) -> Phase ! [] ~ [] @det {
    return phase;
}

fn phase_body(route: Routed) -> Phase ! [] ~ [] @det {
    match route {
        Assign(Job { phase }) => {
            return phase;
        }
    }
}

fn phase_return(route: Routed) -> Phase ! [] ~ [] @det {
    return match route {
        Assign(Job { phase }) => {
            return phase;
        }
    };
}

fn phase_list(List<Routed,1>[Assign(Job { phase })]) -> Phase ! [] ~ [] @det {
    return phase;
}

fn fieldless_signature(Mark(Ready)) -> Phase ! [] ~ [] @det {
    return Ready;
}

fn fieldless_body(route: RoutedKind) -> Phase ! [] ~ [] @det {
    match route {
        Mark(Ready) => {
            return Ready;
        }
    }
}

fn fieldless_return(route: RoutedKind) -> Phase ! [] ~ [] @det {
    return match route {
        Mark(Ready) => {
            return Ready;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            signature: phase_signature(Assign(Job { phase: Ready })),
            body: phase_body(Assign(Job { phase: Done })),
            ret: phase_return(Assign(Job { phase: Ready })),
            list: phase_list(List<Routed,1>[Assign(Job { phase: Done })]),
            fieldless_signature: fieldless_signature(Mark(Ready)),
            fieldless_body: fieldless_body(Mark(Ready)),
            fieldless_ret: fieldless_return(Mark(Ready)),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source).expect("nested source function patterns should check");
}

#[test]
fn source_functions_dispatch_same_constructor_by_disjoint_fieldless_nested_predicates() {
    let source = r#"
module payload_sensitive_function_dispatch;

record MainState {
    body_ready: Phase,
    body_done: Phase,
    body_fallback: Phase,
    return_ready: Phase,
    return_done: Phase,
    return_fallback: Phase,
    bound_assign: Phase,
    bound_hold: Phase,
}
record Job { phase: Phase }
enum Phase { Ready, Done, Other }
enum Routed {
    Assign(Phase),
    AssignJob(Job),
    Hold(List<Job,2>),
}
enum Packet { Envelope(Routed) }
enum MainMsg { Start }

fn route_body(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    }
}

fn route_return(packet: Packet) -> Phase ! [] ~ [] @det {
    return match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    };
}

fn route_body_with_fallback(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        _ => {
            return Other;
        }
    }
}

fn route_return_with_fallback(packet: Packet) -> Phase ! [] ~ [] @det {
    return match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        _ => {
            return Other;
        }
    };
}

fn route_bound(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(AssignJob(Job { phase })) => {
            return phase;
        }
        Envelope(Hold(List[Job { phase }, ..tail])) => {
            return phase;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            body_ready: route_body(Envelope(Assign(Ready))),
            body_done: route_body(Envelope(Assign(Done))),
            body_fallback: route_body_with_fallback(Envelope(Assign(Done))),
            return_ready: route_return(Envelope(Assign(Ready))),
            return_done: route_return(Envelope(Assign(Done))),
            return_fallback: route_return_with_fallback(Envelope(Assign(Done))),
            bound_assign: route_bound(Envelope(AssignJob(Job { phase: Ready }))),
            bound_hold: route_bound(Envelope(Hold(List<Job,2>[
                Job { phase: Done },
                Job { phase: Other },
            ]))),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("payload-sensitive function dispatch should check");
    lower_to_artifact(&checked, source).expect("payload-sensitive function dispatch should lower");
}
