#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FeatureStatus {
    Current,
    DocumentationOnly,
    FutureOrRejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SurfaceLayer {
    SourceOnly,
    CheckerLowering,
    ArtifactAdmission,
    RuntimeBehavior,
    DocsExamplesOnly,
    FutureNonAdmitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum EvidenceClass {
    ParserCoverage,
    CheckerValidation,
    CheckedIrLowering,
    ArtifactAdmission,
    RuntimeExecution,
    Diagnostics,
    RunnableExample,
    PositiveTest,
    NegativeTest,
    SourceToRuntimeGate,
    FuzzSeed,
    BoundedOrProperty,
    Documentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ProofObligationClass {
    SourceSyntax,
    StaticValidation,
    CheckedIrProjection,
    StrataMantleBoundary,
    ArtifactAdmission,
    RuntimeExecution,
    Diagnostics,
    RunnableExample,
    TestCoverage,
    SourceToRuntimeExecution,
    FuzzSeedCorpus,
    BoundedOrProperty,
    Documentation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Evidence {
    pub(crate) class: EvidenceClass,
    pub(crate) path: &'static str,
    pub(crate) marker: &'static str,
}

impl Evidence {
    pub(crate) const fn new(
        class: EvidenceClass,
        path: &'static str,
        marker: &'static str,
    ) -> Self {
        Self {
            class,
            path,
            marker,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProofObligation {
    pub(crate) class: ProofObligationClass,
    pub(crate) claim: &'static str,
}

impl ProofObligation {
    pub(crate) const fn new(class: ProofObligationClass, claim: &'static str) -> Self {
        Self { class, claim }
    }
}

#[derive(Debug)]
pub(crate) struct Feature {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) status: FeatureStatus,
    pub(crate) layer: SurfaceLayer,
    pub(crate) required: &'static [EvidenceClass],
    pub(crate) evidence: &'static [Evidence],
}

#[derive(Debug)]
pub(crate) struct ProofDomain {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) feature_ids: &'static [&'static str],
    pub(crate) obligations: &'static [ProofObligation],
}

pub(crate) mod requirements {
    use super::EvidenceClass;

    pub(crate) const SOURCE_SYNTAX_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::Documentation,
    ];

    pub(crate) const CHECKER_LOWERING_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::Documentation,
    ];

    pub(crate) const CHECKER_LOWERING_FUZZ_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::FuzzSeed,
        EvidenceClass::Documentation,
    ];

    pub(crate) const CHECKER_LOWERING_FUZZ_BOUNDED_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::FuzzSeed,
        EvidenceClass::BoundedOrProperty,
        EvidenceClass::Documentation,
    ];

    pub(crate) const RUNTIME_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::ArtifactAdmission,
        EvidenceClass::RuntimeExecution,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::Documentation,
    ];

    pub(crate) const RUNTIME_FUZZ_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::ArtifactAdmission,
        EvidenceClass::RuntimeExecution,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::FuzzSeed,
        EvidenceClass::Documentation,
    ];

    pub(crate) const RUNTIME_FUZZ_BOUNDED_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ParserCoverage,
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::ArtifactAdmission,
        EvidenceClass::RuntimeExecution,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::FuzzSeed,
        EvidenceClass::BoundedOrProperty,
        EvidenceClass::Documentation,
    ];

    pub(crate) const TYPED_BOUNDARY_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::CheckerValidation,
        EvidenceClass::CheckedIrLowering,
        EvidenceClass::Diagnostics,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::Documentation,
    ];

    pub(crate) const BOUNDARY_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::ArtifactAdmission,
        EvidenceClass::RuntimeExecution,
        EvidenceClass::Diagnostics,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::Documentation,
    ];

    pub(crate) const RUNTIME_OBSERVABILITY_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::RuntimeExecution,
        EvidenceClass::Diagnostics,
        EvidenceClass::RunnableExample,
        EvidenceClass::PositiveTest,
        EvidenceClass::NegativeTest,
        EvidenceClass::SourceToRuntimeGate,
        EvidenceClass::Documentation,
    ];

    pub(crate) const REJECTED_SURFACE_REQUIREMENTS: &[EvidenceClass] = &[
        EvidenceClass::Diagnostics,
        EvidenceClass::NegativeTest,
        EvidenceClass::Documentation,
    ];

    pub(crate) const DOCS_EXAMPLES_REQUIREMENTS: &[EvidenceClass] =
        &[EvidenceClass::RunnableExample, EvidenceClass::Documentation];
}

