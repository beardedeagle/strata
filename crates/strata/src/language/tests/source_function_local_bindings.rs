use super::support::*;

mod shadowing;

const SOURCE_FUNCTION_LOCAL_BINDINGS: &str = r#"
module source_function_local_bindings;

enum Bool { False, True }
enum Phase { Idle, Active }
record Work { phase: Phase }
record Route {
    selected: Phase,
    flags: List<Bool,2>,
    mapping: Map<Phase,Phase,1>,
}
record MainState {
    selected: Phase,
    echoed: Phase,
}
enum MainMsg { Start }

fn status(Work { phase }) -> Phase ! [] ~ [] @det {
    return phase;
}

fn select_phase(flag: Bool) -> Phase ! [] ~ [] @det {
    if (flag) {
        let selected_if_local: Phase = Active;
        return selected_if_local;
    } else {
        let selected_else_local: Phase = Idle;
        return selected_else_local;
    }
}

fn route(work: Work) -> Route ! [] ~ [] @det {
    let current_local: Phase = status(work);
    let active_flag_local: Bool = current_local == Active;
    let selected_local: Phase = if (active_flag_local) { Active } else { Idle };
    let routed_local: Phase = select_phase(active_flag_local);
    let flags_local: List<Bool,2> = List<Bool,2>[active_flag_local, True];
    let mapping_local: Map<Phase,Phase,1> = Map<Phase,Phase,1>[selected_local => routed_local];
    return Route { selected: routed_local, flags: flags_local, mapping: mapping_local };
}

fn echo_route(route_value: Route) -> Phase ! [] ~ [] @det {
    return match route_value {
        Route { selected: echo_source_local } => {
            let echo_local: Phase = echo_source_local;
            return echo_local;
        }
    };
}

