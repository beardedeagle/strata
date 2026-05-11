set dotenv-load

stable_toolchain := "stable"
nightly_toolchain := "nightly"
mdbook_version := "0.5.2"
cargo_fuzz_version := "0.13.1"

default:
    @just --list

# =============================================================================
# Local development
# =============================================================================

fmt:
    cargo +{{stable_toolchain}} fmt --all
    cargo +{{stable_toolchain}} fmt --manifest-path fuzz/Cargo.toml --all

fmt-check:
    cargo +{{stable_toolchain}} fmt --all --check
    cargo +{{stable_toolchain}} fmt --manifest-path fuzz/Cargo.toml --all --check

check:
    cargo +{{stable_toolchain}} check --workspace --all-targets

test:
    cargo +{{stable_toolchain}} test --workspace --all-targets

lint:
    cargo +{{stable_toolchain}} clippy --workspace --all-targets -- -D warnings

performance-smoke:
    cargo +{{stable_toolchain}} test -p strata-mantle-acceptance --test performance_smoke -- --ignored --nocapture

build:
    cargo +{{stable_toolchain}} build

metadata-check:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! command -v jq >/dev/null 2>&1; then
        echo "Error: jq is required for metadata-check." >&2
        echo "Install jq and retry. On macOS: brew install jq. On Ubuntu: sudo apt-get install jq." >&2
        exit 1
    fi

    if ! command -v xmllint >/dev/null 2>&1; then
        echo "Error: xmllint is required for metadata-check." >&2
        echo "Install xmllint and retry. On macOS: install libxml2. On Ubuntu: sudo apt-get install libxml2-utils." >&2
        exit 1
    fi

    jq empty tools/vscode-strata/package.json
    xmllint --noout tools/mime/strata.xml

docs:
    mdbook build docs

docs-serve:
    cd docs && mdbook serve

diff-check:
    git diff --check

toolchain-policy-check:
    #!/usr/bin/env bash
    set -euo pipefail

    paths=(README.md docs .github Justfile)
    nightly_word="nightly"
    forbidden_patterns=(
        "rustup override set ${nightly_word}"
        "rustup default ${nightly_word}"
    )

    for pattern in "${forbidden_patterns[@]}"; do
        if git grep --untracked -n -- "$pattern" -- "${paths[@]}"; then
            echo "Error: repo toolchain policy forbids '$pattern'." >&2
            echo "Use stable for standard gates and select nightly per command with +nightly." >&2
            exit 1
        fi
    done

    nightly_cargo_regex='(^|[[:space:]])cargo (fuzz|miri)([[:space:]]|$)'
    if git grep --untracked -n -E "$nightly_cargo_regex" -- "${paths[@]}"; then
        echo "Error: nightly-only cargo subcommands must be invoked as cargo +nightly ..." >&2
        exit 1
    fi

    fuzz_clippy_regex='(^|[[:space:]])cargo clippy --manifest-path fuzz/Cargo.toml'
    if git grep --untracked -n -E "$fuzz_clippy_regex" -- "${paths[@]}"; then
        echo "Error: fuzz clippy must be invoked as cargo +nightly clippy ..." >&2
        exit 1
    fi

    echo "Toolchain policy OK: standard gates use stable and nightly gates are explicit."

source-to-runtime-gates: source-to-runtime-success-gates source-to-runtime-failure-gates

source-to-runtime-success-gates: build
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/hello.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/hello.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_ping.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_ping.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_ping.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_sequence.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_sequence.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/init_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/init_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/init_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/function_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/function_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/function_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/function_payload_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/function_payload_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/function_payload_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/function_collection_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/function_collection_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/function_collection_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/state_payload_enum.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/state_payload_enum.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/state_payload_enum.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/collection_state.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/collection_state.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/collection_state.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/state_payload_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/state_payload_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/state_payload_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_instances.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_instances.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_instances.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_payloads.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_payloads.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_payloads.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_payload_match.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_payload_match.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_payload_match.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_reply.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_reply.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_reply.mta
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_emit_spawn_send.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_emit_spawn_send.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_emit_spawn_send.mta

