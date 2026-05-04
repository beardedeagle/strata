# Getting Started

This guide takes a clean checkout to a checked, built, and executed Strata
program.

## Prerequisites

Install a Rust toolchain with Cargo. The repository automation uses `just`, but
the first commands can also be run directly with Cargo-built binaries.

Useful local tools:

```sh
cargo install just --version 1.50.0 --locked
```

Linux documentation and metadata checks also use `jq`, `xmllint`, and `mdbook`.
The CI setup recipe installs those for Ubuntu-based jobs:

```sh
just install-ci-tools-linux
```

## Build The Binaries

From the repository root:

```sh
cargo build
```

This produces:

- `target/debug/strata`, the Strata CLI;
- `target/debug/mantle`, the Mantle runtime CLI.

## Check A Strata Program

`strata check` parses and semantically checks source without writing an
artifact.

```sh
target/debug/strata check examples/hello.str
```

Expected result:

```text
strata: checked examples/hello.str (module hello, entry Main)
```

## Build A Mantle Artifact

`strata build` checks the source and writes a Mantle Target Artifact under
`target/strata/` by default.

```sh
target/debug/strata build examples/hello.str
```

Expected result:

```text
strata: built examples/hello.str -> target/strata/hello.mta
```

## Run The Program

Mantle admits and executes the generated artifact:

```sh
target/debug/mantle run target/strata/hello.mta
```

Expected output includes:

```text
mantle: loaded target/strata/hello.mta
mantle: spawned Main pid=1
mantle: delivered Start to Main
hello from Strata
mantle: stopped Main normally
mantle: trace target/strata/hello.observability.jsonl
```

The trace path is important. It records what Mantle actually admitted and
executed.

## Run The Standard Gate

After editing source, examples, runtime behavior, artifacts, or docs, use the
central automation:

```sh
just quality
```

For the source-to-runtime acceptance examples only:

```sh
just product-gates
```

For the docs only:

```sh
just docs
```

## What To Read Next

Read Language Concepts for the core ideas, then Tutorial: Hello for a guided
walkthrough of the smallest accepted program.
