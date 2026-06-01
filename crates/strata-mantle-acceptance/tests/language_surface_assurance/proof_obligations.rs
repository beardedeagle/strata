use crate::model::{ProofObligation, ProofObligationClass};

const SOURCE_SYNTAX: ProofObligation = obligation(
    ProofObligationClass::SourceSyntax,
    "the source grammar admits only the declared forms for this domain",
);
const STATIC_VALIDATION: ProofObligation = obligation(
    ProofObligationClass::StaticValidation,
    "the checker validates the static meaning for this domain",
);
const CHECKED_IR_PROJECTION: ProofObligation = obligation(
    ProofObligationClass::CheckedIrProjection,
    "checked source meaning projects into typed checked IR before lowering",
);
const STRATA_MANTLE_BOUNDARY: ProofObligation = obligation(
    ProofObligationClass::StrataMantleBoundary,
    "typed IDs cross the Strata/Mantle boundary without source-name dispatch",
);
const ARTIFACT_ADMISSION: ProofObligation = obligation(
    ProofObligationClass::ArtifactAdmission,
    "Mantle validates admitted artifact shapes before runtime execution",
);
const RUNTIME_EXECUTION: ProofObligation = obligation(
    ProofObligationClass::RuntimeExecution,
    "Mantle executes the admitted runtime behavior",
);
const DIAGNOSTICS: ProofObligation = obligation(
    ProofObligationClass::Diagnostics,
    "invalid forms have layer-appropriate diagnostics",
);
const RUNNABLE_EXAMPLE: ProofObligation = obligation(
    ProofObligationClass::RunnableExample,
    "the admitted surface is represented by a repository example",
);
const TEST_COVERAGE: ProofObligation = obligation(
    ProofObligationClass::TestCoverage,
    "tests bound the admitted or rejected surface",
);
const SOURCE_TO_RUNTIME: ProofObligation = obligation(
    ProofObligationClass::SourceToRuntimeExecution,
    "runtime-bearing behavior keeps an executable source-to-runtime gate",
);
const FUZZ_SEED: ProofObligation = obligation(
    ProofObligationClass::FuzzSeedCorpus,
    "fuzz seeds keep mutation-based gates anchored on admitted examples and fail-closed rejections",
);
const BOUNDED_PROPERTY: ProofObligation = obligation(
    ProofObligationClass::BoundedOrProperty,
    "bounded or property evidence covers the finite semantic space",
);
const DOCUMENTATION: ProofObligation = obligation(
    ProofObligationClass::Documentation,
    "public docs describe the domain contract",
);

pub(crate) const SOURCE_ONLY: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    DOCUMENTATION,
];

pub(crate) const CHECKER_LOWERING: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    CHECKED_IR_PROJECTION,
    STRATA_MANTLE_BOUNDARY,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    DOCUMENTATION,
];

pub(crate) const CHECKER_LOWERING_FUZZ: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    CHECKED_IR_PROJECTION,
    STRATA_MANTLE_BOUNDARY,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    FUZZ_SEED,
    DOCUMENTATION,
];

pub(crate) const CHECKER_LOWERING_FUZZ_BOUNDED: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    CHECKED_IR_PROJECTION,
    STRATA_MANTLE_BOUNDARY,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    FUZZ_SEED,
    BOUNDED_PROPERTY,
    DOCUMENTATION,
];

pub(crate) const RUNTIME: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    CHECKED_IR_PROJECTION,
    STRATA_MANTLE_BOUNDARY,
    ARTIFACT_ADMISSION,
    RUNTIME_EXECUTION,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    DOCUMENTATION,
];

pub(crate) const RUNTIME_FUZZ: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    CHECKED_IR_PROJECTION,
    STRATA_MANTLE_BOUNDARY,
    ARTIFACT_ADMISSION,
    RUNTIME_EXECUTION,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    FUZZ_SEED,
    DOCUMENTATION,
];

pub(crate) const RUNTIME_FUZZ_BOUNDED: &[ProofObligation] = &[
    SOURCE_SYNTAX,
    STATIC_VALIDATION,
    CHECKED_IR_PROJECTION,
    STRATA_MANTLE_BOUNDARY,
    ARTIFACT_ADMISSION,
    RUNTIME_EXECUTION,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    FUZZ_SEED,
    BOUNDED_PROPERTY,
    DOCUMENTATION,
];

pub(crate) const ARTIFACT_ADMISSION_ONLY: &[ProofObligation] = &[
    STRATA_MANTLE_BOUNDARY,
    ARTIFACT_ADMISSION,
    RUNTIME_EXECUTION,
    DIAGNOSTICS,
    TEST_COVERAGE,
    DOCUMENTATION,
];

pub(crate) const ARTIFACT_ADMISSION_BOUNDED: &[ProofObligation] = &[
    STRATA_MANTLE_BOUNDARY,
    ARTIFACT_ADMISSION,
    RUNTIME_EXECUTION,
    DIAGNOSTICS,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    BOUNDED_PROPERTY,
    DOCUMENTATION,
];

pub(crate) const OBSERVABILITY: &[ProofObligation] = &[
    RUNTIME_EXECUTION,
    DIAGNOSTICS,
    RUNNABLE_EXAMPLE,
    TEST_COVERAGE,
    SOURCE_TO_RUNTIME,
    FUZZ_SEED,
    BOUNDED_PROPERTY,
    DOCUMENTATION,
];

pub(crate) const REJECTED: &[ProofObligation] = &[DIAGNOSTICS, TEST_COVERAGE, DOCUMENTATION];

pub(crate) const DOCS_EXAMPLES: &[ProofObligation] = &[RUNNABLE_EXAMPLE, DOCUMENTATION];

const fn obligation(class: ProofObligationClass, claim: &'static str) -> ProofObligation {
    ProofObligation::new(class, claim)
}
