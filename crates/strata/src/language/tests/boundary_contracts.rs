use mantle_artifact::{ArtifactAction, PortId};

use super::super::{
    FunctionBody, SourceProgram, SourceUnit, SourceUnitId, Statement, check_source_program,
    lower_to_artifact, parse_source,
};
use super::support::check_source;

#[test]
fn parser_accepts_protocol_port_component_and_send_via() {
    let module = parse_source(root_source()).expect("boundary declarations should parse");

    assert_eq!(module.protocols.len(), 1);
    assert_eq!(module.ports.len(), 1);
    assert_eq!(module.components.len(), 1);
    let main = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Main")
        .expect("Main process should parse");
    let Some(FunctionBody::Block(body)) = main.steps[0].body.as_ref() else {
        panic!("Main step should have a block body");
    };
    let Statement::Send { port, .. } = &body.statements[1] else {
        panic!("Main step second statement should be send");
    };
    assert_eq!(port.as_ref().map(|port| port.as_str()), Some("WorkerPort"));
}

#[test]
fn checker_builds_typed_boundary_ids_and_lowering_tables() {
    let program = boundary_program(root_source()).expect("boundary source program should parse");
    let source_hash_input = program.source_hash_input();
    let checked = check_source_program(program).expect("boundary source program should check");
    let artifact =
        lower_to_artifact(&checked, &source_hash_input).expect("boundary program should lower");

    assert_eq!(checked.protocols().len(), 1);
    assert_eq!(checked.ports().len(), 1);
    assert_eq!(checked.components().len(), 1);
    assert_eq!(artifact.protocols[0].debug_name, "WorkerProtocol");
    assert_eq!(artifact.ports[0].debug_name, "WorkerPort");
    assert_eq!(artifact.components[0].debug_name, "WorkerComponent");

    let main = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Main")
        .expect("Main artifact process should exist");
    assert!(main.transitions[0].actions.iter().any(|action| matches!(
        action,
        ArtifactAction::Send {
            port: Some(port),
            ..
        } if *port == PortId::new(0)
    )));
}

#[test]
fn checker_rejects_undeclared_send_port() {
    let err = boundary_program(&root_source().replace("via WorkerPort", "via MissingPort"))
        .and_then(check_source_program)
        .expect_err("undeclared send port should fail closed");

    assert!(err.to_string().contains("port MissingPort is not declared"));
}

#[test]
fn checker_rejects_send_port_without_process_authority() {
    let err = boundary_program(&root_source().replace(
        "    authority connect_worker: Cap<PortConnect<WorkerPort>>;\n",
        "",
    ))
    .and_then(check_source_program)
    .expect_err("send port without process authority should fail closed");

    assert!(
        err.to_string()
            .contains("send via port WorkerPort requires authority Cap<PortConnect<WorkerPort>>")
    );
}

#[test]
fn checker_rejects_wrong_port_target() {
    let err = boundary_program(&root_source().replace("target Worker", "target Main"))
        .and_then(check_source_program)
        .expect_err("wrong target should fail closed");

    assert!(
        err.to_string()
            .contains("port WorkerPort targets process Main with message type MainMsg")
    );
}

#[test]
fn checker_rejects_duplicate_boundary_identity() {
    let err = boundary_program(&root_source().replace(
        "component WorkerComponent exports WorkerPort",
        "component WorkerPort exports WorkerPort",
    ))
    .and_then(check_source_program)
    .expect_err("duplicate boundary names should fail closed");

    assert!(
        err.to_string()
            .contains("duplicate boundary declaration name WorkerPort")
    );
}

#[test]
fn checker_rejects_reserved_boundary_descriptor_type_names() {
    for reserved in ["ProtocolBoundary", "PortConnect", "ComponentExport"] {
        let record_source =
            root_source().replace("record MainState;", &format!("record {reserved};"));
        let record_err = boundary_program(&record_source)
            .and_then(check_source_program)
            .expect_err("reserved boundary descriptor record name should fail");
        assert!(
            record_err
                .to_string()
                .contains(&format!("type name {reserved} is reserved")),
            "unexpected diagnostic: {record_err}"
        );

        let enum_source = root_source().replace(
            "enum MainMsg { Start }",
            &format!("enum {reserved} {{ Start }}"),
        );
        let enum_err = boundary_program(&enum_source)
            .and_then(check_source_program)
            .expect_err("reserved boundary descriptor enum name should fail");
        assert!(
            enum_err
                .to_string()
                .contains(&format!("type name {reserved} is reserved")),
            "unexpected diagnostic: {enum_err}"
        );
    }
}