fn phase_from_body_match(route_value: Route) -> Phase ! [] ~ [] @det {
    match route_value {
        Route { selected: body_source_local } => {
            let body_local: Phase = body_source_local;
            return body_local;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn process_route(work: Work) -> Phase ! [] ~ [] @det {
        let route_value_local: Route = route(work);
        let phase_local: Phase = phase_from_body_match(route_value_local);
        return phase_local;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle, echoed: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "immutable source computation selected active";
        return Stop(MainState {
            selected: process_route(Work { phase: Active }),
            echoed: echo_route(route(Work { phase: Active }))
        });
    }
}
"#;

#[test]
fn parses_checks_and_lowers_immutable_source_local_bindings() {
    let module =
        parse_source(SOURCE_FUNCTION_LOCAL_BINDINGS).expect("local binding source should parse");
    let route = module
        .functions
        .iter()
        .find(|function| function.name.as_str() == "route")
        .expect("route function should parse");
    let Some(FunctionBody::Block(body)) = &route.body else {
        panic!("route should parse as a block body");
    };
    assert_eq!(body.statements.len(), 6);
    assert!(matches!(body.statements[0], Statement::LetValue { .. }));

    let checked = check_module(module).expect("local binding source should check");
    let main = &checked.processes()[0];
    assert_eq!(
        checked_state_labels(main),
        [
            "MainState{selected:Idle,echoed:Idle}",
            "MainState{selected:Active,echoed:Active}",
        ]
    );

    let artifact = lower_to_artifact(&checked, SOURCE_FUNCTION_LOCAL_BINDINGS)
        .expect("local binding source should lower");
    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        [
            "MainState{selected:Idle,echoed:Idle}",
            "MainState{selected:Active,echoed:Active}",
        ]
    );
    let encoded = artifact.encode();
    for source_only_name in [
        "current_local",
        "active_flag_local",
        "selected_local",
        "routed_local",
        "flags_local",
        "mapping_local",
        "selected_if_local",
        "selected_else_local",
        "echo_source_local",
        "echo_local",
        "body_source_local",
        "body_local",
        "route_value_local",
        "phase_local",
        "process_route",
        "select_phase",
        "phase_from_body_match",
        "echo_route",
        "status",
    ] {
        assert!(
            !encoded.contains(source_only_name),
            "{source_only_name} must not lower into executable artifact meaning"
        );
    }
}

#[test]
fn lowers_runtime_payload_source_local_binding_as_typed_template() {
    let source = r#"
module source_function_local_binding_templates;

enum Phase { Idle, Active }
record MainState { selected: Phase }
enum MainMsg { Start, Assign(Phase) }

fn route_payload(phase: Phase) -> Phase ! [] ~ [] @det {
    let selected_local: Phase = phase;
    return selected_local;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: MainState, Assign(next: Phase)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(MainState { selected: route_payload(next) });
    }
}
"#;

    let checked = check_source(source).expect("payload local binding source should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("payload local binding source should lower");
    let process = &artifact.processes[0];
    let has_payload_template = process.transitions.iter().any(|transition| {
        let NextState::Template(ArtifactValueTemplate::Record { fields, .. }) =
            &transition.next_state
        else {
            return false;
        };
        fields.iter().any(|field| {
            field.name == "selected"
                && matches!(&field.value, ArtifactValueTemplate::ReceivedPayload { .. })
        })
    });

    assert!(
        has_payload_template,
        "source-local payload binding should lower to a typed received-payload template"
    );
    let encoded = artifact.encode();
    assert!(!encoded.contains("selected_local"));
    assert!(!encoded.contains("route_payload"));
}

#[test]
fn bounded_alias_chains_match_direct_source_function_result() {
    for chain_len in 0..=4 {
        let mut statements = String::new();
        let mut previous = "phase_base".to_string();
        for index in 0..chain_len {
            let name = format!("phase_alias_{index}");
            statements.push_str(&format!("    let {name}: Phase = {previous};\n"));
            previous = name;
        }
        let source = format!(
            r#"
module source_function_local_binding_chain_{chain_len};

enum Bool {{ False, True }}
enum Phase {{ Idle, Active }}
record Work {{ phase: Phase }}
record Route {{
    selected: Phase,
    flags: List<Bool,2>,
    mapping: Map<Phase,Phase,1>,
}}
record MainState {{
    computed: Route,
    direct: Route,
}}
enum MainMsg {{ Start }}

fn status(Work {{ phase }}) -> Phase ! [] ~ [] @det {{
    return phase;
}}

fn computed_route(work: Work) -> Route ! [] ~ [] @det {{
    let phase_base: Phase = status(work);
{statements}    let active_flag_local: Bool = {previous} == Active;
    let selected_local: Phase = if (active_flag_local) {{ Active }} else {{ Idle }};
    let flags_local: List<Bool,2> = List<Bool,2>[active_flag_local, True];
    let mapping_local: Map<Phase,Phase,1> = Map<Phase,Phase,1>[selected_local => {previous}];
    return Route {{ selected: selected_local, flags: flags_local, mapping: mapping_local }};
}}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{
            computed: computed_route(Work {{ phase: Active }}),
            direct: Route {{
                selected: Active,
                flags: List<Bool,2>[True, True],
                mapping: Map<Phase,Phase,1>[Active => Active]
            }}
        }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
        );

        let checked = check_source(&source)
            .unwrap_or_else(|err| panic!("generated alias chain should check: {err}"));
        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("generated alias chain should lower: {err}"));
        let labels = artifact_state_labels(&artifact.processes[0]);
        assert_eq!(labels.len(), 1);
        assert!(labels[0].contains("computed:Route{selected:Active"));
        assert!(labels[0].contains("direct:Route{selected:Active"));
        let encoded = artifact.encode();
        assert!(!encoded.contains("computed_route"));
        assert!(!encoded.contains("status"));
        for source_only_name in [
            "phase_base",
            "active_flag_local",
            "selected_local",
            "flags_local",
            "mapping_local",
        ] {
            assert!(!encoded.contains(source_only_name));
        }
        for index in 0..chain_len {
            assert!(
                !encoded.contains(&format!("phase_alias_{index}")),
                "alias binding name should not lower"
            );
        }
    }
}

#[test]
fn rejects_malformed_source_local_binding_syntax() {
    for replacement in [
        "let current_local Phase = Active;",
        "let current_local: = Active;",
        "let current_local: Phase Active;",
    ] {
        let source = SOURCE_FUNCTION_LOCAL_BINDINGS
            .replace("let current_local: Phase = status(work);", replacement);
        assert!(
            parse_source(&source).is_err(),
            "malformed binding should fail to parse: {replacement}"
        );
    }
}

