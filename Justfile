set dotenv-load

default:
    @just --list

# =============================================================================
# Local development
# =============================================================================

fmt:
    cargo fmt --all
    cargo fmt --manifest-path fuzz/Cargo.toml --all

fmt-check:
    cargo fmt --all --check
    cargo fmt --manifest-path fuzz/Cargo.toml --all --check

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace --all-targets

lint:
    cargo clippy --workspace --all-targets -- -D warnings

build:
    cargo build

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

product-gates: build
    cargo run -p strata --bin strata -- check examples/hello.str
    cargo run -p strata --bin strata -- build examples/hello.str
    cargo run -p mantle-runtime --bin mantle -- run target/strata/hello.mta
    cargo run -p strata --bin strata -- check examples/actor_ping.str
    cargo run -p strata --bin strata -- build examples/actor_ping.str
    cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_ping.mta
    cargo run -p strata --bin strata -- check examples/actor_sequence.str
    cargo run -p strata --bin strata -- build examples/actor_sequence.str
    cargo run -p mantle-runtime --bin mantle -- run target/strata/actor_sequence.mta

quality: fmt-check check test lint metadata-check docs product-gates diff-check

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

install-ci-tools-linux:
    #!/usr/bin/env bash
    set -euo pipefail

    sudo apt-get update
    sudo apt-get install -y jq libxml2-utils
    just install-docs-tools

install-docs-tools:
    cargo install mdbook --version 0.5.2 --locked --target-dir target/cargo-install

install-fuzz-tools:
    rustup toolchain install stable --profile minimal
    rustup toolchain install nightly --profile minimal --component clippy
    cargo +stable install cargo-fuzz --version 0.13.1 --locked --target-dir target/cargo-install

ci-rust: check test build

ci-quality: quality

# =============================================================================
# Nightly validation
# =============================================================================

fuzz-lint:
    cargo +nightly clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings

fuzz-build:
    cargo +nightly fuzz build strata_parse_check_lower
    cargo +nightly fuzz build mantle_artifact_decode
    cargo +nightly fuzz build mantle_runtime_from_source

fuzz-smoke:
    cargo +nightly fuzz run strata_parse_check_lower -- -runs=256
    cargo +nightly fuzz run mantle_artifact_decode -- -runs=256
    cargo +nightly fuzz run mantle_runtime_from_source -- -runs=128

fuzz-ci: fuzz-build fuzz-lint fuzz-smoke

miri-setup:
    cargo +nightly miri setup

miri-smoke:
    cargo +nightly miri test -p mantle-artifact artifact_round_trips_and_validates_magic
    cargo +nightly miri test -p strata parses_and_checks_hello
    cargo +nightly miri test -p mantle-runtime in_memory_host_runs_actor_without_filesystem_trace_sink

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
        cargo build "${CARGO_ARGS[@]}" "${PACKAGE_ARGS[@]}" --target "$RUST_TARGET"
    else
        echo "Target: native ($(rustc -vV | awk '/^host:/ { print $2 }'))"
        cargo build "${CARGO_ARGS[@]}" "${PACKAGE_ARGS[@]}"
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
