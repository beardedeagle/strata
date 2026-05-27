# Getting Started

This guide takes a clean checkout to a checked, built, and executed Strata
program.

## Prerequisites

Install stable Rust 1.85 or newer with Cargo. The workspace uses Rust Edition
2024. Standard repository gates select stable explicitly; nightly is reserved
for fuzz and Miri recipes that opt into it per command.

Useful local tools:

```sh
rustup toolchain install stable --profile minimal --component rustfmt,clippy
cargo +stable install just --version 1.50.0 --locked
cargo +stable install mdbook --version 0.5.2 --locked
cargo +stable install mdbook-mermaid --version 0.17.0 --locked
```

The `1.50.0` value above is the pinned `just` command runner version, not the
Rust compiler version.

The standard `just quality` gate runs documentation and metadata checks on every
local platform. Install `jq`, `xmllint`, `mdbook`, and `mdbook-mermaid` before
using that bundle.
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
`xmllint --version`, `mdbook --version`, and `mdbook-mermaid --version`.

Do not set a repository-wide nightly Rust override. Use `just install-fuzz-tools`
and `just install-miri-tools` only when running the nightly fuzz or Miri gates.

## Build The Binaries

From the repository root:

```sh
just build
```

This builds the Strata CLI and Mantle runtime CLI. The executable filenames are
platform-specific, so the commands below use repository `just` recipes to run
the right binary on the local platform.

## Check A Strata Program

`strata check` parses and semantically checks source without writing an
artifact.

```sh
just strata-check examples/hello.str
```

Expected result:

```text
strata: checked examples/hello.str (module hello, entry Main)
```

## Build A Mantle Artifact

`strata build` checks the source and writes a Mantle Target Artifact under
`target/strata/` by default.

```sh
just strata-build examples/hello.str
```

Expected result:

```text
strata: built examples/hello.str -> target/strata/hello.mta
```

## Run The Program

Mantle validates and executes the generated artifact:

```sh
just mantle-run target/strata/hello.mta
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

The trace path is important. It records what Mantle actually validated and
executed.

## Inspect Local Spawn Authority

For programs that declare local spawn authority, the optional inspection
commands show the checked source authority surface and the admitted artifact
authority tables without changing the normal build or run path:

```sh
just strata-authority-summary examples/actor_emit_spawn_send.str
just strata-build examples/actor_emit_spawn_send.str
just mantle-inspect-authority target/strata/actor_emit_spawn_send.mta
```

Pass `json` as the final recipe argument when CI or audit tooling needs
deterministic machine-readable output.

## Run The Standard Gate

After editing source, examples, runtime behavior, artifacts, or docs, use the
central automation:

```sh
just quality
```

For the source-to-runtime acceptance examples only:

```sh
just source-to-runtime-gates
```

For the bounded/property assurance smoke tests:

```sh
just bounded-assurance-smoke
```

For the docs only:

```sh
just docs
```

## What To Read Next

Read Language Concepts for the core ideas, then Tutorial: Hello for a guided
walkthrough of the smallest accepted program.
