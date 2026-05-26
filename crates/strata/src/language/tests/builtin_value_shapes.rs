use super::support::*;

const BUILTIN_OUTCOMES: &str = r#"
module builtin_value_shapes;

record Job { phase: Phase }
record Outcomes {
    sent: Result<Unit,SendError<Job>>,
    spawned: Result<Unit,SpawnError<Job>>,
    maybe: Option<Phase>,
}
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}

fn failed(job: Job) -> Outcomes ! [] ~ [] @det {
    return Outcomes {
        sent: Err(MailboxClosed(job)),
        spawned: Err(Denied(job)),
        maybe: Some(Done),
    };
}

proc Main mailbox bounded(1) {
    type State = Outcomes;
    type Msg = MainMsg;

    fn init() -> Outcomes ! [] ~ [] @det {
        return Outcomes {
            sent: Ok(Unit),
            spawned: Ok(Unit),
            maybe: None,
        };
    }

    fn step(state: Outcomes, Start) -> ProcResult<Outcomes> ! [] ~ [] @det {
        return Continue(failed(Job { phase: Ready }));
    }
}
"#;

#[test]
fn checks_and_lowers_builtin_unit_option_result_error_values() {
    let checked = check_source(BUILTIN_OUTCOMES).expect("builtin outcome values should check");
    let artifact =
        lower_to_artifact(&checked, BUILTIN_OUTCOMES).expect("builtin outcome values should lower");
    let main = &artifact.processes[0];
    let encoded = artifact.encode();

    assert!(
        main.state_values
            .iter()
            .any(|state| contains_enum_variant_value(&state.value))
    );
    assert!(artifact.types.iter().any(|ty| matches!(
        ty.shape,
        Some(mantle_artifact::ArtifactValueShape::Enum { .. })
    )));
    assert!(!encoded.contains("failed"));
}

#[test]
fn rejects_declaration_names_reserved_for_builtin_value_shapes() {
    for (replacement, expected) in [
        ("record Unit;", "type name Unit is reserved"),
        ("record Option;", "type name Option is reserved"),
        ("record SendError;", "type name SendError is reserved"),
    ] {
        let source = HELLO.replace("record MainState;", replacement);
        let err = check_source(&source).expect_err("reserved builtin type name should fail");

        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in `{err}`"
        );
    }
    for (replacement, expected) in [
        ("enum Result { Start }", "type name Result is reserved"),
        (
            "enum SpawnError { Start }",
            "type name SpawnError is reserved",
        ),
    ] {
        let source = HELLO.replace("enum MainMsg { Start }", replacement);
        let err = check_source(&source).expect_err("reserved builtin type name should fail");

        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in `{err}`"
        );
    }
}

#[test]
fn rejects_enum_variants_reserved_for_builtin_value_shapes() {
    for variant in [
        "None",
        "Some",
        "Ok",
        "Err",
        "Full",
        "Stopped",
        "Crashed",
        "MailboxClosed",
        "Denied",
        "Exhausted",
        "BackendUnavailable",
    ] {
        let source = BUILTIN_OUTCOMES.replace(
            "enum MainMsg {\n    Start,\n}",
            &format!("enum MainMsg {{ {variant} }}"),
        );
        let err = check_source(&source).expect_err("reserved builtin variant name should fail");
        let expected =
            format!("enum MainMsg variant {variant} uses reserved builtin value constructor name");

        assert!(
            err.to_string().contains(&expected),
            "expected `{expected}` in `{err}`"
        );
    }
}

#[test]
fn rejects_wrong_arity_for_builtin_value_shapes() {
    for (ty, expected) in [
        (
            "Option<Unit,Unit>",
            "option type Option<Unit,Unit> must declare exactly one type argument",
        ),
        (
            "Result<Unit>",
            "result type Result<Unit> must declare exactly two type arguments",
        ),
        (
            "SendError<Unit,Unit>",
            "send error type SendError<Unit,Unit> must declare exactly one message type",
        ),
        (
            "SpawnError<Unit,Unit>",
            "spawn error type SpawnError<Unit,Unit> must declare exactly one init-argument type",
        ),
    ] {
        let source = HELLO.replace(
            "record MainState;",
            &format!(
                "record MainState;\nfn invalid(input: Unit) -> {ty} ! [] ~ [] @det {{ return input; }}"
            ),
        );
        let err = check_source(&source).expect_err("wrong builtin type arity should fail");

        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in `{err}`"
        );
    }
}

#[test]
fn rejects_process_ref_payload_inside_builtin_error_shapes() {
    for (field, expected) in [
        (
            "sent: SendError<ProcessRef<Main>>",
            "field sent type SendError<ProcessRef<Main>> contains a process reference",
        ),
        (
            "spawned: SpawnError<ProcessRef<Main>>",
            "field spawned type SpawnError<ProcessRef<Main>> contains a process reference",
        ),
    ] {
        let source = HELLO.replace(
            "record MainState;",
            &format!("record MainState {{ {field} }}"),
        );
        let err = check_source(&source).expect_err("authority-carrying builtin shape should fail");

        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in `{err}`"
        );
    }
}

fn contains_enum_variant_value(value: &ArtifactValue) -> bool {
    match value {
        ArtifactValue::EnumVariant { .. } => true,
        ArtifactValue::Record { fields, .. } => fields
            .iter()
            .any(|field| contains_enum_variant_value(&field.value)),
        ArtifactValue::List(items) => items.iter().any(contains_enum_variant_value),
        ArtifactValue::Map(entries) => entries.iter().any(|entry| {
            contains_enum_variant_value(&entry.key) || contains_enum_variant_value(&entry.value)
        }),
        ArtifactValue::Atom(_) | ArtifactValue::Scalar(_) | ArtifactValue::ProcessRef { .. } => {
            false
        }
    }
}
