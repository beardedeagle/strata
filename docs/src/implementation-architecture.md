# Implementation Architecture

This page maps the current source-to-runtime implementation for contributors.
It is not required for writing simple Strata programs.

## Crate Layout

| Path | Responsibility |
| --- | --- |
| `crates/strata` | Strata CLI, parser, AST, checker, checked IR, and lowering. |
| `crates/mantle-artifact` | Mantle Target Artifact encode, decode, validation, limits, and typed artifact IDs. |
| `crates/mantle-runtime` | Mantle admission, runtime process state, mailboxes, dispatch, output, and traces. |
| `examples` | Runnable Strata programs used by source-to-runtime gates. |
| `fuzz` | Fuzz targets for parser/checker/lowering, artifact decode, and runtime admission paths. |
| `tools` | Editor and MIME metadata. |

## Source Path

```text
source text
  -> lexer
  -> parser
  -> AST
  -> semantic checker
  -> checked IR
  -> lowering
  -> Mantle Target Artifact
```

The parser accepts source shape. The checker assigns source-visible meaning,
resolves names, validates process/message/state rules, and produces checked IR.
Lowering converts checked IR into Mantle artifact tables.

## Runtime Path

```text
artifact text
  -> decode
  -> validate
  -> load typed runtime tables
  -> spawn Main
  -> deliver entry message
  -> dispatch by message ID
  -> execute actions
  -> write JSONL trace
```

Mantle must validate artifacts before execution. Runtime dispatch uses loaded
typed IDs, not source strings.

## Important Boundaries

Strata owns:

- source syntax;
- diagnostics;
- AST;
- semantic checking;
- checked IR;
- source-visible meaning.

Lowering owns:

- conversion from checked Strata IR into Mantle artifact records;
- mapping checked process, message, state, and output IDs to artifact IDs.

Mantle owns:

- artifact decoding and validation;
- admitted runtime tables;
- process instances and mailboxes;
- action execution;
- runtime traces.

Do not move source-only assumptions into Mantle as trusted runtime behavior.
Do not make Mantle dispatch through labels that exist only for diagnostics or
trace readability.

## Adding A Language Feature

A source-facing feature usually needs changes across several layers:

- parser;
- AST;
- checker;
- checked IR;
- lowering;
- artifact schema or validation, when runtime representation changes;
- runtime execution, when behavior changes;
- diagnostics;
- examples;
- docs;
- positive tests;
- negative tests;
- source-to-runtime gates, when user-visible execution changes.

Parser acceptance alone is not enough. If another layer can construct or admit
the same invalid state, that layer needs its own validation.

## Existing Product Gates

The current runnable examples are:

- `examples/hello.str`;
- `examples/actor_ping.str`;
- `examples/actor_sequence.str`.

The integration tests in `crates/mantle-runtime/tests/product_gates.rs` mirror
the same source check, artifact build, and runtime execution sequence:

```sh
cargo run -p strata --bin strata -- check examples/hello.str
cargo run -p strata --bin strata -- build examples/hello.str
cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
```

The same pattern applies to the actor examples.

## Closure Rule

A change that affects source syntax, artifact schema, runtime behavior,
diagnostics, examples, or gates should update this book and pass:

```sh
just quality
```

Docs explain the contract; runnable gates prove it still works.
