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

The quality bundle includes `just cfg-check`, which typechecks the workspace for
representative Linux, macOS, and Windows Rust targets through the repository
recipe. This keeps platform-specific and test-only `#[cfg]` paths covered by
real compiler evidence without changing source visibility or suppressing
rust-analyzer's weak inactive-code hints. If a target is missing locally,
install the reported target with `rustup target add --toolchain stable ...`.
Windows cfg-check coverage includes the Windows source-loading, artifact IO,
and trace path implementations that use reparse-point rejection and stable
opened-file metadata checks.

Run the source-to-runtime gates after changes that affect syntax,
checking, lowering, artifacts, runtime behavior, diagnostics, examples, or
acceptance criteria.

```sh
just source-to-runtime-gates
```

The success gates include the typed target-binding path for the component
composition example: `strata target-requirements`, `mantle
feature-declaration`, `mantle admit`, and `mantle run`. That path proves
requirement extraction, `mantle.feature_declaration.v5` rendering,
requirements-vs-declaration admission before runtime execution, and Mantle's
artifact-derived check that declared requirements cover the decoded runtime
constructs.
Trace-reading acceptance gates also validate the Mantle-owned JSONL trace
schema before using trace evidence. That validation is limited to observability
metadata, exact per-event field sets, required field shapes, grouped
payload/loop fields, `artifact_loaded` first/no-repeat ordering,
Mantle-contiguous spawned PID sequencing, u32 artifact typed-ID width, u16
branch path segments and bounded path length, non-entry spawn parent evidence,
runtime PID-to-process-ID correlation, supervisor-child restart causality, and
restart-window numeric bounds/coupling; executable behavior still comes from
checked IR, admitted `.mta` tables, and Mantle runtime dispatch.

## Language Surface Proof Substrate

`just language-surface-assurance` runs the executable current-language-surface
proof substrate. The inventory maps agreed proof domains to declared
Strata/Mantle features, maps those features to typed proof obligations, maps
those obligations to required evidence classes, and verifies that each evidence
pointer still names live repository content. Source-to-runtime evidence must
name active executable check/build/run gate coverage. This gate does not add
language behavior and does not replace `.str` source-to-runtime execution.

```sh
just language-surface-assurance
```

## Bounded Assurance

`just bounded-assurance-smoke` runs the focused bounded assurance surface for
the current bounded language surfaces. It covers immutable source-local binding
chains, bounded scalar expression trees, binding-expanded scalar equivalents,
scalar values in records, lists, and maps, and pure source value/return `if`
selection. It also covers selected `step return match`
action blocks. The gate compares checked IR shapes with
lowered Mantle artifact shapes, validates the artifact boundary, verifies typed
send IDs, checks terminal `Continue` / `Stop` / `Panic` lowering, runs an
explicit smaller bounded runtime execution generator through Mantle, and runs
bypass-mutation rejection tests for malformed artifact equivalents.

This is machine-checked bounded exhaustiveness for the named surface. It is not a
theorem-prover proof of the whole language surface.

## Continuous Integration

The standard CI workflow installs `just` and calls `just ci-rust` on Linux,
macOS, and Windows. The Linux quality job calls `just ci-quality`, which runs
formatting, native and cross-target checks, tests, clippy, performance smoke
checks, build, tool metadata validation, toolchain policy validation, mdBook,
the language surface proof substrate, source-to-runtime gates, and diff hygiene.
The docs tool install path pins both `mdbook` and the Mermaid preprocessor, so
Mermaid diagrams are built by the same `just docs` command locally, in quality
CI, and in the Pages workflow.

CI uses GitHub-owned, SHA-pinned checkout and cache actions. The cache stores
Cargo registry/git data and per-job build target directories. It does not cache
installed executable tools directly; tool installs remain version-pinned and
reuse the cached Cargo target directory where possible.

For local Linux CI parity through `act`:

```sh
just ci-local
```

## Performance Smoke

`just performance-smoke` runs stable source-to-runtime resource smoke profiles
over `examples/collection_state.str`, the multi-file source-unit import example,
the typed boundary-contract example, the checked component-composition example,
and the local supervision restart example. It measures repeated Strata checking
and lowering for collection state, source-unit imports, boundary contracts, and
component composition; repeated checked composition report rendering and target
requirement rendering from a prechecked composition input; Mantle admission
comparison through the runtime profiles; Mantle
artifact encode/decode; repeated Mantle in-memory execution of the
collection-state and boundary-contract artifacts; repeated
Mantle JSONL-trace execution of the collection-state artifact; and repeated
Mantle in-memory execution of the supervision artifact.
The Mantle runtime profiles exercise artifact admission, loaded-program
validation, internal executable-plan construction, typed dispatch, and host
execution together; they do not benchmark a serialized bytecode path.

