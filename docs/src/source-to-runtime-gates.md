# Source-To-Runtime Gates

A runtime-bearing milestone is not complete until the command a user would run
succeeds. Documentation and generated artifacts do not replace executable
source-to-runtime behavior.

The normal gate shape is:

```text
.str source -> strata check -> strata build -> mantle run -> trace
```

For fail-closed runtime behavior, the source must check and build, and
`mantle run` must fail only after Mantle validates the artifact and emits trace
evidence for the failure. Source-level rejection gates are different: they must
fail before build/lowering and must not leave a target artifact behind.

## Canonical Gates

The canonical user-facing gate is:

```sh
just source-to-runtime-gates
```

The split entrypoints are available when only one side of the gate is needed:

```sh
just source-to-runtime-success-gates
just source-to-runtime-failure-gates
```

The maintained command list belongs in the executable gate definitions:

- `Justfile` owns the commands a user or CI runner executes.
- `crates/strata-mantle-acceptance/tests/source_to_runtime_gates.rs` is the root
  integration harness, with focused gate families under
  `crates/strata-mantle-acceptance/tests/source_to_runtime_gates/`.
- `docs/src/examples.md` owns the curated list of runnable examples. The
  grouped example pages document the behavior each example demonstrates.

This page intentionally does not enumerate every runtime gate. When a new
user-visible language or runtime behavior needs source-to-runtime proof, update
the executable gate and document the example in the grouped example pages.

The language surface proof substrate checks that the agreed current-surface
proof domains map to declared features and that those features point at the
required parser, checker, lowering, boundary, artifact, runtime, diagnostics,
example, test, fuzz, and bounded/property evidence classes where those classes
apply. It also checks that each proof domain declares the proof obligations
implied by those features. That substrate supports implementation claims; it
does not replace this executable gate shape.

## Representative Commands

The minimum success gate checks, builds, runs, and traces `hello.str`:

```sh
just run-example hello
```

A richer state/payload gate follows the same shape:

```sh
just run-example state_payload_match
```

An immutable source computation gate proves sequential source-local bindings are
resolved before lowering while Mantle executes only typed artifact data:

```sh
just run-example function_local_bindings
```

A scalar computation gate proves typed scalar payloads, immutable process-local
scalar functions, runtime scalar predicates, runtime-bound value conditionals,
and Mantle execution of typed scalar templates:

```sh
just run-example runtime_scalar_priority
```

A selected return-match arm-prefix gate proves source-selected dispatch before
Mantle executes typed runtime arm actions:

```sh
just run-example process_return_match_arm_runtime_if_prefix
just run-example process_return_match_arm_for_prefix
just run-example process_return_match_arm_for_if_prefix
just run-example process_return_match_arm_if_for_prefix
```

A runtime control-flow gate checks, builds, validates, runs, and traces typed
Mantle execution:

```sh
just run-example runtime_guard_noop
just run-example runtime_scalar_priority
just run-example runtime_nested_if_actions
just run-example runtime_final_if_guarded_loop
just run-example runtime_final_if_nested_if_actions
just run-example runtime_final_if_nested_terminal_if
just run-example runtime_for_each
just run-example runtime_for_each_empty
just run-example runtime_for_each_if
just run-example runtime_for_each_nested_if_actions
just run-example runtime_guarded_for_each
just run-example runtime_guarded_ref_loop
just run-example runtime_guarded_ref_loop_jobs
just run-example runtime_loop_element_projection
```

A source rejection gate must fail during checking and must not create a target
artifact:

```sh
just strata-check examples/failures/effect_authority_missing.str
just strata-check examples/failures/source_local_binding_process_ref_carrier_enum.str
just strata-check examples/failures/scalar_overflow.str
just strata-check examples/failures/scalar_type_mismatch.str
just strata-check examples/failures/scalar_divide_by_zero.str
just strata-check examples/failures/scalar_runtime_divide_by_zero.str
just strata-check examples/failures/scalar_runtime_modulo_by_zero.str
just strata-check examples/failures/scalar_unsuffixed_literal.str
```

A runtime fail-closed gate checks and builds successfully, then returns non-zero
from Mantle after writing trace evidence:

```sh
just run-example actor_panic_no_replay
```

Each successful `mantle run` command must validate the generated `.mta`, execute it,
and emit an observability trace under `target/strata/`. Expected-failure gates
must return non-zero with source diagnostics or runtime failure evidence at the
layer being tested.

When adding a new user-visible language or runtime behavior, add or update an
example that follows this shape. A passing unit test is useful, but it does not
replace a runnable source-to-runtime command when the behavior is user-facing.
