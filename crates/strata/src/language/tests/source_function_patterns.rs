use super::support::*;

#[test]
fn rejects_duplicate_source_function_signature_pattern() {
    let source = FUNCTION_MATCH.replace("fn readiness_sig(Warm)", "fn readiness_sig(Cold)");

    let err = check_source(&source).expect_err("duplicate source function pattern should fail");

    assert!(
        err.to_string()
            .contains("module function readiness_sig declares duplicate pattern for variant Cold")
    );
}

#[test]
fn rejects_non_exhaustive_source_function_signature_patterns() {
    let source = FUNCTION_MATCH.replace(
        r#"
fn readiness_sig(Warm) -> Readiness ! [] ~ [] @det {
    return WarmReady;
}
"#,
        "\n",
    );

    let err = check_source(&source).expect_err("non-exhaustive function patterns should fail");

    assert!(
        err.to_string()
            .contains("module function readiness_sig must handle variant Warm")
    );
}

#[test]
fn rejects_non_exhaustive_source_function_match_body() {
    let source = FUNCTION_MATCH.replace(
        r#"
        Warm => {
            return WarmReady;
        }"#,
        "",
    );

    let err = check_source(&source).expect_err("non-exhaustive function match should fail");

    assert!(
        err.to_string()
            .contains("module function readiness_body match must handle variant Warm")
    );
}

#[test]
fn rejects_source_function_signature_payload_binding_with_wrong_type() {
    let source = r#"
module source_function_payload_signature;

record Payload;
enum Mode { Empty, Filled(Payload) }
enum Readiness { ColdReady, WarmReady }
record MainState { readiness: Readiness }
enum MainMsg { Start }

fn readiness(Filled(payload: Readiness)) -> Readiness ! [] ~ [] @det {
    return WarmReady;
}

fn readiness(Empty) -> Readiness ! [] ~ [] @det {
    return ColdReady;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { readiness: ColdReady };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("wrong source signature payload type should fail");

    assert!(err.to_string().contains(
        "module function readiness signature payload payload has type Readiness, expected Payload"
    ));
}

#[test]
fn rejects_source_function_match_fieldless_variant_payload_binding() {
    let source = r#"
module source_function_payload_match;

record Payload;
enum Mode { Empty, Filled(Payload) }
enum Readiness { ColdReady, WarmReady }
record MainState { readiness: Readiness }
enum MainMsg { Start }

fn readiness(mode: Mode) -> Readiness ! [] ~ [] @det {
    match mode {
        Empty(payload: Payload) => {
            return ColdReady;
        }
        Filled(payload: Payload) => {
            return WarmReady;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { readiness: ColdReady };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("fieldless source match binding should fail");

    assert!(
        err.to_string()
            .contains("module function readiness match pattern Empty does not carry a payload")
    );
}

#[test]
fn checks_source_function_signature_wildcard_covers_payload_variant() {
    let source = r#"
module source_function_payload_signature_wildcard;

record Payload;
enum Mode { Empty, Filled(Payload) }
enum Readiness { ColdReady, WarmReady }
record MainState { readiness: Readiness }
enum MainMsg { Start }

fn readiness(Empty) -> Readiness ! [] ~ [] @det {
    return ColdReady;
}

fn readiness(_) -> Readiness ! [] ~ [] @det {
    return WarmReady;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { readiness: ColdReady };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source).expect("source signature wildcard should cover payload-bearing variant");
}

#[test]
fn checks_source_function_match_wildcard_covers_payload_variant() {
    let source = r#"
module source_function_payload_match_wildcard;

record Payload;
enum Mode { Empty, Filled(Payload) }
enum Readiness { ColdReady, WarmReady }
record MainState { readiness: Readiness }
enum MainMsg { Start }

fn readiness(mode: Mode) -> Readiness ! [] ~ [] @det {
    match mode {
        Empty => {
            return ColdReady;
        }
        _ => {
            return WarmReady;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { readiness: ColdReady };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source).expect("source match wildcard should cover payload-bearing variant");
}

#[test]
fn rejects_unknown_source_enum_payload_constructor_value() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        "status_sig(Assigned(Job { phase: Ready }))",
        "status_sig(Missing(Job { phase: Ready }))",
    );

    let err = check_source(&source).expect_err("unknown source enum constructor should fail");

    assert!(
        err.to_string()
            .contains("value Missing is not a variant of enum Work")
    );
}

#[test]
fn rejects_payload_enum_constructor_without_payload_value() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        "status_sig(Assigned(Job { phase: Ready }))",
        "status_sig(Assigned)",
    );

    let err = check_source(&source).expect_err("payload constructor without payload should fail");

    assert!(err.to_string().contains(
        "enum variant Assigned requires a payload and cannot be used as a fieldless value"
    ));
}

#[test]
fn rejects_unknown_source_function_payload_signature_pattern() {
    let source = FUNCTION_PAYLOAD_MATCH.replace("fn status_sig(Empty)", "fn status_sig(Missing)");

    let err = check_source(&source).expect_err("unknown source signature pattern should fail");

    assert!(
        err.to_string()
            .contains("pattern Missing is not a declared enum variant")
    );
}

#[test]
fn rejects_unknown_source_function_payload_match_body_pattern() {
    let source = FUNCTION_PAYLOAD_MATCH.replace("        Empty => {", "        Missing => {");

    let err = check_source(&source).expect_err("unknown source match pattern should fail");

    assert!(
        err.to_string()
            .contains("match pattern Missing is not a variant of enum Work")
    );
}

#[test]
fn rejects_duplicate_source_function_payload_signature_pattern() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        "fn status_sig(Empty) -> WorkStatus ! [] ~ [] @det",
        "fn status_sig(Assigned(job: Job)) -> WorkStatus ! [] ~ [] @det",
    );

    let err = check_source(&source).expect_err("duplicate payload signature pattern should fail");

    assert!(
        err.to_string()
            .contains("module function status_sig declares duplicate pattern for variant Assigned")
    );
}

