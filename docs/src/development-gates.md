# Development Gates

Repository automation is centralized in `Justfile`. GitHub Actions and
lefthook delegate to the same recipes used locally.

The standard local verification bundle is:

```sh
just quality
```

This bundle selects stable Rust explicitly through the `Justfile`. The workspace
requires Rust 1.85 or newer for Edition 2024. The bundle also runs `just
toolchain-policy-check`, which rejects repository instructions or workflow
steps that switch the whole checkout to nightly Rust. Nightly-only tools must be
invoked per command with `+nightly`.

Run the source-to-runtime product gates after changes that affect syntax,
checking, lowering, artifacts, runtime behavior, diagnostics, examples, or
acceptance criteria.

```sh
just product-gates
```

## Continuous Integration

The standard CI workflow installs `just` and calls `just ci-rust` on Linux,
macOS, and Windows. The Linux quality job calls `just ci-quality`, which runs
formatting, check, tests, clippy, build, tool metadata validation, toolchain
policy validation, mdBook, product gates, and diff hygiene.

CI uses GitHub-owned, SHA-pinned checkout and cache actions. The cache stores
Cargo registry/git data and per-job build target directories. It does not cache
installed executable tools directly; tool installs remain version-pinned and
reuse the cached Cargo target directory where possible.

For local Linux CI parity through `act`:

```sh
just ci-local
```

## Fuzzing

The fuzz harnesses live under `fuzz/` and run with `cargo-fuzz` on nightly Rust.
They cover three initial boundaries:

- parsing, checking, and lowering arbitrary UTF-8 source;
- decoding and re-encoding arbitrary UTF-8 artifact text;
- running valid lowered artifacts through the in-memory runtime host.

Useful local commands:

```sh
just install-fuzz-tools
just fuzz-ci
```

## Miri

Miri runs on nightly Rust. The Miri gate is a smoke suite focused on pure or
in-memory paths rather than filesystem-specific CLI behavior.

Useful local commands:

```sh
just install-miri-tools
just miri-ci
```

Every slice that changes user-facing syntax, artifact schema, runtime behavior,
diagnostics, examples, or acceptance gates should update this book and pass
`mdbook build docs`.
