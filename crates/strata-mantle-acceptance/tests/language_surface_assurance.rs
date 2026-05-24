#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[path = "language_surface_assurance/model.rs"]
mod model;

macro_rules! evidence {
    ($class:ident, $path:literal, $marker:literal) => {
        $crate::model::Evidence::new($crate::model::EvidenceClass::$class, $path, $marker)
    };
}

macro_rules! feature {
    ($id:literal, $title:literal, $status:ident, $layer:ident, $required:expr, [$($class:ident => ($path:literal, $marker:literal)),+ $(,)?] $(,)?) => {
        $crate::model::Feature {
            id: $id,
            title: $title,
            status: $crate::model::FeatureStatus::$status,
            layer: $crate::model::SurfaceLayer::$layer,
            required: $required,
            evidence: &[$(evidence!($class, $path, $marker)),+],
        }
    };
}

#[path = "language_surface_assurance/boundary.rs"]
mod boundary;
#[path = "language_surface_assurance/docs_only.rs"]
mod docs_only;
#[path = "language_surface_assurance/process.rs"]
mod process;
#[path = "language_surface_assurance/rejected.rs"]
mod rejected;
#[path = "language_surface_assurance/runtime.rs"]
mod runtime;
#[path = "language_surface_assurance/source.rs"]
mod source;
#[path = "language_surface_assurance/validation.rs"]
mod validation;

use model::{EvidenceClass, Feature, FeatureStatus, SurfaceLayer, expected};
use validation::EvidenceCache;

const FEATURE_GROUPS: &[&[Feature]] = &[
    source::FEATURES,
    process::FEATURES,
    runtime::FEATURES,
    boundary::FEATURES,
    docs_only::FEATURES,
    rejected::FEATURES,
];

#[test]
fn every_declared_surface_has_required_evidence_classes() {
    let mut violations = Vec::new();

    for feature in inventory() {
        for required in feature.required {
            if !feature
                .evidence
                .iter()
                .any(|evidence| evidence.class == *required)
            {
                violations.push(format!(
                    "{} lacks required {} evidence",
                    feature.id,
                    required.as_str()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "language surface assurance inventory is incomplete:\n{}",
        violations.join("\n")
    );
}

#[test]
fn evidence_files_and_markers_exist() {
    let mut cache = EvidenceCache::new(workspace_root());
    let mut violations = Vec::new();

    for feature in inventory() {
        for evidence in feature.evidence {
            if let Err(message) = cache.verify(evidence) {
                violations.push(format!("{}: {message}", feature.id));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "language surface assurance evidence points at missing repo content:\n{}",
        violations.join("\n")
    );
}

#[test]
fn inventory_metadata_stays_stable_and_classified() {
    let mut ids = BTreeSet::new();
    let mut current_ids = BTreeSet::new();
    let mut docs_only_ids = BTreeSet::new();
    let mut rejected_ids = BTreeSet::new();

    for feature in inventory() {
        assert_feature_id(feature.id);
        assert!(
            !feature.title.trim().is_empty(),
            "{} must have a human-readable title",
            feature.id
        );
        assert!(
            !feature.required.is_empty(),
            "{} must declare required evidence classes",
            feature.id
        );
        assert!(
            !feature.evidence.is_empty(),
            "{} must declare evidence pointers",
            feature.id
        );
        assert!(
            ids.insert(feature.id),
            "duplicate language surface feature id {}",
            feature.id
        );
        for required in feature.required {
            assert!(
                feature.layer.allows_evidence(*required),
                "{} requires {} evidence, which is incompatible with {:?}",
                feature.id,
                required.as_str(),
                feature.layer
            );
        }
        for evidence in feature.evidence {
            assert!(
                feature.layer.allows_evidence(evidence.class),
                "{} claims {} evidence, which is incompatible with {:?}",
                feature.id,
                evidence.class.as_str(),
                feature.layer
            );
        }

        match feature.status {
            FeatureStatus::Current => {
                current_ids.insert(feature.id);
                assert!(
                    feature.layer.is_current_surface(),
                    "current feature {} must use an implementation surface layer",
                    feature.id
                );
                assert!(
                    !feature
                        .evidence
                        .iter()
                        .map(|evidence| evidence.class)
                        .all(EvidenceClass::is_docs_only),
                    "current feature {} cannot be documented by docs/examples only",
                    feature.id
                );
            }
            FeatureStatus::DocumentationOnly => {
                docs_only_ids.insert(feature.id);
                assert_eq!(
                    feature.layer,
                    SurfaceLayer::DocsExamplesOnly,
                    "documentation-only feature {} must use the docs/examples layer",
                    feature.id
                );
                assert!(
                    feature
                        .evidence
                        .iter()
                        .map(|evidence| evidence.class)
                        .all(EvidenceClass::is_docs_only),
                    "documentation-only feature {} must not claim implementation evidence",
                    feature.id
                );
            }
            FeatureStatus::FutureOrRejected => {
                rejected_ids.insert(feature.id);
                assert_eq!(
                    feature.layer,
                    SurfaceLayer::FutureNonAdmitted,
                    "future/rejected feature {} must use the future layer",
                    feature.id
                );
                assert!(
                    feature
                        .evidence
                        .iter()
                        .map(|evidence| evidence.class)
                        .all(EvidenceClass::is_rejected_surface_evidence),
                    "future/rejected feature {} must not claim admitted implementation evidence",
                    feature.id
                );
                assert!(
                    feature
                        .evidence
                        .iter()
                        .any(|evidence| evidence.class == EvidenceClass::NegativeTest),
                    "future/rejected feature {} must carry rejection evidence",
                    feature.id
                );
            }
        }
    }

    assert_expected_feature_ids(
        "current implemented surface",
        &current_ids,
        expected::CURRENT_FEATURE_IDS,
    );
    assert_expected_feature_ids(
        "docs/examples-only surface",
        &docs_only_ids,
        expected::DOCUMENTATION_ONLY_FEATURE_IDS,
    );
    assert_expected_feature_ids(
        "future or non-admitted surface",
        &rejected_ids,
        expected::FUTURE_OR_REJECTED_FEATURE_IDS,
    );
}

fn inventory() -> impl Iterator<Item = &'static Feature> {
    FEATURE_GROUPS.iter().flat_map(|features| features.iter())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test crate should be under crates/")
        .to_path_buf()
}

fn assert_feature_id(id: &str) {
    assert!(!id.is_empty(), "feature id must not be empty");
    assert!(
        id.bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "feature id {id:?} must use lowercase kebab-case"
    );
    assert!(
        !id.starts_with('-') && !id.ends_with('-') && !id.contains("--"),
        "feature id {id:?} must use stable kebab-case"
    );
}

fn assert_expected_feature_ids(
    label: &str,
    actual: &BTreeSet<&'static str>,
    expected: &[&'static str],
) {
    for expected_id in expected {
        assert!(
            actual.contains(expected_id),
            "{label} inventory is missing expected feature id {expected_id:?}"
        );
    }

    for actual_id in actual {
        assert!(
            expected.contains(actual_id),
            "{label} inventory contains unexpected feature id {actual_id:?}"
        );
    }
}