pub(crate) mod expected {
    pub(crate) const CURRENT_FEATURE_IDS: &[&str] = &[
        "source-unit-top-level-items",
        "records-enums-immutable-values",
        "pure-source-functions-and-calls",
        "source-function-whole-body-match",
        "source-function-return-if",
        "source-function-return-match",
        "source-function-record-patterns",
        "source-function-list-map-patterns",
        "source-patterns-and-return-match",
        "static-map-keys-and-collections",
        "bool-equality-predicates",
        "process-init-step-results",
        "init-whole-body-match",
        "init-return-match",
        "step-parameter-patterns",
        "whole-body-match-msg",
        "whole-body-match-state",
        "step-return-match",
        "message-dispatch-patterns",
        "explicit-effects-and-authority",
        "typed-message-payloads",
        "process-instances-and-spawn-routing",
        "direct-process-ref-authority",
        "runtime-if-control-flow",
        "runtime-if-payload-projection",
        "runtime-if-current-state-projection",
        "runtime-if-final-next-state",
        "runtime-if-noop-branches",
        "runtime-if-nested-action-branches",
        "runtime-for-checked-lists",
        "runtime-for-loop-branching",
        "runtime-for-guarded-loops",
        "runtime-for-received-ref-routing",
        "runtime-for-loop-element-projection",
        "return-match-action-blocks",
        "checked-ir-and-typed-id-boundary",
        "mantle-artifact-admission",
        "loaded-runtime-validation",
        "runtime-observability",
    ];

    pub(crate) const DOCUMENTATION_ONLY_FEATURE_IDS: &[&str] =
        &["source-to-runtime-documentation-index"];

    pub(crate) const FUTURE_OR_REJECTED_FEATURE_IDS: &[&str] = &[
        "rejected-source-mutation-and-statements",
        "rejected-general-match-expressions",
        "rejected-step-return-match-state-scrutinee",
        "rejected-init-return-match-payload-binding",
        "rejected-source-function-loops-and-statements",
        "rejected-return-match-nested-loops-and-depth",
        "rejected-nested-process-ref-payloads",
        "rejected-dynamic-map-keys",
        "rejected-unbounded-collections",
    ];

    pub(crate) const PROOF_DOMAIN_IDS: &[&str] = &[
        "module-declarations-and-top-level-items",
        "records-enums-fieldless-and-payload-variants",
        "pure-source-functions",
        "source-function-calls",
        "source-function-braced-return-if",
        "source-function-return-match",
        "function-whole-body-match",
        "record-list-map-enum-pattern-destructuring",
        "static-map-key-behavior",
        "bool-equality-predicate-value-forms",
        "process-declarations",
        "init-and-step",
        "typed-message-payloads",
        "process-instances-and-spawn-routing",
        "step-parameter-patterns",
        "whole-body-match-msg",
        "whole-body-match-state",
        "step-return-match",
        "message-dispatch-patterns",
        "terminal-continue-stop-panic",
        "explicit-effects",
        "emit-spawn-send",
        "typed-direct-process-ref-authority",
        "process-ref-payload-forwarding",
        "runtime-if",
        "runtime-for-over-checked-list",
        "checked-ir-action-templates",
        "mantle-artifact-admission",
        "mantle-loaded-artifact-validation",
        "runtime-trace-observability-boundaries",
        "rejection-fail-closed-unsupported-forms",
        "docs-examples-only-surfaces",
    ];
}

