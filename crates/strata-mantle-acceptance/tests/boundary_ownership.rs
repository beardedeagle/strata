#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn mantle_crates_do_not_own_strata_surfaces() {
    let root = workspace_root();
    let mantle_paths = [
        root.join("crates/mantle-artifact"),
        root.join("crates/mantle-runtime"),
    ];
    let forbidden = [
        "strata",
        "target/strata",
        "STRATA_SOURCE_LANGUAGE",
        "source-to-runtime",
        "examples/",
        "ProcessRef<",
        "MAX_TYPE_REF_BYTES",
        "validate_type_field",
        "process_ref_type_target",
        "payload_type: Option<String>",
        "state_type: String",
        "message_type: String",
        "ty: String",
        "payload_type\":\"",
    ];

    let mut violations = Vec::new();
    for path in mantle_paths {
        collect_forbidden_matches(&path, &forbidden, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "Mantle-owned crates must stay language-neutral:\n{}",
        violations.join("\n")
    );
}

#[test]
fn strata_mantle_acceptance_harness_is_workspace_owned() {
    let root = workspace_root();

    assert!(
        root.join("crates/strata-mantle-acceptance/tests/source_to_runtime_gates.rs")
            .is_file(),
        "source-to-runtime gates must live in the workspace acceptance harness"
    );
    assert!(
        !root
            .join("crates/mantle-runtime/tests/source_to_runtime_gates.rs")
            .exists(),
        "Mantle runtime tests must not own Strata source-to-runtime gates"
    );
}

#[test]
fn strata_lowering_consumes_checked_type_ids_not_source_type_refs() {
    let root = workspace_root();
    let lowering = root.join("crates/strata/src/language/lowering.rs");
    let checked = root.join("crates/strata/src/language/checked.rs");
    let forbidden = [
        "use super::TypeRef",
        "PROCESS_REF_TYPE",
        "process_ids_by_name",
        "process_id_for_name",
        "entries: Vec<(TypeRef",
        "types.intern(process.state_type())",
        "types.intern(process.message_type())",
        "types.intern(ty)",
    ];

    let mut violations = Vec::new();
    collect_forbidden_matches(&lowering, &forbidden, &mut violations);
    collect_forbidden_matches(
        &checked,
        &["ast::TypeRef", "Module, TypeRef"],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "Strata checked IR and lowering must use checked type IDs after semantic resolution:\n{}",
        violations.join("\n")
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test crate should be under crates/")
        .to_path_buf()
}

fn collect_forbidden_matches(path: &Path, forbidden: &[&str], violations: &mut Vec<String>) {
    if path.is_dir() {
        for entry in fs::read_dir(path)
            .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", path.display()))
        {
            let entry = entry.expect("directory entry should be readable");
            collect_forbidden_matches(&entry.path(), forbidden, violations);
        }
        return;
    }

    if !is_scanned_file(path) {
        return;
    }

    let contents = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let normalized = source_boundary_normalized_contents(path, &contents);
    for pattern in forbidden {
        if normalized.contains(&pattern.to_ascii_lowercase()) {
            violations.push(format!("{} contains {pattern:?}", path.display()));
        }
    }
}

fn source_boundary_normalized_contents(path: &Path, contents: &str) -> String {
    let mut normalized = contents.to_ascii_lowercase();
    if path.ends_with("crates/mantle-runtime/src/feature_declaration.rs") {
        // Mantle may publish spec-defined source-family metadata keys in its
        // runtime declaration; it still must not own source semantics.
        for allowed in [
            "strata_version",
            "optional_strata_profiles",
            "strata.exact_effects_supported",
            "strata.determinism_sources_supported",
        ] {
            normalized = normalized.replace(allowed, "");
        }
    }
    normalized
}

fn is_scanned_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "Cargo.toml")
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "rs")
}
