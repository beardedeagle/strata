# Getting Started

This guide takes a clean checkout to a checked, built, and executed Strata
program.

## Prerequisites

Install a Rust toolchain with Cargo. The repository automation uses `just`, but
the first commands can also be run directly with Cargo-built binaries.

Useful local tools:

```sh
cargo install just --version 1.50.0 --locked
cargo install mdbook --version 0.5.2 --locked
```

The standard `just quality` gate runs documentation and metadata checks on every
local platform. Install `jq`, `xmllint`, and `mdbook` before using that bundle.
Ubuntu-based environments can install the metadata tools with:

```sh
sudo apt-get install jq libxml2-utils
```

macOS systems with Homebrew can install the metadata tools with:

```sh
brew install jq libxml2
```

Windows systems should install `jq` and an `xmllint` provider such as libxml2
with their package manager, or run the full `just quality` bundle in a WSL
Ubuntu environment. Confirm the tools are on `PATH` with `jq --version`,
`xmllint --version`, and `mdbook --version`.

## Build The Binaries

From the repository root:

```sh
cargo build
```

This builds the Strata CLI and Mantle runtime CLI. The executable filenames are
platform-specific, so the commands below use Cargo to run the right binary on
the current platform.

## Check A Strata Program

`strata check` parses and semantically checks source without writing an
artifact.

```sh
cargo run -p strata --bin strata -- check examples/hello.str
```

Expected result:

```text
strata: checked examples/hello.str (module hello, entry Main)
```

## Build A Mantle Artifact

`strata build` checks the source and writes a Mantle Target Artifact under
`target/strata/` by default.

```sh
cargo run -p strata --bin strata -- build examples/hello.str
```

Expected result:

```text
strata: built examples/hello.str -> target/strata/hello.mta
```

## Run The Program

Mantle admits and executes the generated artifact:

```sh
cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
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