impl EvidenceClass {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            EvidenceClass::ParserCoverage => "parser",
            EvidenceClass::CheckerValidation => "checker",
            EvidenceClass::CheckedIrLowering => "checked-ir/lowering",
            EvidenceClass::ArtifactAdmission => "artifact-admission",
            EvidenceClass::RuntimeExecution => "runtime-execution",
            EvidenceClass::Diagnostics => "diagnostics",
            EvidenceClass::RunnableExample => "example",
            EvidenceClass::PositiveTest => "positive-test",
            EvidenceClass::NegativeTest => "negative-test",
            EvidenceClass::SourceToRuntimeGate => "source-to-runtime-gate",
            EvidenceClass::FuzzSeed => "fuzz-seed",
            EvidenceClass::BoundedOrProperty => "bounded/property",
            EvidenceClass::Documentation => "documentation",
        }
    }

    pub(crate) fn is_docs_only(self) -> bool {
        matches!(
            self,
            EvidenceClass::RunnableExample | EvidenceClass::Documentation
        )
    }

    pub(crate) fn is_rejected_surface_evidence(self) -> bool {
        matches!(
            self,
            EvidenceClass::Diagnostics | EvidenceClass::NegativeTest | EvidenceClass::Documentation
        )
    }
}

impl ProofObligationClass {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProofObligationClass::SourceSyntax => "source-syntax",
            ProofObligationClass::StaticValidation => "static-validation",
            ProofObligationClass::CheckedIrProjection => "checked-ir-projection",
            ProofObligationClass::StrataMantleBoundary => "strata-mantle-boundary",
            ProofObligationClass::ArtifactAdmission => "artifact-admission",
            ProofObligationClass::RuntimeExecution => "runtime-execution",
            ProofObligationClass::Diagnostics => "diagnostics",
            ProofObligationClass::RunnableExample => "runnable-example",
            ProofObligationClass::TestCoverage => "test-coverage",
            ProofObligationClass::SourceToRuntimeExecution => "source-to-runtime-execution",
            ProofObligationClass::FuzzSeedCorpus => "fuzz-seed-corpus",
            ProofObligationClass::BoundedOrProperty => "bounded-property",
            ProofObligationClass::Documentation => "documentation",
        }
    }

    pub(crate) fn is_supported_by(self, evidence_classes: &impl EvidenceClassSet) -> bool {
        match self {
            ProofObligationClass::SourceSyntax => {
                evidence_classes.contains(EvidenceClass::ParserCoverage)
            }
            ProofObligationClass::StaticValidation => {
                evidence_classes.contains(EvidenceClass::CheckerValidation)
            }
            ProofObligationClass::CheckedIrProjection => {
                evidence_classes.contains(EvidenceClass::CheckedIrLowering)
            }
            ProofObligationClass::StrataMantleBoundary => {
                evidence_classes.contains(EvidenceClass::CheckedIrLowering)
                    || evidence_classes.contains(EvidenceClass::ArtifactAdmission)
            }
            ProofObligationClass::ArtifactAdmission => {
                evidence_classes.contains(EvidenceClass::ArtifactAdmission)
            }
            ProofObligationClass::RuntimeExecution => {
                evidence_classes.contains(EvidenceClass::RuntimeExecution)
            }
            ProofObligationClass::Diagnostics => {
                evidence_classes.contains(EvidenceClass::Diagnostics)
            }
            ProofObligationClass::RunnableExample => {
                evidence_classes.contains(EvidenceClass::RunnableExample)
            }
            ProofObligationClass::TestCoverage => {
                evidence_classes.contains(EvidenceClass::PositiveTest)
                    || evidence_classes.contains(EvidenceClass::NegativeTest)
            }
            ProofObligationClass::SourceToRuntimeExecution => {
                evidence_classes.contains(EvidenceClass::SourceToRuntimeGate)
            }
            ProofObligationClass::FuzzSeedCorpus => {
                evidence_classes.contains(EvidenceClass::FuzzSeed)
            }
            ProofObligationClass::BoundedOrProperty => {
                evidence_classes.contains(EvidenceClass::BoundedOrProperty)
            }
            ProofObligationClass::Documentation => {
                evidence_classes.contains(EvidenceClass::Documentation)
            }
        }
    }
}

