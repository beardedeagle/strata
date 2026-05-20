use super::*;

#[test]
fn rejects_mixed_parameter_pattern_and_match_dispatch() {
    let source = ACTOR_SEQUENCE.replace(
        r#"fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Second => {
                emit "worker handled Second";
                return Stop(Done);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("mixed step dispatch should fail");

    assert!(
        err.to_string()
            .contains("process Worker cannot mix match step bodies with step parameter patterns")
    );
}

#[test]
fn rejects_step_pattern_invalid_next_state() {
    let source = ACTOR_SEQUENCE.replace("Continue(SawFirst)", "Continue(UnknownState)");

    let err = check_source(&source).expect_err("invalid next state should fail");

    assert!(
        err.to_string()
            .contains("value UnknownState is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_match_arm_comma_separator() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            },
        }
    }"#,
    );

    let err = parse_source(&source).expect_err("comma-separated match arms should fail");

    assert!(
        err.to_string()
            .contains("match arms are block-delimited and must not use comma separators")
    );
}

#[test]
fn rejects_match_arm_split_fat_arrow() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping = > {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = parse_source(&source).expect_err("split match arm arrow should fail");

    assert!(err.to_string().contains("expected =>"));
}