The smoke output reports wall time, allocation/deallocation counts, allocated
and deallocated bytes, interval-relative live-byte metrics, process CPU time
when the platform exposes it, and resident memory. Allocation metrics come from
a test-only global allocator wrapper around `std::alloc::System`; the wrapper
does not change allocation policy and only records atomic counters. The live
metrics are relative to the start of each measured profile: net live-byte delta
is the signed end-of-interval live byte change, and peak live over interval start
is the highest live byte count above that start baseline. The net live-byte
delta budget is an upper bound; negative deltas remain valid. Linux CI reports
process CPU from `/proc/self/stat` and current/peak RSS through `/proc`; macOS
and BSD local runs report current RSS through `ps`. RSS budgets are enforced
against current RSS for each measured profile; process-lifetime peak RSS is
reported as context when available. Unlike allocation metrics, ordinary smoke
RSS is whole-test-process resident memory, so use it as a budget signal rather
than per-profile attribution.

The gate uses enforced resource ceilings with CI headroom for scheduler noise.
It is meant to catch meaningful regressions in compilation, runtime, CPU, or
memory paths without treating every small local timing fluctuation as a failure.

Reviewed reference values and enforced budget ceilings live in
`benchmarks/performance-smoke.baseline`. Local and CI runs print current
measurements to their logs; git tracks reviewed baseline changes, not every
noisy raw run. The baseline file uses strict `key=value` entries with
nanosecond, KiB, byte, and count units.

For RSS attribution, use the fresh-process comparison harness instead of the
ordinary smoke run. `just performance-rss-compare <base-worktree>
<current-worktree>` installs a temporary probe test into both worktrees, runs
one named profile per test process, repeats each profile, and reports median,
minimum, maximum, and p90 current-RSS deltas in KiB. This keeps profile-order,
test-binary, and allocator-retained-page effects separate from the ordinary
multi-profile smoke gate.

The default RSS comparison profiles cover collection-state checking/lowering,
collection-state in-memory runtime execution, and boundary-contract in-memory
runtime execution. To attribute local-supervision RSS, pass
`local_supervision_restart.in_memory_runtime` explicitly after the two
worktrees; both worktrees must contain `examples/local_supervision_restart.str`.

For review-grade memory evidence, use `just performance-memory-review
<base-worktree> <current-worktree>`. It runs both the fresh-process test-profile
RSS comparison and the product CLI footprint comparison. Treat allocator
metrics as the strict code-path signal. Treat RSS as a repeated median/p90
comparison signal because operating systems and allocators can keep freed pages
resident outside the measured source path.

For product-footprint attribution, use `just performance-cli-footprint-compare
<base-worktree> <current-worktree>`. It builds the release Strata and Mantle CLI
binaries in both worktrees, compares release binary file sizes, then repeats
fresh-process product commands for `strata check`, `strata build`, and
`mantle run` over `examples/collection_state.str` under a minimal environment.
The reported CLI RSS values come from the operating system process for each
command; they are closer to product footprint than test-harness RSS, but they
still are not per-Mantle-actor memory. The harness also writes metadata with the
compared worktrees, HEADs, platform, run count, and clean-environment policy.

For Linux `/proc`-based memory evidence from a macOS checkout, run the same
review through local `act` with two mounted worktrees:

```sh
just performance-memory-review-act /tmp/strata-base .
```

The `act` recipe uses the `performance-memory` workflow and mounts the base and
current worktrees into an Ubuntu container. It is an optional review tool, not a
replacement for `just quality`. The same workflow can also be started manually
in GitHub with a base ref when local worktree mounts are not available.

Useful local command:

```sh
just performance-smoke
```

Useful RSS comparison command:

```sh
just performance-rss-compare /tmp/strata-base .
```

Useful product CLI footprint command:

```sh
just performance-cli-footprint-compare /tmp/strata-base .
```

Useful full memory review command:

```sh
just performance-memory-review /tmp/strata-base .
```

## Fuzzing

The fuzz harnesses live under `fuzz/` and run with `cargo-fuzz` on nightly Rust.
They cover five boundaries:

- parsing, checking, and lowering arbitrary UTF-8 source;
- parsing, checking, and lowering delimited multi-source Strata source programs;
- decoding and re-encoding arbitrary UTF-8 artifact text;
- running valid lowered artifacts through the in-memory runtime host;
- validating Mantle-owned runtime trace JSONL without trusting trace strings as
  executable meaning.

Committed seed corpora under `fuzz/seeds/` keep the smoke runs exercising valid
collection, template, and source-to-runtime examples even before mutation finds
those forms from random input. The smoke recipe copies those seeds into ignored
`fuzz/corpus/` directories before running `cargo-fuzz`, so mutation output does
not touch tracked seed fixtures.

Useful local commands:

```sh
just install-fuzz-tools
just fuzz-ci
```

## Miri

Miri runs on nightly Rust. The Miri gate is a smoke suite focused on pure or
in-memory paths rather than filesystem-specific CLI behavior. It includes a
targeted immutable source-local binding check/lower smoke for the source
resolution path; this surface does not add a new unsafe-adjacent runtime path.

Useful local commands:

```sh
just install-miri-tools
just miri-ci
```

Every change that affects user-facing syntax, artifact schema, runtime behavior,
diagnostics, examples, or acceptance gates should update this book and pass
`mdbook build docs`.
