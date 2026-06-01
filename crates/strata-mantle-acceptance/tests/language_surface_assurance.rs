#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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
#[path = "language_surface_assurance/proof_domains.rs"]
mod proof_domains;
#[path = "language_surface_assurance/proof_obligations.rs"]
mod proof_obligations;
#[path = "language_surface_assurance/rejected.rs"]
mod rejected;
#[path = "language_surface_assurance/runtime.rs"]
mod runtime;
#[path = "language_surface_assurance/source.rs"]
mod source;
#[path = "language_surface_assurance/validation.rs"]
mod validation;

use model::{EvidenceClass, Feature, FeatureStatus, ProofObligationClass, SurfaceLayer, expected};
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
        "language surface proof substrate inventory is incomplete:\n{}",
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
        "language surface proof substrate evidence points at missing repo content:\n{}",
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

    assert_expected_ids(
        "current implemented surface",
        &current_ids,
        expected::CURRENT_FEATURE_IDS,
    );
    assert_expected_ids(
        "docs/examples-only surface",
        &docs_only_ids,
        expected::DOCUMENTATION_ONLY_FEATURE_IDS,
    );
    assert_expected_ids(
        "future or non-admitted surface",
        &rejected_ids,
        expected::FUTURE_OR_REJECTED_FEATURE_IDS,
    );
}

#[test]
fn proof_domains_cover_the_declared_language_surface() {
    let feature_ids = inventory()
        .map(|feature| feature.id)
        .collect::<BTreeSet<_>>();
    let mut domain_ids = BTreeSet::new();
    let mut covered_feature_ids = BTreeSet::new();

    for domain in proof_domains::DOMAINS {
        assert_feature_id(domain.id);
        assert!(
            !domain.title.trim().is_empty(),
            "{} must have a human-readable title",
            domain.id
        );
        assert!(
            !domain.feature_ids.is_empty(),
            "{} must cite at least one language-surface feature",
            domain.id
        );
        assert!(
            domain_ids.insert(domain.id),
            "duplicate proof domain id {}",
            domain.id
        );

        for &feature_id in domain.feature_ids {
            assert!(
                feature_ids.contains(feature_id),
                "proof domain {} cites unknown language-surface feature {}",
                domain.id,
                feature_id
            );
            covered_feature_ids.insert(feature_id);
        }
    }

    assert_expected_ids(
        "proof substrate domain",
        &domain_ids,
        expected::PROOF_DOMAIN_IDS,
    );

    for feature_id in feature_ids {
        assert!(
            covered_feature_ids.contains(feature_id),
            "language-surface feature {feature_id:?} is not included in any proof domain"
        );
    }
}

