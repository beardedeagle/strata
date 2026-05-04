# Source-To-Runtime Gates

A runtime-bearing milestone is not complete until the command a user would run
succeeds. Documentation and generated artifacts do not replace executable
source-to-runtime behavior.

The gate shape is always:

```text
.str source -> strata check -> strata build -> mantle run -> trace
```

The current product gates are:

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
```

Each `mantle run` command must admit the generated `.mta`, execute it, and emit
an observability trace under `target/strata/`.

The product-gate integration tests in
`crates/mantle-runtime/tests/product_gates.rs` mirror this user-facing sequence
and should stay aligned with the examples.

When adding a new user-visible language or runtime behavior, add or update an
example that follows this shape. A passing unit test is useful, but it does not
replace a runnable source-to-runtime command when the behavior is user-facing.
