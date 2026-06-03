use crate::language::{
    AuthoritySummaryFormat, CompositionAdmissionReport, CompositionAdmissionReportFormat,
    SourceProgram, SourceUnit, SourceUnitId, render_authority_summary,
    render_composition_admission_report,
};

#[test]
fn text_report_includes_checked_composition_authority_edges() {
    let report_input = checked_composition();
    let report = render_composition_admission_report(
        &report_input,
        "component_composition_main.str",
        CompositionAdmissionReportFormat::Text,
    );

    assert!(report.contains("format: strata.checked_component_composition_admission_report"));
    assert!(report.contains("source_hash_algorithm: fnv1a64-diagnostic"));
    assert!(report.contains("composition 0 AppComposition"));
    assert!(report.contains("admission_result: admitted"));
    assert!(report.contains("unsatisfied_imports: []"));
    assert!(report.contains("instance 0 main component=1 MainComponent"));
    assert!(report.contains("binding 0 importer=0 main imported_port=0 WorkerPort exporter=1 worker exported_port=0 WorkerPort protocol=0 WorkerProtocol binding_result=admitted imported_port_authority=Cap<PortConnect<WorkerPort>> exported_port_authority=Cap<PortConnect<WorkerPort>>"));
    assert!(report.contains("authority_edge 0 exporter_component=0 WorkerComponent -> importer_component=1 MainComponent exported_port=0 WorkerPort imported_port=0 WorkerPort protocol=0 WorkerProtocol"));
    assert!(report.contains("imported_port_authority=Cap<PortConnect<WorkerPort>>"));
}

#[test]
fn json_report_matches_checked_schema_facts() {
    let report_input = checked_composition();
    let report = render_composition_admission_report(
        &report_input,
        "component_composition_main.str",
        CompositionAdmissionReportFormat::Json,
    );
    let expected = [
        r#"{"report_format":"strata.checked_component_composition_admission_report","report_version":1,"source_language":"strata","source":"component_composition_main.str","module":"component_composition_main","source_hash_fnv1a64":""#,
        report_input.source_hash().fnv1a64(),
        r#"","source_hash_algorithm":"fnv1a64-diagnostic","compositions":[{"composition_id":0,"composition":"AppComposition","admission_result":"admitted","unsatisfied_imports":[],"component_instances":[{"component_instance_id":0,"instance":"main","component_id":1,"component":"MainComponent","component_authority":{"kind":"component_export","component_id":1,"component":"MainComponent"}},{"component_instance_id":1,"instance":"worker","component_id":0,"component":"WorkerComponent","component_authority":{"kind":"component_export","component_id":0,"component":"WorkerComponent"}}],"port_bindings":[{"port_binding_id":0,"importer_instance_id":0,"importer_instance":"main","imported_port_id":0,"imported_port":"WorkerPort","exporter_instance_id":1,"exporter_instance":"worker","exported_port_id":0,"exported_port":"WorkerPort","protocol_id":0,"protocol":"WorkerProtocol","binding_result":"admitted","imported_port_authority":{"kind":"port_connect","port_id":0,"port":"WorkerPort"},"exported_port_authority":{"kind":"port_connect","port_id":0,"port":"WorkerPort"}}],"authority_edges":[{"port_binding_id":0,"edge_kind":"port_binding","exporter_component_id":0,"exporter_component":"WorkerComponent","importer_component_id":1,"importer_component":"MainComponent","exported_port_id":0,"exported_port":"WorkerPort","imported_port_id":0,"imported_port":"WorkerPort","protocol_id":0,"protocol":"WorkerProtocol","export_authority":{"kind":"component_export","component_id":0,"component":"WorkerComponent"},"exported_port_authority":{"kind":"port_connect","port_id":0,"port":"WorkerPort"},"imported_port_authority":{"kind":"port_connect","port_id":0,"port":"WorkerPort"}}]}]}"#,
    ]
    .concat();

    assert_eq!(report, expected);
}