#[test]
fn proof_domain_obligations_match_surface_layers_and_evidence() {
    let features = inventory()
        .map(|feature| (feature.id, feature))
        .collect::<BTreeMap<_, _>>();
    let mut violations = Vec::new();

    for domain in proof_domains::DOMAINS {
        let declared_obligations = domain
            .obligations
            .iter()
            .map(|obligation| obligation.class)
            .collect::<BTreeSet<_>>();
        let mut required_obligations = BTreeSet::new();
        let mut evidence_classes = BTreeSet::new();

        if domain.obligations.is_empty() {
            violations.push(format!("{} must declare proof obligations", domain.id));
        }

        for obligation in domain.obligations {
            if obligation.claim.trim().is_empty() {
                violations.push(format!(
                    "{} has empty {} proof claim",
                    domain.id,
                    obligation.class.as_str()
                ));
            }
        }

        for &feature_id in domain.feature_ids {
            let feature = features
                .get(feature_id)
                .expect("proof domain feature ids are validated by coverage test");
            required_obligations.extend(required_obligations_for_feature(feature));
            evidence_classes.extend(feature.evidence.iter().map(|evidence| evidence.class));
        }

        for required in required_obligations {
            if !declared_obligations.contains(&required) {
                violations.push(format!(
                    "{} lacks required {} proof obligation",
                    domain.id,
                    required.as_str()
                ));
            }
        }

        for obligation in domain.obligations {
            if !obligation.class.is_supported_by(&evidence_classes) {
                violations.push(format!(
                    "{} declares {} proof obligation without supporting evidence",
                    domain.id,
                    obligation.class.as_str()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "language surface proof obligations are incomplete:\n{}",
        violations.join("\n")
    );
}

#[test]
fn mantle_trace_validate_seed_inventory_covers_committed_jsonl_seeds() {
    let committed = committed_mantle_trace_validate_seed_paths();
    let declared = runtime::FEATURES
        .iter()
        .flat_map(|feature| feature.evidence)
        .filter(|evidence| {
            evidence.class == EvidenceClass::FuzzSeed
                && evidence
                    .path
                    .starts_with("fuzz/seeds/mantle_trace_validate/")
        })
        .map(|evidence| evidence.path.to_string())
        .collect::<BTreeSet<_>>();

    let missing = committed
        .difference(&declared)
        .map(String::as_str)
        .collect::<Vec<_>>();
    let stale = declared
        .difference(&committed)
        .map(String::as_str)
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty() && stale.is_empty(),
        "mantle_trace_validate fuzz seed inventory drifted\nmissing from inventory: {missing:#?}\nstale inventory entries: {stale:#?}"
    );
}

fn inventory() -> impl Iterator<Item = &'static Feature> {
    FEATURE_GROUPS.iter().flat_map(|features| features.iter())
}

fn committed_mantle_trace_validate_seed_paths() -> BTreeSet<String> {
    let seed_dir = workspace_root().join("fuzz/seeds/mantle_trace_validate");
    fs::read_dir(&seed_dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", seed_dir.display()))
        .map(|entry| {
            let path = entry
                .unwrap_or_else(|err| panic!("failed to read seed entry: {err}"))
                .path();
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| {
                    panic!("seed path {} must have a UTF-8 file name", path.display())
                });
            assert!(
                file_name.ends_with(".jsonl"),
                "mantle trace fuzz seed {} must be a JSONL corpus file",
                path.display()
            );
            format!("fuzz/seeds/mantle_trace_validate/{file_name}")
        })
        .collect()
}

fn required_obligations_for_feature(feature: &Feature) -> BTreeSet<ProofObligationClass> {
    let mut required = BTreeSet::new();

    for evidence_class in feature.required {
        match evidence_class {
            EvidenceClass::ParserCoverage => {
                required.insert(ProofObligationClass::SourceSyntax);
            }
            EvidenceClass::CheckerValidation => {
                required.insert(ProofObligationClass::StaticValidation);
            }
            EvidenceClass::CheckedIrLowering => {
                required.insert(ProofObligationClass::CheckedIrProjection);
                required.insert(ProofObligationClass::StrataMantleBoundary);
            }
            EvidenceClass::CompositionReport => {}
            EvidenceClass::ArtifactAdmission => {
                required.insert(ProofObligationClass::ArtifactAdmission);
                required.insert(ProofObligationClass::StrataMantleBoundary);
            }
            EvidenceClass::RuntimeExecution => {
                required.insert(ProofObligationClass::RuntimeExecution);
            }
            EvidenceClass::Diagnostics => {
                required.insert(ProofObligationClass::Diagnostics);
            }
            EvidenceClass::RunnableExample => {
                required.insert(ProofObligationClass::RunnableExample);
            }
            EvidenceClass::PositiveTest | EvidenceClass::NegativeTest => {
                required.insert(ProofObligationClass::TestCoverage);
            }
            EvidenceClass::SourceToRuntimeGate => {
                required.insert(ProofObligationClass::SourceToRuntimeExecution);
            }
            EvidenceClass::FuzzSeed => {
                required.insert(ProofObligationClass::FuzzSeedCorpus);
            }
            EvidenceClass::BoundedOrProperty => {
                required.insert(ProofObligationClass::BoundedOrProperty);
            }
            EvidenceClass::PerformanceSmoke => {}
            EvidenceClass::Documentation => {
                required.insert(ProofObligationClass::Documentation);
            }
        }
    }

    required
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

fn assert_expected_ids(label: &str, actual: &BTreeSet<&'static str>, expected: &[&'static str]) {
    for expected_id in expected {
        assert!(
            actual.contains(expected_id),
            "{label} inventory is missing expected id {expected_id:?}"
        );
    }

    for actual_id in actual {
        assert!(
            expected.contains(actual_id),
            "{label} inventory contains unexpected id {actual_id:?}"
        );
    }
}