#[test]
fn checker_rejects_return_match_arm_send_port_without_process_authority() {
    let err = check_source(return_match_arm_port_source())
        .expect_err("return-match port send without process authority should fail closed");

    assert!(
        err.to_string()
            .contains("send via port SinkPort requires authority Cap<PortConnect<SinkPort>>"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn source_program_rejects_ambiguous_imported_boundary_names() {
    let root = r#"module boundary_root;
import boundary_left;
import boundary_right;
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
    let left = r#"module boundary_left;
record LeftMsgState;
enum LeftProtocolMsg { Left }
protocol SharedProtocol message LeftProtocolMsg requires Cap<ProtocolBoundary<SharedProtocol>>;
"#;
    let right = r#"module boundary_right;
record RightMsgState;
enum RightProtocolMsg { Right }
protocol SharedProtocol message RightProtocolMsg requires Cap<ProtocolBoundary<SharedProtocol>>;
"#;

    let err = source_program([root, left, right])
        .expect_err("ambiguous imported protocol names should fail closed");

    let err = err.to_string();
    assert!(
        err.contains("ambiguous imported protocol name SharedProtocol")
            || err.contains("duplicate protocol name SharedProtocol"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn bounded_boundary_sets_lower_deterministically() {
    for count in 1..=4 {
        let root = generated_boundary_root(count);
        let program = boundary_program(&root).expect("generated boundary program should parse");
        let source_hash_input = program.source_hash_input();
        let first = lower_to_artifact(
            &check_source_program(program).expect("generated boundary program should check"),
            &source_hash_input,
        )
        .expect("generated boundary program should lower");

        let program = boundary_program(&root).expect("generated boundary program should reparse");
        let second = lower_to_artifact(
            &check_source_program(program).expect("generated boundary program should recheck"),
            &source_hash_input,
        )
        .expect("generated boundary program should relower");

        assert_eq!(first.protocols, second.protocols);
        assert_eq!(first.ports, second.ports);
        assert_eq!(first.components, second.components);
        assert_eq!(first.protocols.len(), count);
        assert_eq!(first.ports.len(), count);
        assert_eq!(first.components.len(), count);
    }
}

fn boundary_program(root: &str) -> crate::language::Result<SourceProgram> {
    source_program([root, worker_source()])
}

fn source_program<const N: usize>(sources: [&str; N]) -> crate::language::Result<SourceProgram> {
    let units = sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            SourceUnit::parse(SourceUnitId::from_index(index)?, source.to_string())
        })
        .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(0)?, units)
}

fn generated_boundary_root(count: usize) -> String {
    let declarations = (0..count)
        .map(|index| {
            format!(
                "protocol WorkerProtocol{index} message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol{index}>>;\nport WorkerPort{index} protocol WorkerProtocol{index} target Worker requires Cap<PortConnect<WorkerPort{index}>>;\ncomponent WorkerComponent{index} exports WorkerPort{index} requires Cap<ComponentExport<WorkerComponent{index}>>;\n"
            )
        })
        .collect::<String>();
    format!(
        r#"module boundary_contracts_main;
import boundary_contracts_worker;

{declarations}
record MainState;
enum MainMsg {{ Start }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority connect_worker: Cap<PortConnect<WorkerPort0>>;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker via WorkerPort0 Work;
        return Stop(state);
    }}
}}
"#
    )
}

fn root_source() -> &'static str {
    r#"module boundary_contracts_main;
import boundary_contracts_worker;

protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;

record MainState;
enum MainMsg { Start }

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

fn return_match_arm_port_source() -> &'static str {
    r#"module boundary_contracts_main;

protocol SinkProtocol message SinkMsg requires Cap<ProtocolBoundary<SinkProtocol>>;
port SinkPort protocol SinkProtocol target Sink requires Cap<PortConnect<SinkPort>>;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
record WorkerState;
enum WorkerMsg { Envelope(Route) }
record SinkState;
enum SinkMsg { SinkDone }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    authority spawn_sink: Cap<Spawn<Sink>>;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {
            Ready => {
                send sink via SinkPort SinkDone;
                return Stop(state);
            }
            Done => {
                return Stop(state);
            }
        };
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, SinkDone) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
}

fn worker_source() -> &'static str {
    r#"module boundary_contracts_worker;

record WorkerState;
enum WorkerMsg { Work }

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
