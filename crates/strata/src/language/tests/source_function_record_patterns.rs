use super::support::*;

fn assert_concrete_record_value_argument_error(source: &str, context: &str) {
    let err = check_source(source).expect_err("non-concrete record dispatch should fail");
    let message = err.to_string();

    assert!(
        message.contains(context),
        "expected error context `{context}` in `{message}`"
    );
    assert!(
        message.contains("requires a concrete record value argument"),
        "expected concrete record diagnostic in `{message}`"
    );
}

mod body_match;
mod return_match;
mod signature_patterns;
