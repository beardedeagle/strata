use super::support::*;

fn assert_concrete_collection_value_argument_error(source: &str, context: &str, expected: &str) {
    let err = check_source(source).expect_err("non-concrete collection dispatch should fail");
    let message = err.to_string();

    assert!(
        message.contains(context),
        "expected error context `{context}` in `{message}`"
    );
    assert!(
        message.contains(expected),
        "expected concrete collection diagnostic `{expected}` in `{message}`"
    );
}

mod helper_surfaces;
mod map_rest_patterns;
mod non_concrete_dispatch;
mod subset_map_patterns;
mod template_keys;
mod type_validation;
