use crate::model::ProofDomain;

pub(crate) const DOMAINS: &[ProofDomain] = &[
    ProofDomain {
        id: "module-declarations-and-top-level-items",
        title: "Module declarations and top-level item parsing",
        feature_ids: &["source-unit-top-level-items"],
    },
    ProofDomain {
        id: "records-enums-fieldless-and-payload-variants",
        title: "Records, enums, and fieldless or payload-bearing variants",
        feature_ids: &["records-enums-immutable-values", "typed-message-payloads"],
    },
    ProofDomain {
        id: "pure-source-functions",
        title: "Pure source functions",
        feature_ids: &["pure-source-functions-and-calls"],
    },
    ProofDomain {
        id: "source-function-calls",
        title: "Source function calls",
        feature_ids: &["pure-source-functions-and-calls"],
    },
    ProofDomain {
        id: "source-function-braced-return-if",
        title: "Source function braced return-if selection",
        feature_ids: &["source-function-return-if"],
    },
    ProofDomain {
        id: "source-function-return-match",
        title: "Source function return-match selection",
        feature_ids: &["source-function-return-match"],
    },
    ProofDomain {
        id: "function-whole-body-match",
        title: "Function whole-body match selection",
        feature_ids: &["source-function-whole-body-match"],
    },
    ProofDomain {
        id: "record-list-map-enum-pattern-destructuring",
        title: "Record, list, map, and enum pattern destructuring",
        feature_ids: &[
            "source-function-record-patterns",
            "source-function-list-map-patterns",
            "source-patterns-and-return-match",
            "step-parameter-patterns",
            "whole-body-match-msg",
            "whole-body-match-state",
        ],
    },
    ProofDomain {
        id: "static-map-key-behavior",
        title: "Static map-key behavior",
        feature_ids: &[
            "static-map-keys-and-collections",
            "rejected-dynamic-map-keys",
        ],
    },
    ProofDomain {
        id: "bool-equality-predicate-value-forms",
        title: "Bool, equality, and predicate value forms",
        feature_ids: &["bool-equality-predicates"],
    },
    ProofDomain {
        id: "process-declarations",
        title: "Process declarations",
        feature_ids: &["process-init-step-results"],
    },
    ProofDomain {
        id: "init-and-step",
        title: "Process init and step bodies",
        feature_ids: &[
            "process-init-step-results",
            "init-whole-body-match",
            "init-return-match",
        ],
    },
    ProofDomain {
        id: "typed-message-payloads",
        title: "Typed message payloads",
        feature_ids: &["typed-message-payloads"],
    },
    ProofDomain {
        id: "process-instances-and-spawn-routing",
        title: "Process instances and spawn routing",
        feature_ids: &["process-instances-and-spawn-routing"],
    },
    ProofDomain {
        id: "step-parameter-patterns",
        title: "Step parameter patterns",
        feature_ids: &["step-parameter-patterns"],
    },
    ProofDomain {
        id: "whole-body-match-msg",
        title: "Whole-body match msg dispatch",
        feature_ids: &["whole-body-match-msg"],
    },
    ProofDomain {
        id: "whole-body-match-state",
        title: "Whole-body match state dispatch",
        feature_ids: &["whole-body-match-state"],
    },
    ProofDomain {
        id: "step-return-match",
        title: "Step return-match dispatch and selected-arm action blocks",
        feature_ids: &["step-return-match", "return-match-action-blocks"],
    },
    ProofDomain {
        id: "message-dispatch-patterns",
        title: "Message dispatch patterns across admitted dispatch surfaces",
        feature_ids: &[
            "message-dispatch-patterns",
            "step-parameter-patterns",
            "whole-body-match-msg",
            "whole-body-match-state",
            "step-return-match",
        ],
    },
    ProofDomain {
        id: "terminal-continue-stop-panic",
        title: "Terminal Continue, Stop, and Panic results",
        feature_ids: &["process-init-step-results"],
    },
    ProofDomain {
        id: "explicit-effects",
        title: "Explicit effect declarations and authority checks",
        feature_ids: &["explicit-effects-and-authority"],
    },
    ProofDomain {
        id: "emit-spawn-send",
        title: "Emit, spawn, and send action effects",
        feature_ids: &["explicit-effects-and-authority"],
    },
    ProofDomain {
        id: "typed-direct-process-ref-authority",
        title: "Typed direct ProcessRef authority",
        feature_ids: &["direct-process-ref-authority"],
    },
    ProofDomain {
        id: "process-ref-payload-forwarding",
        title: "Process-reference payload forwarding",
        feature_ids: &[
            "direct-process-ref-authority",
            "runtime-for-received-ref-routing",
        ],
    },
    ProofDomain {
        id: "runtime-if",
        title: "Runtime if control flow and projections",
        feature_ids: &[
            "runtime-if-control-flow",
            "runtime-if-payload-projection",
            "runtime-if-current-state-projection",
            "runtime-if-final-next-state",
            "runtime-if-noop-branches",
            "runtime-if-nested-action-branches",
        ],
    },
    ProofDomain {
        id: "runtime-for-over-checked-list",
        title: "Runtime for over checked List<T,N> values",
        feature_ids: &[
            "runtime-for-checked-lists",
            "runtime-for-loop-branching",
            "runtime-for-guarded-loops",
            "runtime-for-received-ref-routing",
            "runtime-for-loop-element-projection",
        ],
    },
    ProofDomain {
        id: "checked-ir-action-templates",
        title: "Checked IR action templates and typed lowering",
        feature_ids: &[
            "checked-ir-and-typed-id-boundary",
            "return-match-action-blocks",
            "runtime-if-control-flow",
            "runtime-for-checked-lists",
        ],
    },
    ProofDomain {
        id: "mantle-artifact-admission",
        title: "Mantle artifact admission",
        feature_ids: &["mantle-artifact-admission"],
    },
    ProofDomain {
        id: "mantle-loaded-artifact-validation",
        title: "Mantle runtime loaded-artifact validation",
        feature_ids: &["loaded-runtime-validation"],
    },
    ProofDomain {
        id: "runtime-trace-observability-boundaries",
        title: "Runtime trace and observability boundaries",
        feature_ids: &["runtime-observability"],
    },
    ProofDomain {
        id: "rejection-fail-closed-unsupported-forms",
        title: "Fail-closed rejection cases for unsupported forms",
        feature_ids: &[
            "rejected-source-mutation-and-statements",
            "rejected-general-match-expressions",
            "rejected-step-return-match-state-scrutinee",
            "rejected-init-return-match-payload-binding",
            "rejected-source-function-loops-and-statements",
            "rejected-return-match-nested-loops-and-depth",
            "rejected-nested-process-ref-payloads",
            "rejected-dynamic-map-keys",
            "rejected-unbounded-collections",
        ],
    },
    ProofDomain {
        id: "docs-examples-only-surfaces",
        title: "Documentation and example-only surfaces",
        feature_ids: &["source-to-runtime-documentation-index"],
    },
];
