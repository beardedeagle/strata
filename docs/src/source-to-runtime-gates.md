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
evidence for the failure.

The source-to-runtime gates are:

```sh
cargo build

cargo run -p strata --bin strata -- check examples/hello.str
cargo run -p strata --bin strata -- build examples/hello.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta

cargo run -p strata --bin strata -- check examples/actor_ping.str
cargo run -p strata --bin strata -- build examples/actor_ping.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_ping.mta

cargo run -p strata --bin strata -- check examples/actor_sequence.str
cargo run -p strata --bin strata -- build examples/actor_sequence.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta

cargo run -p strata --bin strata -- check examples/actor_match.str
cargo run -p strata --bin strata -- build examples/actor_match.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_match.mta

cargo run -p strata --bin strata -- check examples/actor_instances.str
cargo run -p strata --bin strata -- build examples/actor_instances.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_instances.mta

cargo run -p strata --bin strata -- check examples/actor_payloads.str
cargo run -p strata --bin strata -- build examples/actor_payloads.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_payloads.mta

cargo run -p strata --bin strata -- check examples/actor_reply.str
cargo run -p strata --bin strata -- build examples/actor_reply.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_reply.mta

cargo run -p strata --bin strata -- check examples/actor_emit_spawn_send.str
cargo run -p strata --bin strata -- build examples/actor_emit_spawn_send.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_emit_spawn_send.mta

# Expected to fail during source checking before build/lowering.
cargo run -p strata --bin strata -- check examples/failures/effect_authority_missing.str

cargo run -p strata --bin strata -- check examples/actor_panic_no_replay.str
cargo run -p strata --bin strata -- build examples/actor_panic_no_replay.str
# Expected to fail closed after writing actor_panic_no_replay.observability.jsonl.
cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_panic_no_replay.mta
```

Each `mantle run` command must admit the generated `.mta`, execute it, and emit
an observability trace under `target/strata/`. Expected-failure gates must
return non-zero with failure evidence in the trace.

The source-to-runtime gate integration tests in
`crates/strata-mantle-acceptance/tests/source_to_runtime_gates.rs` mirror this
user-facing sequence and should stay aligned with the examples. They live
outside Mantle-owned crates because these gates prove the Strata/Mantle
execution path, not Mantle runtime ownership of Strata source behavior.

When adding a new user-visible language or runtime behavior, add or update an
example that follows this shape. A passing unit test is useful, but it does not
replace a runnable source-to-runtime command when the behavior is user-facing.
