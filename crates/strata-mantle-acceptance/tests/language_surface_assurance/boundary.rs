use crate::model::{Feature, requirements::*};

pub(crate) const FEATURES: &[Feature] = &[
    feature!(
        "checked-ir-and-typed-id-boundary",
        "Checked IR and lowering preserve typed IDs across the boundary",
        Current,
        CheckerLowering,
        TYPED_BOUNDARY_REQUIREMENTS,
        [
            CheckerValidation => ("crates/strata-mantle-acceptance/tests/boundary_ownership.rs", "strata_lowering_consumes_checked_type_ids_not_source_type_refs"),
            CheckedIrLowering => ("crates/strata/src/language/tests/process_refs.rs", "!encoded.contains(\"target_process=Worker\")"),
            Diagnostics => ("docs/src/runtime-traces.md", "Do not treat labels as runtime dispatch keys"),
            PositiveTest => ("crates/strata/src/language/tests/nested_pattern_destructuring/basic_patterns.rs", "constructor names must not be lowered as record-field executable references"),
            NegativeTest => ("crates/strata-mantle-acceptance/tests/boundary_ownership.rs", "Mantle-owned crates must stay language-neutral"),
            Documentation => ("docs/src/runtime-traces.md", "Do not treat labels as runtime dispatch keys"),
        ],
    ),
    feature!(
        "mantle-artifact-admission",
        "Mantle artifact identity, decoding, and admission",
        Current,
        ArtifactAdmission,
        BOUNDARY_REQUIREMENTS,
        [
            ArtifactAdmission => ("crates/mantle-artifact/src/tests/codec/decode_failures.rs", "decode_rejects_unbounded_process_count_before_allocation"),
            RuntimeExecution => ("crates/mantle-runtime/src/run/tests/identity_admission.rs", "runtime_rejects_loaded_invalid_artifact_identity_before_artifact_loaded"),
            Diagnostics => ("docs/src/artifact-runtime-boundary.md", "Mantle admits artifacts through validation, not filename trust"),
            PositiveTest => ("crates/mantle-artifact/src/tests/codec/round_trip.rs", "artifact_round_trips_and_validates_magic"),
            NegativeTest => ("crates/mantle-artifact/src/tests/identity_and_labels/artifact_limits_and_metadata.rs", "validate_treats_debug_names_as_metadata_not_targets"),
            Documentation => ("docs/src/file-types.md", "Mantle Target Artifact"),
        ],
    ),
    feature!(
        "loaded-runtime-validation",
        "Loaded-runtime validation before runtime side effects",
        Current,
        ArtifactAdmission,
        BOUNDARY_REQUIREMENTS,
        [
            ArtifactAdmission => ("crates/mantle-runtime/src/program/admission.rs", "validate_loaded_artifact_identity"),
            RuntimeExecution => ("crates/mantle-runtime/src/run/tests/state_message_admission/action_admission.rs", "runtime_rejects_loaded_spawn_inside_runtime_if_branch_before_artifact_loaded"),
            Diagnostics => ("docs/src/artifact-runtime-boundary.md", "`ArtifactLoaded` or executing runtime side effects"),
            PositiveTest => ("crates/mantle-runtime/src/run/tests/state_message_admission/action_admission.rs", "runtime_accepts_loaded_for_each_inside_runtime_if_branch"),
            NegativeTest => ("crates/mantle-runtime/src/run/tests/state_message_admission/next_state_templates.rs", "runtime_rejects_loaded_unadmitted_template_state_before_artifact_loaded"),
            Documentation => ("docs/src/artifact-runtime-boundary.md", "Mantle validates loaded"),
        ],
    ),
];