#[test]
fn rejects_duplicate_source_local_binding() {
    let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(
        "let active_flag_local: Bool = current_local == Active;",
        "let current_local: Phase = Active;",
    );

    let err = check_source(&source).expect_err("duplicate local binding should fail");

    assert!(err.to_string().contains(
        "source-local binding current_local conflicts with an existing source value binding"
    ));
}

#[test]
fn rejects_source_local_binding_type_mismatch_and_unknowns() {
    for (needle, replacement, expected) in [
        (
            "let current_local: Phase = status(work);",
            "let current_local: Missing = status(work);",
            "function route source-local binding current_local must use a declared record, enum, scalar, list, or map type without process-reference authority, found Missing",
        ),
        (
            "let current_local: Phase = status(work);",
            "let current_local: Bool = status(work);",
            "source-local binding current_local value must produce Bool",
        ),
        (
            "let current_local: Phase = status(work);",
            "let current_local: Phase = missing_local;",
            "source-local binding current_local value must produce Phase",
        ),
    ] {
        let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(needle, replacement);
        let err = check_source(&source).expect_err("bad binding should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_process_reference_source_local_binding() {
    let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(
        "let current_local: Phase = status(work);",
        "let worker_local: ProcessRef<Main> = Active;",
    );

    let err = check_source(&source).expect_err("process ref local binding should fail");

    assert!(
        err.to_string().contains(
            "function route source-local binding worker_local must use a declared record, enum, scalar, list, or map type without process-reference authority, found ProcessRef<Main>"
        ),
        "{err}"
    );
}

#[test]
fn rejects_source_local_binding_used_outside_scope() {
    let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(
        "selected: process_route(Work { phase: Active }),",
        "selected: current_local,",
    );

    let err = check_source(&source).expect_err("source-local binding should stay scoped");

    assert!(
        err.to_string()
            .contains("value current_local is not a variant of enum Phase")
    );
}

#[test]
fn rejects_source_local_binding_function_call_cycle() {
    let source = r#"
module source_function_local_binding_cycle;

enum Phase { Idle, Active }
record MainState { selected: Phase }
enum MainMsg { Start }

fn first(phase: Phase) -> Phase ! [] ~ [] @det {
    let routed: Phase = second(phase);
    return routed;
}

fn second(phase: Phase) -> Phase ! [] ~ [] @det {
    return first(phase);
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { selected: first(Active) });
    }
}
"#;

    let err = check_source(source).expect_err("local binding call cycle should fail");

    assert!(
        err.to_string()
            .contains("module source function call cycle first -> second -> first"),
        "{err}"
    );
}

#[test]
fn rejects_runtime_forms_in_source_local_binding_positions() {
    for (needle, replacement, expected) in [
        (
            "let current_local: Phase = status(work);",
            "let worker_local: ProcessRef<Main> = spawn Main;",
            "function route must not perform statements",
        ),
        (
            "let current_local: Phase = status(work);",
            "for item in current_local { emit \"bad\"; }",
            "function route must not perform statements",
        ),
        (
            "let current_local: Phase = status(work);",
            "current_local = Active;",
            "assignment statements are not supported",
        ),
    ] {
        let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(needle, replacement);
        let err = if expected == "assignment statements are not supported" {
            parse_source(&source).expect_err("assignment should fail to parse")
        } else {
            check_source(&source).expect_err("runtime form should fail")
        };
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_source_local_binding_in_runtime_step_body() {
    let source = r#"
module source_local_binding_in_step_body;

enum Phase { Idle, Active }
record MainState { selected: Phase }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        let step_local: Phase = Active;
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("step-local source binding should fail");

    assert!(err.to_string().contains(
        "process Main step source-local value binding step_local is only supported in pure source functions"
    ));
}

#[test]
fn rejects_source_local_binding_in_runtime_if_branch() {
    let source = r#"
module source_local_binding_in_runtime_if_branch;

enum Bool { False, True }
enum Phase { Idle, Active }
record MainState { selected: Phase }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        if (True) {
            let branch_local: Phase = Active;
        }
        return Stop(state);
    }
}
"#;

    let err = parse_source(source).expect_err("branch-local source binding should fail to parse");

    assert!(
        err.to_string()
            .contains("statement-level if branches cannot bind local values or process references")
    );
}
