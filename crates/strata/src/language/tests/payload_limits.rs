use super::support::*;

#[test]
fn accepts_payload_value_near_label_limit_without_wrapping_message_label() {
    let source = payload_message_label_overflow_source();

    let checked = check_source(&source).expect("payload value near label limit should check");
    let worker = &checked.processes()[1];

    assert_eq!(worker.message_cases()[0].label(), "Assign");
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
    lower_to_artifact(&checked, &source).expect("near-limit payload should lower to artifact");
}