#[test]
fn rejects_non_exhaustive_source_function_payload_match_body() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        r#"        Empty => {
            return Idle;
        }
"#,
        "",
    );

    let err = check_source(&source).expect_err("non-exhaustive payload match should fail");

    assert!(
        err.to_string()
            .contains("module function status_body match must handle variant Empty")
    );
}

#[test]
fn rejects_unreachable_source_function_payload_match_wildcard() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        r#"        Assigned(job: Job) => {
            return Active(job);
        }
"#,
        r#"        Assigned(job: Job) => {
            return Active(job);
        }
        _ => {
            return Idle;
        }
"#,
    );

    let err = check_source(&source).expect_err("unreachable payload wildcard should fail");

    assert!(
        err.to_string()
            .contains("module function status_body match wildcard pattern is unreachable")
    );
}

#[test]
fn rejects_source_function_match_payload_binding_named_like_parameter() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        r#"        Assigned(job: Job) => {
            return Active(job);
        }
"#,
        r#"        Assigned(work: Job) => {
            return Active(work);
        }
"#,
    );

    let err = check_source(&source).expect_err("shadowing source payload binding should fail");

    assert!(err.to_string().contains(
        "function status_body match payload binding work conflicts with an existing source value binding"
    ));
}

#[test]
fn rejects_source_helper_name_colliding_with_payload_constructor() {
    let source = FUNCTION_PAYLOAD_MATCH.replace(
        "proc Main mailbox bounded(1) {",
        r#"fn Assigned(job: Job) -> WorkStatus ! [] ~ [] @det {
    return Active(job);
}

proc Main mailbox bounded(1) {"#,
    );

    let err = check_source(&source).expect_err("constructor helper name collision should fail");

    assert!(
        err.to_string().contains(
            "module function Assigned conflicts with a declared type or value constructor"
        )
    );
}
