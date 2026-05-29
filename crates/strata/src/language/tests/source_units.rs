use mantle_artifact::{ArtifactAction, ArtifactCapabilityDescriptor, ProcessId};

use super::super::{
    SourceProgram, SourceUnit, SourceUnitId, check_source, check_source_program, lower_to_artifact,
    parse_source,
};

#[test]
fn parser_accepts_imports_before_declarations() {
    let module = parse_source(
        r#"module root;
import shared;
import worker;

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
"#,
    )
    .expect("imports before declarations should parse");

    assert_eq!(module.imports.len(), 2);
    assert_eq!(module.imports[0].module.as_str(), "shared");
    assert_eq!(module.imports[1].module.as_str(), "worker");
}

#[test]
fn parser_rejects_imports_after_declarations() {
    let err = parse_source(
        r#"module root;
record MainState;
import shared;
"#,
    )
    .expect_err("late import should fail");

    assert!(
        err.to_string()
            .contains("imports must appear before top-level declarations")
    );
}

#[test]
fn parser_rejects_unsupported_import_forms() {
    for source in [
        r#"module root;
import shared as alias;
"#,
        r#"module root;
import "../shared";
"#,
    ] {
        let err = parse_source(source).expect_err("unsupported import form should fail");
        let err = err.to_string();
        assert!(
            err.contains("expected symbol ';'") || err.contains("expected identifier"),
            "unexpected import diagnostic: {err}"
        );
    }
}

#[test]
fn single_source_checker_rejects_imports_without_source_program() {
    let err = check_source(
        r#"module root;
import shared;
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
"#,
    )
    .expect_err("single-source checker should fail closed on imports");

    assert!(
        err.to_string()
            .contains("imports require checking from a root source path")
    );
}

#[test]
fn source_program_checks_lowers_cross_unit_records_functions_and_processes() {
    let program =
        source_program_from_array([root_source(), shared_types_source(), worker_source()])
            .expect("multi-source program should build a source dependency graph");
    let source_hash_input = program.source_hash_input();
    let checked = check_source_program(program).expect("multi-source program should check");
    let artifact =
        lower_to_artifact(&checked, &source_hash_input).expect("multi-source program should lower");

    assert_eq!(checked.module_name(), "imports_main");
    assert_eq!(artifact.module, "imports_main");

    let worker_id = process_id(&artifact, "Worker");
    let main = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Main")
        .expect("Main artifact process should exist");

    assert_eq!(
        main.authorities[0].descriptor,
        ArtifactCapabilityDescriptor::Spawn { target: worker_id }
    );
    assert!(
        main.transitions[0]
            .actions
            .iter()
            .any(|action| matches!(action, ArtifactAction::Send { .. })),
        "Main transition should send to the imported Worker process"
    );
}

#[test]
fn source_program_dependency_order_is_import_order_deterministic() {
    let program =
        source_program_from_array([root_source(), shared_types_source(), worker_source()])
            .expect("multi-source program should build a source dependency graph");
    let ordered_names = program
        .dependency_order()
        .iter()
        .map(|id| program.units()[id.index()].module().name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ordered_names,
        ["imports_types", "imports_worker", "imports_main"]
    );
}