source-to-runtime-failure-gates: build
    #!/usr/bin/env bash
    set -euo pipefail

    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/actor_panic_no_replay.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/actor_panic_no_replay.str
    trace="target/strata/actor_panic_no_replay.observability.jsonl"
    effect_authority_stderr="$(mktemp)"
    run_stderr="$(mktemp)"
    trap 'rm -f "$effect_authority_stderr" "$run_stderr"' EXIT

    if cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/failures/effect_authority_missing.str 2>"$effect_authority_stderr"; then
        echo "Error: effect_authority_missing was expected to fail source effect authority checks." >&2
        exit 1
    fi
    if ! grep -q 'step uses effect send but does not declare it' "$effect_authority_stderr"; then
        echo "Error: effect_authority_missing failed for an unexpected reason." >&2
        cat "$effect_authority_stderr" >&2
        exit 1
    fi

    rm -f "$trace"
    if cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/actor_panic_no_replay.mta 2>"$run_stderr"; then
        echo "Error: actor_panic_no_replay was expected to fail closed." >&2
        exit 1
    fi
    if ! grep -q 'mantle: error: process Worker panicked after consuming message Ping; message will not be replayed' "$run_stderr"; then
        echo "Error: actor_panic_no_replay failed for an unexpected reason." >&2
        cat "$run_stderr" >&2
        exit 1
    fi

    test -f "$trace"
    accepted_count="$(grep -c '"event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping"' "$trace")"
    dequeued_count="$(grep -c '"event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping"' "$trace")"
    if [[ "$accepted_count" != "2" || "$dequeued_count" != "1" ]]; then
        echo "Error: expected two accepted Worker Ping messages and one dequeue before panic." >&2
        exit 1
    fi
    grep -q '"event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Ping","result":"Panic","state_id":1,"state":"Failed"' "$trace"
    grep -q '"event":"process_failed","pid":2,"process_id":1,"process":"Worker","state_id":1,"state":"Failed","reason":"panic"' "$trace"
    if grep -q '"event":"process_stopped","pid":2,"process_id":1,"process":"Worker"' "$trace"; then
        echo "Error: panic must not be reported as a normal process stop." >&2
        exit 1
    fi

quality: fmt-check check test lint performance-smoke metadata-check toolchain-policy-check docs source-to-runtime-gates diff-check

ci-native: quality

ci-local:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! command -v act >/dev/null 2>&1; then
        echo "Error: act is required for Linux CI parity." >&2
        echo "Install it from https://nektosact.com/ and retry." >&2
        exit 1
    fi

    if ! command -v docker >/dev/null 2>&1; then
        echo "Error: Docker is required by act but is not on PATH." >&2
        exit 1
    fi

    if ! docker info >/dev/null 2>&1; then
        echo "Error: Docker is not running. Start Docker and retry." >&2
        exit 1
    fi

    echo "==> [1/2] Native quality gate"
    just ci-native

    echo "==> [2/2] Linux CI parity via act"
    act pull_request \
        -W .github/workflows/ci.yml \
        -j quality-docs \
        -P ubuntu-latest=ghcr.io/catthehacker/ubuntu:act-latest \
        --container-architecture linux/amd64

# =============================================================================
# CI setup and entry points
# =============================================================================

install-ci-tools-linux: install-linux-metadata-tools install-docs-tools

install-linux-metadata-tools:
    #!/usr/bin/env bash
    set -euo pipefail

    sudo apt-get update
    sudo apt-get install -y jq libxml2-utils

install-docs-tools:
    rustup toolchain install {{stable_toolchain}} --profile minimal
    cargo +{{stable_toolchain}} install mdbook --version {{mdbook_version}} --locked --target-dir target/cargo-install

install-fuzz-tools:
    rustup toolchain install {{stable_toolchain}} --profile minimal
    rustup toolchain install {{nightly_toolchain}} --profile minimal --component clippy
    cargo +{{stable_toolchain}} install cargo-fuzz --version {{cargo_fuzz_version}} --locked --target-dir target/cargo-install

install-miri-tools:
    rustup toolchain install {{nightly_toolchain}} --profile minimal --component miri

ci-rust: check test build

ci-quality: quality

# =============================================================================
# Nightly validation
# =============================================================================

fuzz-lint:
    cargo +{{nightly_toolchain}} clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings

fuzz-build:
    cargo +{{nightly_toolchain}} fuzz build strata_parse_check_lower
    cargo +{{nightly_toolchain}} fuzz build mantle_artifact_decode
    cargo +{{nightly_toolchain}} fuzz build mantle_runtime_from_source

fuzz-smoke:
    #!/usr/bin/env bash
    set -euo pipefail

    mkdir -p \
        fuzz/corpus/strata_parse_check_lower \
        fuzz/corpus/mantle_artifact_decode \
        fuzz/corpus/mantle_runtime_from_source

    cargo +{{nightly_toolchain}} fuzz run strata_parse_check_lower fuzz/corpus/strata_parse_check_lower fuzz/seeds/strata_parse_check_lower -- -runs=256
    cargo +{{nightly_toolchain}} fuzz run mantle_artifact_decode fuzz/corpus/mantle_artifact_decode fuzz/seeds/mantle_artifact_decode -- -runs=256
    cargo +{{nightly_toolchain}} fuzz run mantle_runtime_from_source fuzz/corpus/mantle_runtime_from_source fuzz/seeds/mantle_runtime_from_source -- -runs=128

fuzz-ci: fuzz-build fuzz-lint fuzz-smoke

miri-setup:
    cargo +{{nightly_toolchain}} miri setup