pub(crate) trait EvidenceClassSet {
    fn contains(&self, class: EvidenceClass) -> bool;
}

impl EvidenceClassSet for std::collections::BTreeSet<EvidenceClass> {
    fn contains(&self, class: EvidenceClass) -> bool {
        std::collections::BTreeSet::contains(self, &class)
    }
}

impl SurfaceLayer {
    pub(crate) fn is_current_surface(self) -> bool {
        !matches!(
            self,
            SurfaceLayer::DocsExamplesOnly | SurfaceLayer::FutureNonAdmitted
        )
    }

    pub(crate) fn allows_evidence(self, class: EvidenceClass) -> bool {
        match self {
            SurfaceLayer::SourceOnly => matches!(
                class,
                EvidenceClass::ParserCoverage
                    | EvidenceClass::CheckerValidation
                    | EvidenceClass::Diagnostics
                    | EvidenceClass::RunnableExample
                    | EvidenceClass::PositiveTest
                    | EvidenceClass::NegativeTest
                    | EvidenceClass::FuzzSeed
                    | EvidenceClass::BoundedOrProperty
                    | EvidenceClass::Documentation
            ),
            SurfaceLayer::CheckerLowering => matches!(
                class,
                EvidenceClass::ParserCoverage
                    | EvidenceClass::CheckerValidation
                    | EvidenceClass::CheckedIrLowering
                    | EvidenceClass::Diagnostics
                    | EvidenceClass::RunnableExample
                    | EvidenceClass::PositiveTest
                    | EvidenceClass::NegativeTest
                    | EvidenceClass::SourceToRuntimeGate
                    | EvidenceClass::FuzzSeed
                    | EvidenceClass::BoundedOrProperty
                    | EvidenceClass::Documentation
            ),
            SurfaceLayer::ArtifactAdmission => matches!(
                class,
                EvidenceClass::ArtifactAdmission
                    | EvidenceClass::RuntimeExecution
                    | EvidenceClass::Diagnostics
                    | EvidenceClass::PositiveTest
                    | EvidenceClass::NegativeTest
                    | EvidenceClass::Documentation
            ),
            SurfaceLayer::RuntimeBehavior => true,
            SurfaceLayer::DocsExamplesOnly => class.is_docs_only(),
            SurfaceLayer::FutureNonAdmitted => class.is_rejected_surface_evidence(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EvidenceClass, SurfaceLayer};

    #[test]
    fn source_only_layer_rejects_runtime_and_boundary_evidence() {
        assert!(!SurfaceLayer::SourceOnly.allows_evidence(EvidenceClass::CheckedIrLowering));
        assert!(!SurfaceLayer::SourceOnly.allows_evidence(EvidenceClass::ArtifactAdmission));
        assert!(!SurfaceLayer::SourceOnly.allows_evidence(EvidenceClass::RuntimeExecution));
        assert!(!SurfaceLayer::SourceOnly.allows_evidence(EvidenceClass::SourceToRuntimeGate));
    }

    #[test]
    fn non_admitted_layers_reject_current_surface_evidence() {
        assert!(!SurfaceLayer::DocsExamplesOnly.allows_evidence(EvidenceClass::RuntimeExecution));
        assert!(!SurfaceLayer::FutureNonAdmitted.allows_evidence(EvidenceClass::PositiveTest));
        assert!(
            !SurfaceLayer::FutureNonAdmitted.allows_evidence(EvidenceClass::SourceToRuntimeGate)
        );
    }
}