#[test]
fn source_program_rejects_missing_import() {
    let err = source_program_from_array([r#"module imports_main;
import missing;
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
"#])
    .expect_err("missing import should fail");

    assert!(
        err.to_string()
            .contains("source unit imports_main imports missing module missing")
    );
}

#[test]
fn source_program_rejects_duplicate_imports() {
    let err = source_program_from_array([
        r#"module imports_main;
import imports_types;
import imports_types;

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
"#,
        shared_types_source(),
    ])
    .expect_err("duplicate import should fail");

    assert!(
        err.to_string()
            .contains("source unit imports_main imports module imports_types more than once")
    );
}

#[test]
fn source_program_rejects_duplicate_module_identity() {
    let err =
        source_program_from_array([root_source(), shared_types_source(), shared_types_source()])
            .expect_err("duplicate module identity should fail");

    assert!(
        err.to_string()
            .contains("duplicate module identity imports_types")
    );
}

#[test]
fn source_program_requires_root_owned_main() {
    let err = source_program_from_array([
        r#"module imports_main;
import imports_worker;

record RootOnly;
"#,
        r#"module imports_worker;

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
"#,
    ])
    .expect_err("imported Main must not satisfy root entry");

    assert!(
        err.to_string()
            .contains("root source unit imports_main must declare entry process Main")
    );
}

#[test]
fn source_program_rejects_import_cycles() {
    let err = source_program_from_array([
        r#"module imports_main;
import imports_worker;
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
"#,
        r#"module imports_worker;
import imports_main;
record WorkerState;
"#,
    ])
    .expect_err("import cycle should fail");

    assert!(
        err.to_string()
            .contains("import cycle imports_main -> imports_worker -> imports_main")
    );
}

#[test]
fn source_program_rejects_cross_unit_name_ambiguity() {
    let err = source_program_from_array([
        root_source(),
        shared_types_source(),
        r#"module imports_worker;
import imports_types;

record Job;
record WorkerState;
enum WorkerMsg { Work(Job) }
"#,
    ])
    .expect_err("ambiguous imported type name should fail");

    assert!(err.to_string().contains(
        "ambiguous imported type name Job declared by modules imports_types and imports_worker"
    ));
}

#[test]
fn source_program_rejects_reachable_but_unimported_symbols() {
    let err = source_program_from_array([
        root_source(),
        shared_types_source(),
        r#"module imports_worker;

record WorkerState;
enum WorkerMsg {
    Work(Job),
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(job: Job)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker used a type it did not import";
        return Stop(state);
    }
}
"#,
    ])
    .expect_err("reachable sibling declarations must not leak through flattening");

    assert!(err.to_string().contains(
        "source unit imports_worker references type Job from module imports_types without importing imports_types"
    ));
}

#[test]
fn source_program_rejects_unimported_send_outcome_annotation_type() {
    let err = source_program_from_array([
        r#"module imports_main;
import imports_worker;

record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    authority spawn_worker: Cap<Spawn<Worker>>;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<Hidden>> = send worker Ping;
        return Stop(state);
    }
}
"#,
        r#"module imports_worker;
import imports_types;
record WorkerState;
enum WorkerMsg { Ping }
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;
    fn init() -> WorkerState ! [] ~ [] @det { return WorkerState; }
    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#,
        r#"module imports_types;
record Hidden;
"#,
    ])
    .expect_err("send outcome annotation must not bypass direct import validation");

    assert!(err.to_string().contains(
        "source unit imports_main references type Hidden from module imports_types without importing imports_types"
    ));
}

#[test]
fn source_program_rejects_cross_unit_function_constructor_name_ambiguity() {
    let err = source_program_from_array([
        r#"module imports_main;
import imports_types;
import imports_worker;

record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        let value: MaybeJob = Assign(Job);
        return Stop(state);
    }
}
"#,
        r#"module imports_types;

record Job;
enum MaybeJob {
    Assign(Job),
}
"#,
        r#"module imports_worker;
import imports_helper;

record WorkerState;
"#,
        r#"module imports_helper;

record HelperState;
fn Assign(value: HelperState) -> HelperState ! [] ~ [] @det {
    return value;
}
"#,
    ])
    .expect_err("function and enum constructor names must not collide across source units");

    assert!(err.to_string().contains(
        "ambiguous imported callable name Assign declared by modules imports_types and imports_helper"
    ));
}

#[test]
fn source_program_preserves_builtin_unit_value_with_transitive_unit_variant() {
    let program = source_program_from_array([
        r#"module imports_main;
import imports_worker;

enum MainMsg { Start }

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
"#,
        r#"module imports_worker;
import imports_types;

record WorkerState;
"#,
        r#"module imports_types;

enum HiddenUnitName {
    Unit,
}
"#,
    ])
    .expect("transitive enum variant named Unit must not shadow builtin Unit value");

    check_source_program(program).expect("builtin Unit value should check after import validation");
}

#[test]
fn source_program_rejects_cross_unit_enum_variant_name_ambiguity() {
    let err = source_program_from_array([
        r#"module imports_main;
import imports_types;
import imports_worker;

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
"#,
        r#"module imports_types;
enum Phase {
    Ready,
}
"#,
        r#"module imports_worker;
enum WorkerPhase {
    Ready,
}
"#,
    ])
    .expect_err("cross-unit enum constructor names must not collide before flattening");

    assert!(err.to_string().contains(
        "ambiguous imported enum variant name Ready declared by modules imports_types and imports_worker"
    ));
}

#[test]
fn source_unit_parse_rejects_invalid_source() {
    let err = SourceUnit::parse(
        SourceUnitId::from_index(0).unwrap(),
        "record MissingModule;".to_string(),
    )
    .expect_err("source unit construction must parse the source it stores");

    assert!(err.to_string().contains("expected keyword module"));
}