miri-smoke:
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact artifact_round_trips_and_validates_magic
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact map_projection_rejects_duplicate_expected_keys
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact validate_rejects_payload_dependent_map_template_key
    cargo +{{nightly_toolchain}} miri test -p strata parses_and_checks_hello
    cargo +{{nightly_toolchain}} miri test -p strata checks_source_function_subset_map_patterns
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime in_memory_host_runs_actor_without_filesystem_trace_sink

miri-ci: miri-setup miri-smoke

nightly-ci: fuzz-ci miri-ci

# =============================================================================
# Build matrix
# =============================================================================

_opt_levels := "debug release optimized max"
_targets := "native linux linux-musl macos macos-arm windows"

build-matrix level target="native" package="all":
    #!/usr/bin/env bash
    set -euo pipefail

    LEVEL="{{level}}"
    TARGET="{{target}}"
    PACKAGE="{{package}}"

    case "$LEVEL" in
        debug|release|optimized|max) ;;
        *)
            echo "Error: invalid optimization level '$LEVEL'" >&2
            echo "Valid levels: debug, release, optimized, max" >&2
            exit 1
            ;;
    esac

    case "$TARGET" in
        native)     RUST_TARGET="" ;;
        linux)      RUST_TARGET="x86_64-unknown-linux-gnu" ;;
        linux-musl) RUST_TARGET="x86_64-unknown-linux-musl" ;;
        macos)      RUST_TARGET="x86_64-apple-darwin" ;;
        macos-arm)  RUST_TARGET="aarch64-apple-darwin" ;;
        windows)    RUST_TARGET="x86_64-pc-windows-msvc" ;;
        *)
            echo "Error: invalid target '$TARGET'" >&2
            echo "Valid targets: native, linux, linux-musl, macos, macos-arm, windows" >&2
            exit 1
            ;;
    esac

    if [[ "$PACKAGE" == "all" ]]; then
        PACKAGE_ARGS=(--workspace)
        PACKAGE_DESC="all packages"
    else
        PACKAGE_ARGS=(-p "$PACKAGE")
        PACKAGE_DESC="$PACKAGE"
    fi

    case "$LEVEL" in
        debug)
            CARGO_ARGS=()
            export CARGO_PROFILE_DEV_OPT_LEVEL=0
            echo "Building: debug"
            ;;
        release)
            CARGO_ARGS=(--release)
            echo "Building: release"
            ;;
        optimized)
            CARGO_ARGS=(--release)
            export CARGO_PROFILE_RELEASE_LTO=thin
            export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
            echo "Building: optimized"
            ;;
        max)
            CARGO_ARGS=(--release)
            export CARGO_PROFILE_RELEASE_LTO=fat
            export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
            export CARGO_PROFILE_RELEASE_OPT_LEVEL=3
            export CARGO_PROFILE_RELEASE_STRIP=symbols
            if [[ "$TARGET" == "macos-arm" ]] || [[ "$TARGET" == "native" && "$(uname -m)" == "arm64" ]]; then
                export RUSTFLAGS="-C target-cpu=native"
            else
                export RUSTFLAGS="-C target-cpu=x86-64-v3"
            fi
            echo "Building: max"
            ;;
    esac

    echo "Package: $PACKAGE_DESC"
    if [[ -n "$RUST_TARGET" ]]; then
        echo "Target: $RUST_TARGET"
        cargo +{{stable_toolchain}} build "${CARGO_ARGS[@]}" "${PACKAGE_ARGS[@]}" --target "$RUST_TARGET"
    else
        echo "Target: native ($(rustc +{{stable_toolchain}} -vV | awk '/^host:/ { print $2 }'))"
        cargo +{{stable_toolchain}} build "${CARGO_ARGS[@]}" "${PACKAGE_ARGS[@]}"
    fi

build-all level="release":
    @echo "Building native target at '{{level}}' optimization level."
    @just build-matrix {{level}} native
    @echo ""
    @echo "For cross-compilation, install the needed target and run:"
    @echo "  just build-matrix {{level}} linux"
    @echo "  just build-matrix {{level}} linux-musl"
    @echo "  just build-matrix {{level}} macos"
    @echo "  just build-matrix {{level}} macos-arm"
    @echo "  just build-matrix {{level}} windows"

build-help:
    @echo "Build matrix"
    @echo ""
    @echo "Usage:"
    @echo "  just build-matrix <level> [target] [package]"
    @echo ""
    @echo "Optimization levels:"
    @echo "  debug      Fast local build"
    @echo "  release    Cargo release defaults"
    @echo "  optimized  Release with thin LTO and one codegen unit"
    @echo "  max        Release with fat LTO, strip, and CPU targeting"
    @echo ""
    @echo "Targets:"
    @echo "  native linux linux-musl macos macos-arm windows"
    @echo ""
    @echo "Packages:"
    @echo "  all mantle-artifact mantle-runtime strata"
