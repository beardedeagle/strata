# Source-To-Runtime Gates

A runtime-bearing milestone is not complete until the command a user would run
succeeds. Documentation and generated artifacts do not replace executable
source-to-runtime behavior.

The normal gate shape is:

```text
.str source -> strata check -> strata build -> mantle run -> trace
```

For fail-closed runtime behavior, the source must check and build, and
`mantle run` must fail only after Mantle admits the artifact and emits trace
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
- `docs/src/examples.md` owns the curated list of runnable examples and the
  behavior each one demonstrates.

This page intentionally does not enumerate every runtime gate. When a new
user-visible language or runtime behavior needs source-to-runtime proof, update
the executable gate and document the example on the examples page.

## Representative Commands

The minimum success gate checks, builds, runs, and traces `hello.str`:

```sh
cargo run -p strata --bin strata -- check examples/hello.str
cargo run -p strata --bin strata -- build examples/hello.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
```

A richer state/payload gate follows the same shape:

```sh
cargo run -p strata --bin strata -- check examples/state_payload_match.str
cargo run -p strata --bin strata -- build examples/state_payload_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/state_payload_match.mta
```

A runtime control-flow gate checks, builds, admits, runs, and traces typed
Mantle execution:

```sh
cargo run -p strata --bin strata -- check examples/runtime_guard_noop.str
cargo run -p strata --bin strata -- build examples/runtime_guard_noop.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guard_noop.mta
cargo run -p strata --bin strata -- check examples/runtime_nested_if_actions.str
cargo run -p strata --bin strata -- build examples/runtime_nested_if_actions.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_nested_if_actions.mta
cargo run -p strata --bin strata -- check examples/runtime_for_each.str
cargo run -p strata --bin strata -- build examples/runtime_for_each.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_for_each.mta
cargo run -p strata --bin strata -- check examples/runtime_for_each_empty.str
cargo run -p strata --bin strata -- build examples/runtime_for_each_empty.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_for_each_empty.mta
cargo run -p strata --bin strata -- check examples/runtime_for_each_if.str
cargo run -p strata --bin strata -- build examples/runtime_for_each_if.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_for_each_if.mta
cargo run -p strata --bin strata -- check examples/runtime_guarded_for_each.str
cargo run -p strata --bin strata -- build examples/runtime_guarded_for_each.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guarded_for_each.mta
cargo run -p strata --bin strata -- check examples/runtime_guarded_ref_loop.str
cargo run -p strata --bin strata -- build examples/runtime_guarded_ref_loop.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guarded_ref_loop.mta
cargo run -p strata --bin strata -- check examples/runtime_guarded_ref_loop_jobs.str
cargo run -p strata --bin strata -- build examples/runtime_guarded_ref_loop_jobs.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_guarded_ref_loop_jobs.mta
cargo run -p strata --bin strata -- check examples/runtime_loop_element_projection.str
cargo run -p strata --bin strata -- build examples/runtime_loop_element_projection.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/runtime_loop_element_projection.mta
```

A source rejection gate must fail during checking and must not create a target
artifact:

```sh
cargo run -p strata --bin strata -- check examples/failures/effect_authority_missing.str
```

A runtime fail-closed gate checks and builds successfully, then returns non-zero
from Mantle after writing trace evidence:

```sh
cargo run -p strata --bin strata -- check examples/actor_panic_no_replay.str
cargo run -p strata --bin strata -- build examples/actor_panic_no_replay.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_panic_no_replay.mta
```

Each successful `mantle run` command must admit the generated `.mta`, execute it,
and emit an observability trace under `target/strata/`. Expected-failure gates
must return non-zero with source diagnostics or runtime failure evidence at the
layer being tested.

When adding a new user-visible language or runtime behavior, add or update an
example that follows this shape. A passing unit test is useful, but it does not
replace a runnable source-to-runtime command when the behavior is user-facing.