#[test]
fn source_program_hash_input_frames_multi_source_units() {
    let program =
        source_program_from_array([root_source(), shared_types_source(), worker_source()])
            .expect("multi-source program should build");
    let input = program.source_hash_input();

    let expected = format!(
        "strata-source-program-v2\n3\n{}\n{}{}\n{}{}\n{}",
        shared_types_source().len(),
        shared_types_source(),
        worker_source().len(),
        worker_source(),
        root_source().len(),
        root_source()
    );
    assert_eq!(input, expected);
    assert_eq!(
        program.source_provenance_hash().fnv1a64(),
        mantle_artifact::source_hash_fnv1a64(&expected)
    );
}

#[test]
fn source_program_hash_input_does_not_depend_on_source_unit_ids() {
    let canonical =
        source_program_from_array([root_source(), shared_types_source(), worker_source()])
            .expect("canonical multi-source program should build");
    let reordered = SourceProgram::new(
        SourceUnitId::from_index(1).unwrap(),
        vec![
            SourceUnit::parse(
                SourceUnitId::from_index(0).unwrap(),
                worker_source().to_string(),
            )
            .unwrap(),
            SourceUnit::parse(
                SourceUnitId::from_index(1).unwrap(),
                root_source().to_string(),
            )
            .unwrap(),
            SourceUnit::parse(
                SourceUnitId::from_index(2).unwrap(),
                shared_types_source().to_string(),
            )
            .unwrap(),
        ],
    )
    .expect("reordered multi-source program should build");

    assert_eq!(
        canonical.source_provenance_hash(),
        reordered.source_provenance_hash()
    );
}

#[test]
fn property_generated_dependency_graphs_reject_cycles_and_preserve_order() {
    for chain_len in 1..=6 {
        let mut sources = Vec::new();
        sources.push(generated_root_source(chain_len, false));
        for index in 0..chain_len {
            sources.push(generated_leaf_source(index, chain_len, false));
        }
        let program = source_program_from_strings(sources)
            .expect("generated acyclic graph should be accepted");
        assert_eq!(program.units().len(), chain_len + 1);
        assert_eq!(
            program.dependency_order().last().copied(),
            Some(SourceUnitId::from_index(0).unwrap()),
            "root should be last after dependency-first ordering"
        );

        let mut cyclic_sources = Vec::new();
        cyclic_sources.push(generated_root_source(chain_len, true));
        for index in 0..chain_len {
            cyclic_sources.push(generated_leaf_source(index, chain_len, true));
        }
        let err = source_program_from_strings(cyclic_sources)
            .expect_err("generated cycle should be rejected");
        assert!(err.to_string().contains("import cycle"));
    }
}

fn source_program_from_array<const N: usize>(
    sources: [&str; N],
) -> crate::language::Result<SourceProgram> {
    source_program_from_strings(
        sources
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
}

fn source_program_from_strings(sources: Vec<String>) -> crate::language::Result<SourceProgram> {
    let units = sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| SourceUnit::parse(SourceUnitId::from_index(index)?, source))
        .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(0)?, units)
}

fn process_id(artifact: &mantle_artifact::MantleArtifact, name: &str) -> ProcessId {
    let index = artifact
        .processes
        .iter()
        .position(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("{name} artifact process should exist"));
    ProcessId::from_index(index).expect("artifact process index should fit")
}

fn root_source() -> &'static str {
    r#"module imports_main;
import imports_types;
import imports_worker;

record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Work(complete(Job { phase: Ready }));
        return Stop(state);
    }
}
"#
}

fn shared_types_source() -> &'static str {
    r#"module imports_types;

record Job {
    phase: Phase,
}

enum Phase {
    Ready,
    Done,
}

fn complete(job: Job) -> Job ! [] ~ [] @det {
    return Job { phase: Done };
}
"#
}

fn worker_source() -> &'static str {
    r#"module imports_worker;
import imports_types;

record WorkerState;
enum WorkerMsg {
    Work(Job),
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(job: Job)) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "imported worker handled completed job";
        return Stop(state);
    }
}
"#
}

fn generated_root_source(chain_len: usize, cyclic: bool) -> String {
    format!(
        r#"module generated_root_{chain_len}_{cyclic};
import leaf0;
record MainState;
enum MainMsg {{ Start }}
proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState; }}
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

fn generated_leaf_source(index: usize, chain_len: usize, cyclic: bool) -> String {
    let next_import = if index + 1 < chain_len {
        format!("import leaf{};\n", index + 1)
    } else if cyclic {
        format!("import generated_root_{chain_len}_{cyclic};\n")
    } else {
        String::new()
    };
    format!(
        r#"module leaf{index};
{next_import}record Leaf{index};
"#
    )
}
