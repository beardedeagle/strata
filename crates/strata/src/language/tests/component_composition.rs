use super::super::{
    SourceProgram, SourceUnit, SourceUnitId, check_source_program, lower_to_artifact, parse_source,
};
use mantle_artifact::{MAX_COMPONENT_INSTANCE_COUNT, MAX_PORT_BINDING_COUNT, MAX_PORT_COUNT};

#[test]
fn parser_accepts_component_imports_and_composition() {
    let module = parse_source(root_source()).expect("component composition should parse");

    assert_eq!(module.components.len(), 1);
    assert_eq!(module.components[0].imports.len(), 1);
    assert_eq!(module.components[0].imports[0].as_str(), "WorkerPort");
    assert_eq!(module.compositions.len(), 1);
    assert_eq!(module.compositions[0].instances.len(), 2);
    assert_eq!(module.compositions[0].port_bindings.len(), 1);
}

#[test]
fn parser_rejects_malformed_composition_item() {
    let err = parse_source(&root_source().replace(
        "    bind main imports WorkerPort -> worker exports WorkerPort;",
        "    wire main imports WorkerPort -> worker exports WorkerPort;",
    ))
    .expect_err("malformed composition item should fail parsing");

    assert!(
        err.to_string()
            .contains("expected component instance or port binding"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_builds_typed_composition_graph_and_lowers_metadata() {
    let program = composition_program().expect("component composition source should parse");
    let source_hash_input = program.source_hash_input();
    let checked = check_source_program(program).expect("component composition should check");
    let artifact =
        lower_to_artifact(&checked, &source_hash_input).expect("composition should lower");

    assert_eq!(checked.compositions().len(), 1);
    assert_eq!(checked.components().len(), 2);
    assert_eq!(artifact.compositions.len(), 1);
    assert_eq!(artifact.compositions[0].debug_name, "AppComposition");
    assert_eq!(artifact.compositions[0].component_instances.len(), 2);
    assert_eq!(artifact.compositions[0].port_bindings.len(), 1);
    assert_eq!(artifact.components[1].import_ports.len(), 1);
    assert_eq!(
        artifact.compositions[0].port_bindings[0].importer.as_u32(),
        1
    );
    assert_eq!(
        artifact.compositions[0].port_bindings[0].exporter.as_u32(),
        0
    );
}

#[test]
fn checker_rejects_unbound_component_import() {
    let err = source_program([
        root_source().replace(
            "    bind main imports WorkerPort -> worker exports WorkerPort;\n",
            "",
        ),
        worker_source().to_string(),
    ])
    .and_then(check_source_program)
    .expect_err("unbound component import should fail closed");

    assert!(
        err.to_string()
            .contains("composition AppComposition instance main component MainComponent import port WorkerPort is not bound"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_rejects_composition_without_process_port_authority() {
    let err = source_program([
        root_source().replace(
            "    authority connect_worker: Cap<PortConnect<WorkerPort>>;\n",
            "",
        ),
        worker_source().to_string(),
    ])
    .and_then(check_source_program)
    .expect_err("composition binding must not grant process port authority");

    assert!(
        err.to_string()
            .contains("send via port WorkerPort requires authority Cap<PortConnect<WorkerPort>>"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_rejects_duplicate_import_binding() {
    let err = source_program([root_source().replace(
        "    bind main imports WorkerPort -> worker exports WorkerPort;\n",
        "    bind main imports WorkerPort -> worker exports WorkerPort;\n    bind main imports WorkerPort -> worker exports WorkerPort;\n",
    ), worker_source().to_string()])
    .and_then(check_source_program)
    .expect_err("duplicate component import binding should fail closed");

    assert!(
        err.to_string().contains(
            "composition AppComposition binds instance main imported port WorkerPort more than once"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_rejects_duplicate_component_instance() {
    let err = source_program([
        root_source().replace(
            "    instance worker component WorkerComponent;\n",
            "    instance worker component WorkerComponent;\n    instance worker component WorkerComponent;\n",
        ),
        worker_source().to_string(),
    ])
    .and_then(check_source_program)
    .expect_err("duplicate component instance should fail closed");

    assert!(
        err.to_string().contains(
            "composition AppComposition declares component instance worker more than once"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_rejects_duplicate_composition_declaration() {
    let err = source_program([
        root_source().replace(
            "\n\nproc Main",
            "\n\ncomposition AppComposition {\n    instance worker component WorkerComponent;\n}\n\nproc Main",
        ),
        worker_source().to_string(),
    ])
    .and_then(check_source_program)
    .expect_err("duplicate composition declaration should fail closed");

    assert!(
        err.to_string()
            .contains("duplicate composition declaration AppComposition"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_rejects_component_self_import_and_duplicate_imports() {
    let cases = [
        (
            root_source().replace(
                "component MainComponent exports MainPort imports WorkerPort",
                "component MainComponent exports MainPort imports MainPort",
            ),
            "component MainComponent cannot import its exported port MainPort",
        ),
        (
            root_source().replace(
                "component MainComponent exports MainPort imports WorkerPort",
                "component MainComponent exports MainPort imports WorkerPort, WorkerPort",
            ),
            "component MainComponent imports port WorkerPort more than once",
        ),
    ];

    for (source, expected) in cases {
        let err = source_program([source, worker_source().to_string()])
            .and_then(check_source_program)
            .expect_err("invalid component imports should fail closed");

        assert!(
            err.to_string().contains(expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

#[test]
fn checker_rejects_bad_composition_bindings() {
    let cases = [
        (
            root_source().replace(
                "bind main imports WorkerPort -> worker exports WorkerPort",
                "bind main imports WorkerPort -> main exports MainPort",
            ),
            "composition AppComposition cannot bind instance main to itself",
        ),
        (
            root_source().replace("worker exports WorkerPort", "missing exports WorkerPort"),
            "composition AppComposition references unknown component instance missing",
        ),
        (
            root_source().replace(
                "bind main imports WorkerPort -> worker exports WorkerPort",
                "bind worker imports WorkerPort -> main exports MainPort",
            ),
            "composition AppComposition instance worker component WorkerComponent does not import port WorkerPort",
        ),
        (
            root_source().replace("worker exports WorkerPort", "worker exports MainPort"),
            "composition AppComposition instance worker component WorkerComponent does not export port MainPort",
        ),
    ];

    for (source, expected) in cases {
        let err = source_program([source, worker_source().to_string()])
            .and_then(check_source_program)
            .expect_err("bad composition binding should fail closed");

        assert!(
            err.to_string().contains(expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

#[test]
fn checker_rejects_composition_counts_above_bounds() {
    let too_many_bindings = (0..=MAX_PORT_BINDING_COUNT)
        .map(|_| "    bind main imports WorkerPort -> worker exports WorkerPort;")
        .collect::<Vec<_>>()
        .join("\n");
    let too_many_instances = (0..=MAX_COMPONENT_INSTANCE_COUNT)
        .map(|index| format!("    instance main{index} component MainComponent;"))
        .collect::<Vec<_>>()
        .join("\n");
    let cases = [
        (
            root_source().replace(
                "    bind main imports WorkerPort -> worker exports WorkerPort;",
                &too_many_bindings,
            ),
            format!("port_binding_count must be no greater than {MAX_PORT_BINDING_COUNT}"),
        ),
        (
            root_source()
                .replace(
                    "    instance worker component WorkerComponent;\n    instance main component MainComponent;",
                    &too_many_instances,
                )
                .replace(
                    "    bind main imports WorkerPort -> worker exports WorkerPort;\n",
                    "",
                ),
            format!(
                "component_instance_count must be no greater than {MAX_COMPONENT_INSTANCE_COUNT}"
            ),
        ),
    ];

    for (source, expected) in cases {
        let err = source_program([source, worker_source().to_string()])
            .and_then(check_source_program)
            .expect_err("oversized composition should fail closed");

        assert!(
            err.to_string().contains(&expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

#[test]
fn checker_rejects_component_import_count_before_import_resolution() {
    let imports = (0..=MAX_PORT_COUNT)
        .map(|index| format!("MissingPort{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"module too_many_component_imports;

record MainState;
enum MainMsg {{ Start }}
protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
component MainComponent exports MainPort imports {imports} requires Cap<ComponentExport<MainComponent>>;

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState; }}
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    );
    let err = source_program([source])
        .and_then(check_source_program)
        .expect_err("oversized component imports should fail before resolving missing ports");

    assert!(
        err.to_string().contains(&format!(
            "component_import_count must be no greater than {MAX_PORT_COUNT}"
        )),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn checker_rejects_protocol_mismatch() {
    let root = root_source().replace(
        "    bind main imports WorkerPort -> worker exports WorkerPort;",
        "    instance other component OtherComponent;\n    bind main imports WorkerPort -> other exports OtherPort;",
    );
    let worker = worker_source().replace(
        "component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;",
        "protocol OtherProtocol message WorkerMsg requires Cap<ProtocolBoundary<OtherProtocol>>;\nport OtherPort protocol OtherProtocol target Worker requires Cap<PortConnect<OtherPort>>;\ncomponent WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;\ncomponent OtherComponent exports OtherPort requires Cap<ComponentExport<OtherComponent>>;",
    );
    let err = source_program([root, worker])
        .and_then(check_source_program)
        .expect_err("protocol mismatch should fail closed");

    assert!(
        err.to_string().contains(
            "composition AppComposition cannot bind imported port WorkerPort to exported port OtherPort because their protocols differ"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn source_program_rejects_ambiguous_direct_component_import() {
    let root = r#"module composition_root;
import composition_a;
import composition_b;

record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let a = r#"module composition_a;
record AState;
enum AMsg { Ping }
protocol AProtocol message AMsg requires Cap<ProtocolBoundary<AProtocol>>;
port APort protocol AProtocol target A requires Cap<PortConnect<APort>>;
component SharedComponent exports APort requires Cap<ComponentExport<SharedComponent>>;
proc A mailbox bounded(1) {
    type State = AState;
    type Msg = AMsg;
    fn init() -> AState ! [] ~ [] @det { return AState; }
    fn step(state: AState, Ping) -> ProcResult<AState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let b = r#"module composition_b;
record BState;
enum BMsg { PingB }
protocol BProtocol message BMsg requires Cap<ProtocolBoundary<BProtocol>>;
port BPort protocol BProtocol target B requires Cap<PortConnect<BPort>>;
component SharedComponent exports BPort requires Cap<ComponentExport<SharedComponent>>;
proc B mailbox bounded(1) {
    type State = BState;
    type Msg = BMsg;
    fn init() -> BState ! [] ~ [] @det { return BState; }
    fn step(state: BState, PingB) -> ProcResult<BState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = source_program([root.to_string(), a.to_string(), b.to_string()])
        .expect_err("ambiguous direct component imports should fail closed");

    assert!(
        err.to_string().contains(
            "ambiguous imported component name SharedComponent declared by modules composition_a and composition_b"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn source_program_rejects_transitive_only_component_composition_access() {
    let root = r#"module composition_root;
import composition_api;

record MainState;
enum MainMsg { Start }
protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
component MainComponent exports MainPort imports HiddenPort requires Cap<ComponentExport<MainComponent>>;
composition AppComposition {
    instance main component MainComponent;
    instance hidden component HiddenComponent;
    bind main imports HiddenPort -> hidden exports HiddenPort;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let api = r#"module composition_api;
import composition_hidden;
record ApiMarker;
"#;
    let hidden = r#"module composition_hidden;
record HiddenState;
enum HiddenMsg { Ping }
protocol HiddenProtocol message HiddenMsg requires Cap<ProtocolBoundary<HiddenProtocol>>;
port HiddenPort protocol HiddenProtocol target Hidden requires Cap<PortConnect<HiddenPort>>;
component HiddenComponent exports HiddenPort requires Cap<ComponentExport<HiddenComponent>>;
proc Hidden mailbox bounded(1) {
    type State = HiddenState;
    type Msg = HiddenMsg;
    fn init() -> HiddenState ! [] ~ [] @det { return HiddenState; }
    fn step(state: HiddenState, Ping) -> ProcResult<HiddenState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = source_program([root.to_string(), api.to_string(), hidden.to_string()])
        .expect_err("transitive-only component and port access should fail closed");

    assert!(
        err.to_string()
            .contains("source unit composition_root references port HiddenPort from module composition_hidden without importing composition_hidden"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn bounded_composition_graphs_lower_deterministically() {
    let source = root_source();
    let first_program = source_program([source.to_string(), worker_source().to_string()])
        .expect("first composition program should parse");
    let first_source_hash_input = first_program.source_hash_input();
    let first = lower_to_artifact(
        &check_source_program(first_program).expect("first composition program should check"),
        &first_source_hash_input,
    )
    .expect("first composition program should lower");

    let second_program =
        source_program_with_root(1, [worker_source().to_string(), source.to_string()])
            .expect("reordered composition program should parse");
    let second_source_hash_input = second_program.source_hash_input();
    let second = lower_to_artifact(
        &check_source_program(second_program).expect("reordered composition program should check"),
        &second_source_hash_input,
    )
    .expect("reordered composition program should lower");

    assert_eq!(first.components, second.components);
    assert_eq!(first.compositions, second.compositions);
}

fn composition_program() -> crate::language::Result<SourceProgram> {
    source_program([root_source().to_string(), worker_source().to_string()])
}

fn source_program<const N: usize>(sources: [String; N]) -> crate::language::Result<SourceProgram> {
    source_program_with_root(0, sources)
}

fn source_program_with_root<const N: usize>(
    root: usize,
    sources: [String; N],
) -> crate::language::Result<SourceProgram> {
    let units = sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| SourceUnit::parse(SourceUnitId::from_index(index)?, source))
        .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(root)?, units)
}

fn root_source() -> &'static str {
    r#"module component_composition_main;
import component_composition_worker;

record MainState;
enum MainMsg { Start }
protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
component MainComponent exports MainPort imports WorkerPort requires Cap<ComponentExport<MainComponent>>;
composition AppComposition {
    instance worker component WorkerComponent;
    instance main component MainComponent;
    bind main imports WorkerPort -> worker exports WorkerPort;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority connect_worker: Cap<PortConnect<WorkerPort>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker via WorkerPort Work;
        return Stop(state);
    }
}
"#
}

fn worker_source() -> &'static str {
    r#"module component_composition_worker;

record WorkerState;
enum WorkerMsg { Work }
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
}
