use super::support::*;

const PROCESS_REF_CARRIER_BASE: &str = r#"
module source_function_process_ref_carrier;

enum Phase { Idle, Active }
record MainState { selected: Phase }
record WorkerState;
enum MainMsg { Start, Route(ProcessRef<Worker>) }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: MainState, Route(worker: ProcessRef<Worker>)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[test]
fn rejects_process_reference_carrier_enum_source_function_contract() {
    let source = PROCESS_REF_CARRIER_BASE.replace(
        "proc Main mailbox bounded(1)",
        "fn keep(msg: MainMsg) -> MainMsg ! [] ~ [] @det { return msg; }\n\nproc Main mailbox bounded(1)",
    );

    let err = check_source(&source)
        .expect_err("source function contract should reject authority carrier enum");

    assert!(
        err.to_string().contains(
            "function keep return type must use a declared record, enum, list, or map type without process-reference authority, found MainMsg"
        ),
        "{err}"
    );
}

#[test]
fn rejects_process_reference_carrier_enum_source_local_binding() {
    let source = PROCESS_REF_CARRIER_BASE.replace(
        "proc Main mailbox bounded(1)",
        "fn route(phase: Phase) -> Phase ! [] ~ [] @det {\n    let copy: MainMsg = Start;\n    return phase;\n}\n\nproc Main mailbox bounded(1)",
    );

    let err = check_source(&source)
        .expect_err("source-local binding should reject authority carrier enum");

    assert!(
        err.to_string().contains(
            "source-local binding copy must use a declared record, enum, list, or map type without process-reference authority, found MainMsg"
        ),
        "{err}"
    );
}
