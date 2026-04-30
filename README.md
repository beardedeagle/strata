# Strata

Strata is an experimental systems language for explicit authority, typed
concurrency, and runtime-visible execution.

Mantle is the runtime target for Strata programs. Strata source files are
written as `.str`; the compiler builds language-neutral Mantle Target Artifacts
as `.mta`; Mantle validates and executes those artifacts.

The project is early, but the shape is deliberate: Strata should make effects,
authority, process behavior, determinism, and communication protocols visible in
the program text and checkable before execution. Mantle should execute only what
the artifact is allowed to do, and it should leave an observability trail that
can be inspected after the run.

## Current Status

The first runnable product gate is in place:

```sh
cargo build
target/debug/strata check examples/hello.str
target/debug/strata build examples/hello.str
target/debug/mantle run target/strata/hello.mta
```

The example program emits `hello from Strata` through an explicit `emit` effect.
Mantle prints the emitted output and records the runtime events in:

```text
target/strata/hello.observability.jsonl
```

This is not yet a complete language or a production runtime. It is the first
source-to-runtime slice: a real `.str` file can be checked, built into `.mta`,
and executed by Mantle.

## What Strata Is For

Strata is aimed at programs where the important behavior should be explicit:

- which effects a function may perform;
- which authority a process or component may exercise;
- which messages a process accepts and emits;
- which state transitions are valid;
- which operations must be deterministic;
- which protocols govern local or distributed communication.

The goal is not just to run code. The goal is to make runtime behavior part of
the checked interface of the program.

## What Mantle Is For

Mantle is the execution layer. Its job is to validate and run `.mta` artifacts,
manage processes and mailboxes, dispatch approved effects, supervise failures,
and emit runtime evidence.

The `.mta` format is intentionally language-neutral. Strata is the first
frontend, but Mantle should remain a stable target for other frontends that want
the same runtime semantics.

## Design Principles

- Source first: `.str` is the authoring surface.
- Explicit effects: undeclared side effects should be rejected.
- Explicit authority: runtime capability use should be visible and checked.
- Runtime evidence: execution-bearing milestones should produce traces.
- Fail closed: invalid artifacts, unsupported authority, and unsafe runtime
  states should be rejected rather than silently widened.
- Language-neutral runtime artifacts: Mantle artifacts identify their format,
  version, and source language internally.
- Corpus matters: examples, libraries, fixtures, and conformance cases are part
  of the product, not an afterthought.

## Corpus And Libraries

New languages do not succeed on syntax alone. They need a durable body of high
quality code: examples, standard patterns, libraries, tests, rejection cases,
runtime traces, and migration guides.

Strata and Mantle will therefore grow in two directions:

- native Strata programs and libraries that show the language as it is intended
  to be written;
- companion Rust crates that expose Mantle-oriented ideas where using an
  existing language is the right engineering path.

Those two tracks should reinforce each other. Rust libraries can make the
runtime semantics useful earlier, while Strata examples and libraries build the
idiomatic corpus needed for the language itself.

## Project Direction

The next milestones are expected to expand the current vertical slice into a
usable MVP:

- richer `.str` parsing and diagnostics;
- actors/processes with typed mailboxes;
- message send and receive behavior;
- process state updates;
- normal termination and failure reporting;
- explicit effect checking beyond the current `emit` slice;
- Mantle runtime traces that prove execution happened inside the runtime;
- conformance tests and example programs that double as corpus material.

Longer term, Strata and Mantle are intended to cover typed distribution,
supervision, capability-aware runtime behavior, artifact validation, upgrade
coordination, and reproducible publication.

## File Types

- `.str` files are Strata source files.
- `.mta` files are Mantle Target Artifacts.

See [docs/file-types.md](docs/file-types.md) for the source/artifact boundary,
MIME identifiers, and tooling notes.

## Repository Layout

```text
examples/                 runnable Strata examples
src/bin/strata.rs          Strata CLI entrypoint
src/bin/mantle.rs          Mantle CLI entrypoint
src/language.rs            current Strata parser and checker slice
src/artifact.rs            Mantle Target Artifact encode/decode/validation
src/runtime.rs             local Mantle runtime slice
tests/first_gate.rs        source-to-runtime acceptance test
tools/                     editor and MIME metadata
```

## Development

Run the current verification bundle:

```sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Run the product gate manually:

```sh
cargo build
target/debug/strata check examples/hello.str
target/debug/strata build examples/hello.str
target/debug/mantle run target/strata/hello.mta
```
