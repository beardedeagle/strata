use super::support::*;

#[test]
fn rejects_state_match_unknown_constructor() {
    let source = STATE_PAYLOAD_MATCH.replace("Idle => {", "Missing => {");

    let err = check_source(&source).expect_err("unknown state match constructor should fail");

    assert!(
        err.to_string()
            .contains("match pattern Missing is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_payload_state_match_without_payload_binding() {
    let source = STATE_PAYLOAD_MATCH.replace("Working(job: Job) => {", "Working => {");

    let err = check_source(&source).expect_err("payload state match without binding should fail");

    assert!(
        err.to_string()
            .contains("process Worker state match pattern Working requires a payload binding"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_fieldless_state_match_with_payload_binding() {
    let source = STATE_PAYLOAD_MATCH.replace("Idle => {", "Idle(job: Job) => {");

    let err = check_source(&source).expect_err("fieldless state match with binding should fail");

    assert!(
        err.to_string()
            .contains("process Worker state match pattern Idle does not carry a payload")
    );
}

#[test]
fn rejects_state_match_payload_binding_with_wrong_type() {
    let source =
        STATE_PAYLOAD_MATCH.replace("Working(job: Job) => {", "Working(job: JobPhase) => {");

    let err = check_source(&source).expect_err("wrong state match payload type should fail");

    assert!(
        err.to_string()
            .contains("process Worker state match payload job has type JobPhase, expected Job")
    );
}

#[test]
fn rejects_state_match_payload_binding_conflicting_with_message_binding() {
    let source = STATE_PAYLOAD_MATCH
        .replace(
            "enum WorkerMsg {\n    Assign(Job),\n    Complete,\n}",
            "enum WorkerMsg {\n    Assign(Job),\n    Complete(Job),\n}",
        )
        .replace(
            "send worker Complete;",
            "send worker Complete(Job { phase: Done });",
        )
        .replace(
            "fn step(state: WorkerState, Complete) -> ProcResult<WorkerState>",
            "fn step(state: WorkerState, Complete(job: Job)) -> ProcResult<WorkerState>",
        );

    let err = check_source(&source).expect_err("state payload binding name conflict should fail");

    assert!(err.to_string().contains(
        "process Worker state payload binding job conflicts with message payload binding"
    ));
}

#[test]
fn rejects_non_exhaustive_state_match() {
    let source = STATE_PAYLOAD_MATCH.replace(
        "            Done(job: Job) => {\n                emit \"worker already done\";\n                return Stop(Done(job));\n            }\n",
        "",
    );

    let err = check_source(&source).expect_err("non-exhaustive state match should fail");

    assert!(
        err.to_string()
            .contains("process Worker state match must handle variant Done")
    );
}

#[test]
fn rejects_duplicate_state_match_arm() {
    let source = STATE_PAYLOAD_MATCH.replace("Done(job: Job) => {", "Idle => {");

    let err = check_source(&source).expect_err("duplicate state match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker state match declares duplicate pattern for variant Idle")
    );
}

#[test]
fn rejects_state_payload_binding_outside_transition_arm() {
    let source = STATE_PAYLOAD_MATCH.replace("return Stop(Idle);", "return Stop(Done(job));");

    let err = check_source(&source).expect_err("state payload binding should be arm-local");

    assert!(
        err.to_string()
            .contains("record state type Job must be constructed with Job { ... }")
    );
}
