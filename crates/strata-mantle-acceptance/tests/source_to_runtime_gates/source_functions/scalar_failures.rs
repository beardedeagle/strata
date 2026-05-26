use crate::support::*;

struct ScalarFailureCase {
    source: &'static str,
    artifact: &'static str,
    diagnostic: &'static str,
}

#[test]
fn scalar_failure_examples_check_fail_closed_without_artifacts() {
    let gate = GateHarness::new();
    for case in [
        ScalarFailureCase {
            source: "examples/failures/scalar_overflow.str",
            artifact: "target/strata/scalar_overflow.mta",
            diagnostic: "scalar arithmetic result 256 is outside U8 range",
        },
        ScalarFailureCase {
            source: "examples/failures/scalar_type_mismatch.str",
            artifact: "target/strata/scalar_type_mismatch.mta",
            diagnostic: "right operand of + must produce U32: scalar literal 2_u64 has type U64, expected U32",
        },
        ScalarFailureCase {
            source: "examples/failures/scalar_divide_by_zero.str",
            artifact: "target/strata/scalar_divide_by_zero.mta",
            diagnostic: "scalar division by zero",
        },
        ScalarFailureCase {
            source: "examples/failures/scalar_runtime_divide_by_zero.str",
            artifact: "target/strata/scalar_runtime_divide_by_zero.mta",
            diagnostic: "scalar division by zero",
        },
        ScalarFailureCase {
            source: "examples/failures/scalar_runtime_modulo_by_zero.str",
            artifact: "target/strata/scalar_runtime_modulo_by_zero.mta",
            diagnostic: "scalar modulo by zero",
        },
        ScalarFailureCase {
            source: "examples/failures/scalar_unsuffixed_literal.str",
            artifact: "target/strata/scalar_unsuffixed_literal.mta",
            diagnostic: "numeric value literals require an explicit scalar suffix",
        },
    ] {
        gate.remove_artifact(case.artifact);
        let check = gate.check_failure(case.source);
        let stderr = String::from_utf8_lossy(&check.stderr);
        assert!(
            stderr.contains(case.diagnostic),
            "unexpected diagnostic for {}\nstderr:\n{stderr}",
            case.source
        );
        assert!(
            !gate.root.join(case.artifact).exists(),
            "source check failure must not create {}",
            case.artifact
        );
    }
}