#[test]
fn json_report_escapes_metadata_fields() {
    let report_input = checked_composition();
    let report = render_composition_admission_report(
        &report_input,
        "component \"composition\"\nmain.str",
        CompositionAdmissionReportFormat::Json,
    );

    assert!(report.contains(r#""source":"component \"composition\"\nmain.str""#));
}

#[test]
fn text_report_escapes_metadata_fields() {
    let report_input = checked_composition();
    let report = render_composition_admission_report(
        &report_input,
        "component\\composition\nmain.str",
        CompositionAdmissionReportFormat::Text,
    );

    assert!(
        report.starts_with(
            "strata composition admission report component\\\\composition\\nmain.str\n"
        )
    );
}

#[test]
fn text_report_escapes_control_metadata_with_unicode_scalars() {
    let report_input = checked_composition();
    let report = render_composition_admission_report(
        &report_input,
        "component\u{0008}\u{000c}\u{001f}main.str",
        CompositionAdmissionReportFormat::Text,
    );

    assert!(report.starts_with(
        "strata composition admission report component\\u0008\\u000c\\u001fmain.str\n"
    ));
}

#[test]
fn report_rejects_distinct_bound_port_authorities() {
    let program = SourceProgram::new(
        SourceUnitId::from_index(0).expect("root id"),
        vec![
            SourceUnit::parse(
                SourceUnitId::from_index(0).expect("root id"),
                distinct_root_source().to_string(),
            )
            .expect("root source should parse"),
            SourceUnit::parse(
                SourceUnitId::from_index(1).expect("worker id"),
                distinct_worker_source().to_string(),
            )
            .expect("worker source should parse"),
        ],
    )
    .expect("distinct source program should validate");
    let err = match CompositionAdmissionReport::from_source_program(program) {
        Ok(_) => panic!("report input must reject authority-widening port bindings"),
        Err(err) => err,
    };

    assert!(
        err.to_string().contains(
            "composition AppComposition cannot bind imported port WorkerClientPort to exported port WorkerServerPort because their port authorities differ"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn authority_summary_reports_component_boundary_edges() {
    let report_input = checked_composition();
    let summary = render_authority_summary(
        report_input.checked_program(),
        "component_composition_main.str",
        AuthoritySummaryFormat::Json,
    );

    assert!(summary.contains(
        r#""component_authority_edges":[{"composition_id":0,"composition":"AppComposition","port_binding_id":0,"edge_kind":"component_port_binding","exporter_component_id":0,"exporter_component":"WorkerComponent","importer_component_id":1,"importer_component":"MainComponent","exported_port_id":0,"exported_port":"WorkerPort","imported_port_id":0,"imported_port":"WorkerPort","protocol_id":0,"protocol":"WorkerProtocol","export_authority":{"kind":"component_export","component_id":0,"component":"WorkerComponent"},"exported_port_authority":{"kind":"port_connect","port_id":0,"port":"WorkerPort"},"imported_port_authority":{"kind":"port_connect","port_id":0,"port":"WorkerPort"}}]"#
    ));
}

fn checked_composition() -> CompositionAdmissionReport {
    checked_program(
        root_source(),
        worker_source(),
        "component_composition_worker",
    )
}

fn checked_program(root: &str, worker: &str, worker_module: &str) -> CompositionAdmissionReport {
    let program = SourceProgram::new(
        SourceUnitId::from_index(0).expect("root id"),
        vec![
            SourceUnit::parse(
                SourceUnitId::from_index(0).expect("root id"),
                root.to_string(),
            )
            .expect("root source should parse"),
            SourceUnit::parse(
                SourceUnitId::from_index(1).expect("worker id"),
                worker.to_string(),
            )
            .expect("worker source should parse"),
        ],
    )
    .unwrap_or_else(|err| panic!("{worker_module} source program should validate: {err}"));
    CompositionAdmissionReport::from_source_program(program).expect("source program should check")
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
    instance main component MainComponent;
    instance worker component WorkerComponent;
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

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "composed worker handled Work";
        return Stop(state);
    }
}
"#
}

fn distinct_root_source() -> &'static str {
    r#"module distinct_component_composition_main;
import distinct_component_composition_worker;

record MainState;
enum MainMsg { Start }
protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
port WorkerClientPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerClientPort>>;
component MainComponent exports MainPort imports WorkerClientPort requires Cap<ComponentExport<MainComponent>>;
composition AppComposition {
    instance main component MainComponent;
    instance worker component WorkerComponent;
    bind main imports WorkerClientPort -> worker exports WorkerServerPort;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority connect_worker: Cap<PortConnect<WorkerClientPort>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker via WorkerClientPort Work;
        return Stop(state);
    }
}
"#
}

fn distinct_worker_source() -> &'static str {
    r#"module distinct_component_composition_worker;

record WorkerState;
enum WorkerMsg { Work }
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerServerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerServerPort>>;
component WorkerComponent exports WorkerServerPort requires Cap<ComponentExport<WorkerComponent>>;

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "distinct worker handled Work";
        return Stop(state);
    }
}
"#
}
