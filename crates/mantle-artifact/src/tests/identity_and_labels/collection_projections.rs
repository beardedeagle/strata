use super::*;

#[test]
fn map_projection_rejects_duplicate_expected_keys() {
    let map =
        ArtifactValue::parse("Map[Done=>Ready,Ready=>Done]").expect("test map value should parse");
    let key = ArtifactValue::parse("Ready").expect("test key should parse");
    let keys = vec![key.clone(), key.clone()];

    let err = map
        .project_map_value(&key, &keys, MapProjectionMode::Exact)
        .expect_err("duplicate projection keys must fail closed");

    assert!(
        err.to_string()
            .contains("map projection duplicates expected map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn map_rest_projection_rejects_duplicate_excluded_keys() {
    let map =
        ArtifactValue::parse("Map[Done=>Ready,Ready=>Done]").expect("test map value should parse");
    let key = ArtifactValue::parse("Ready").expect("test key should parse");
    let keys = vec![key.clone(), key];

    let err = map
        .project_map_rest(&keys)
        .expect_err("duplicate rest projection keys must fail closed");

    assert!(
        err.to_string()
            .contains("map rest projection duplicates excluded map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn map_rest_projection_reports_missing_excluded_key_precisely() {
    let map = ArtifactValue::parse("Map[Done=>Ready]").expect("test map value should parse");
    let keys = vec![ArtifactValue::parse("Ready").expect("test key should parse")];

    let err = map
        .project_map_rest(&keys)
        .expect_err("missing excluded rest key must fail closed");

    assert!(
        err.to_string()
            .contains("map rest projection expected excluded map key Ready, found [Done]"),
        "unexpected error: {err}"
    );
}

#[test]
fn list_prefix_projection_returns_prefix_element_from_longer_list() {
    let list = ArtifactValue::parse("List[Ready,Done]").expect("test list value should parse");

    let element = list
        .project_list_prefix_element(0, 1)
        .expect("list prefix projection should produce prefix element");

    assert_eq!(element.label(), "Ready");
}

#[test]
fn list_prefix_projection_rejects_index_outside_prefix() {
    let list = ArtifactValue::parse("List[Ready,Done]").expect("test list value should parse");

    let err = list
        .project_list_prefix_element(1, 1)
        .expect_err("outside-prefix list element should fail closed");

    assert!(
        err.to_string()
            .contains("list prefix projection index 1 is outside prefix length 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn list_rest_projection_constructs_suffix_list() {
    let list = ArtifactValue::parse("List[Ready,Done]").expect("test list value should parse");

    let rest = list
        .project_list_rest(1)
        .expect("list rest projection should produce suffix");

    assert_eq!(rest.label(), "List[Done]");
}

#[test]
fn list_rest_projection_rejects_zero_prefix() {
    let list = ArtifactValue::parse("List[Ready]").expect("test list value should parse");

    let err = list
        .project_list_rest(0)
        .expect_err("zero prefix list rest should fail closed");

    assert!(
        err.to_string()
            .contains("list rest projection requires at least one prefix element"),
        "unexpected error: {err}"
    );
}

#[test]
fn list_rest_projection_reports_short_list_precisely() {
    let list = ArtifactValue::parse("List[Ready]").expect("test list value should parse");

    let err = list
        .project_list_rest(2)
        .expect_err("short list rest projection should fail closed");

    assert!(
        err.to_string().contains(
            "list rest projection requires at least 2 prefix elements, found 1 in List[Ready]"
        ),
        "unexpected error: {err}"
    );
}
