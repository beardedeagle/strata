use super::super::{SourceProgram, SourceUnit, SourceUnitId, check_source_program};

#[test]
fn source_program_rejects_transitive_fieldless_record_constructor_value() {
    let err = source_program_from_array([
        root_uses_hidden_record(),
        api_imports_hidden_record(),
        hidden_record(),
    ])
    .and_then(check_source_program)
    .expect_err("root must not construct a transitive fieldless record");

    assert!(
        err.to_string().contains(
            "source unit root references type Hidden from module hidden without importing hidden"
        ),
        "{err}"
    );
}

#[test]
fn source_program_rejects_builtin_named_transitive_enum_variant_value() {
    let err = source_program_from_array([
        root_uses_hidden_unit_variant(),
        api_imports_hidden_unit_variant(),
        hidden_unit_variant(),
    ])
    .and_then(check_source_program)
    .expect_err("builtin Unit must not construct a transitive source enum variant");

    assert!(
        err.to_string().contains(
            "source unit root references type HiddenUnitName from module hidden without importing hidden"
        ),
        "{err}"
    );
}

#[test]
fn source_program_rejects_transitive_enum_variant_in_imported_record_field() {
    let err = source_program_from_array([
        root_constructs_hidden_record_field(),
        api_record_with_hidden_field(),
        hidden_unit_variant(),
    ])
    .and_then(check_source_program)
    .expect_err("imported record construction must not expose transitive field types");

    assert!(
        err.to_string().contains(
            "source unit root references type HiddenUnitName from module hidden without importing hidden"
        ),
        "{err}"
    );
}

#[test]
fn source_program_rejects_transitive_enum_variant_in_imported_send_payload() {
    let err = source_program_from_array([
        root_sends_hidden_worker_payload(),
        worker_imports_hidden_message_payload(),
        hidden_unit_variant(),
    ])
    .and_then(check_source_program)
    .expect_err("send payload validation must not expose transitive message payload types");

    assert!(
        err.to_string().contains(
            "source unit root references type HiddenUnitName from module hidden without importing hidden"
        ),
        "{err}"
    );
}

fn source_program_from_array<const N: usize>(
    sources: [&str; N],
) -> crate::language::Result<SourceProgram> {
    let units = sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            SourceUnit::parse(SourceUnitId::from_index(index)?, source.to_string())
        })
        .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(0)?, units)
}

fn root_uses_hidden_record() -> &'static str {
    r#"module root;
import api;

enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = ApiState;
    type Msg = MainMsg;

    fn init() -> ApiState ! [] ~ [] @det {
        return ApiState;
    }

    fn step(state: ApiState, Start) -> ProcResult<ApiState> ! [] ~ [] @det {
        return Stop(as_state(Hidden));
    }
}
"#
}

fn api_imports_hidden_record() -> &'static str {
    r#"module api;
import hidden;

record ApiState;

fn as_state(value: Hidden) -> ApiState ! [] ~ [] @det {
    return ApiState;
}
"#
}

fn hidden_record() -> &'static str {
    r#"module hidden;

record Hidden;
"#
}

fn root_uses_hidden_unit_variant() -> &'static str {
    r#"module root;
import api;

enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = ApiState;
    type Msg = MainMsg;

    fn init() -> ApiState ! [] ~ [] @det {
        return ApiState;
    }

    fn step(state: ApiState, Start) -> ProcResult<ApiState> ! [] ~ [] @det {
        return Stop(as_state(Unit));
    }
}
"#
}

fn root_constructs_hidden_record_field() -> &'static str {
    r#"module root;
import api;

enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = ApiState;
    type Msg = MainMsg;

    fn init() -> ApiState ! [] ~ [] @det {
        return ApiState { hidden: Unit };
    }

    fn step(state: ApiState, Start) -> ProcResult<ApiState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
}

fn api_record_with_hidden_field() -> &'static str {
    r#"module api;
import hidden;

record ApiState {
    hidden: HiddenUnitName,
}
"#
}

fn root_sends_hidden_worker_payload() -> &'static str {
    r#"module root;
import worker;

enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = Unit;
    type Msg = MainMsg;
    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> Unit ! [] ~ [] @det {
        return Unit;
    }

    fn step(state: Unit, Start) -> ProcResult<Unit> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Work(Unit);
        return Stop(state);
    }
}
"#
}

fn worker_imports_hidden_message_payload() -> &'static str {
    r#"module worker;
import hidden;

record WorkerState;
enum WorkerMsg {
    Work(HiddenUnitName),
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(value: HiddenUnitName)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
}

fn api_imports_hidden_unit_variant() -> &'static str {
    r#"module api;
import hidden;

record ApiState;

fn as_state(value: HiddenUnitName) -> ApiState ! [] ~ [] @det {
    return ApiState;
}
"#
}

fn hidden_unit_variant() -> &'static str {
    r#"module hidden;

enum HiddenUnitName {
    Unit,
}
"#
}
