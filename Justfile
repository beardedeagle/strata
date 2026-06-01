set dotenv-load

stable_toolchain := "stable"
nightly_toolchain := "nightly"
mdbook_version := "0.5.2"
mdbook_mermaid_version := "0.17.0"
cargo_fuzz_version := "0.13.1"
cfg_check_targets := "x86_64-unknown-linux-musl x86_64-apple-darwin x86_64-pc-windows-msvc"
fuzz_targets := "strata_parse_check_lower strata_source_program_check_lower mantle_artifact_decode mantle_runtime_from_source mantle_trace_validate"
fuzz_smoke_targets := "strata_parse_check_lower:256 strata_source_program_check_lower:128 mantle_artifact_decode:256 mantle_runtime_from_source:128 mantle_trace_validate:128"

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

cfg-check:
    #!/usr/bin/env bash
    set -euo pipefail

    targets=( {{cfg_check_targets}} )
    installed="$(rustup target list --installed --toolchain {{stable_toolchain}})"
    missing=()

    for target in "${targets[@]}"; do
        if ! grep -qx "$target" <<<"$installed"; then
            missing+=("$target")
        fi
    done

    if (( ${#missing[@]} > 0 )); then
        echo "Error: cfg-check requires additional Rust targets." >&2
        printf 'Install missing targets with: rustup target add --toolchain {{stable_toolchain}}' >&2
        printf ' %s' "${missing[@]}" >&2
        printf '\n' >&2
        exit 1
    fi

    for target in "${targets[@]}"; do
        echo "==> cfg-check target: $target"
        cargo +{{stable_toolchain}} check --workspace --all-targets --target "$target"
    done

test:
    cargo +{{stable_toolchain}} test --workspace --all-targets

lint:
    cargo +{{stable_toolchain}} clippy --workspace --all-targets -- -D warnings

performance-smoke:
    cargo +{{stable_toolchain}} test -p strata-mantle-acceptance --test performance_smoke -- --ignored --nocapture

performance-smoke-profile profile:
    STRATA_PERFORMANCE_SMOKE_PROFILE="{{profile}}" cargo +{{stable_toolchain}} test -p strata-mantle-acceptance --test performance_smoke -- --ignored --nocapture

performance-rss-compare base current runs="30" profiles="collection_state.check_lower collection_state.in_memory_runtime boundary_contracts_main.in_memory_runtime":
    bash tools/performance-rss-compare.sh --runs "{{runs}}" "{{base}}" "{{current}}" {{profiles}}

performance-cli-footprint-compare base current runs="20":
    bash tools/performance-cli-footprint-compare.sh --runs "{{runs}}" "{{base}}" "{{current}}"

performance-memory-review base current rss_runs="30" cli_runs="20":
    just performance-rss-compare "{{base}}" "{{current}}" "{{rss_runs}}"
    just performance-cli-footprint-compare "{{base}}" "{{current}}" "{{cli_runs}}"

performance-memory-review-act base current rss_runs="30" cli_runs="20":
    #!/usr/bin/env bash
    set -euo pipefail

    if ! command -v act >/dev/null 2>&1; then
        echo "Error: act is required for Linux memory review." >&2
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

    base_path="$(cd "{{base}}" && pwd -P)"
    current_path="$(cd "{{current}}" && pwd -P)"

    act workflow_dispatch \
        -W .github/workflows/performance-memory.yml \
        -j memory-review \
        -P ubuntu-latest=ghcr.io/catthehacker/ubuntu:rust-latest \
        --container-architecture linux/amd64 \
        --container-options "--volume ${base_path}:/strata-base --volume ${current_path}:/strata-current" \
        --env STRATA_PERFORMANCE_BASE_WORKTREE=/strata-base \
        --env STRATA_PERFORMANCE_CURRENT_WORKTREE=/strata-current \
        --env STRATA_PERFORMANCE_RSS_RUNS="{{rss_runs}}" \
        --env STRATA_PERFORMANCE_CLI_RUNS="{{cli_runs}}"

build:
    cargo +{{stable_toolchain}} build

strata-check source:
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check "{{source}}"

strata-build source:
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build "{{source}}"

mantle-run artifact:
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run "{{artifact}}"

mantle-run-deny-spawn-authority artifact:
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run "{{artifact}}" --deny-spawn-authority

strata-authority-summary source format="text":
    cargo +{{stable_toolchain}} run -p strata --bin strata -- authority-summary "{{source}}" --format "{{format}}"

strata-composition-report source format="text":
    cargo +{{stable_toolchain}} run -p strata --bin strata -- composition-report "{{source}}" --format "{{format}}"

mantle-inspect-authority artifact format="text":
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- inspect-authority "{{artifact}}" --format "{{format}}"

mantle-feature-declaration format="text":
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- feature-declaration --format "{{format}}"

strata-target-requirements source format="text":
    cargo +{{stable_toolchain}} run -p strata --bin strata -- target-requirements "{{source}}" --format "{{format}}"

mantle-admit artifact format="text":
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- admit "{{artifact}}" --format "{{format}}"

run-example name:
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check "examples/{{name}}.str"
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build "examples/{{name}}.str"
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run "target/strata/{{name}}.mta"

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

    project_cargo_regex='(^|[[:space:]])cargo ([+][[:alnum:]_.-]+[[:space:]]+)?(run|test|clippy|check|build)([[:space:]]|$)'
    if git grep --untracked -n -E "$project_cargo_regex" -- README.md docs; then
        echo "Error: public docs must route project commands through just recipes." >&2
        echo "Add or use a Justfile recipe instead of documenting raw cargo run/test/check/build/clippy commands." >&2
        exit 1
    fi

    echo "Toolchain policy OK: standard gates use stable and nightly gates are explicit."

source-to-runtime-gates: source-to-runtime-success-gates source-to-runtime-failure-gates

source-to-runtime-success-gates: build
    #!/usr/bin/env bash
    set -euo pipefail

    examples=(
        hello
        actor_ping
        actor_sequence
        actor_match
        init_match
        init_return_match
        function_match
        function_payload_match
        function_if_else
        function_local_bindings
        process_return_match
        process_return_match_arm_prefix
        process_return_match_arm_runtime_if_prefix
        process_return_match_arm_for_prefix
        process_return_match_arm_for_if_prefix
        process_return_match_arm_if_for_prefix
        process_return_match_arm_action_block
        function_collection_match
        function_return_match
        function_record_pattern
        function_record_return_match
        function_record_body_match
        imports_main
        boundary_contracts_main
        component_composition_main
        state_payload_enum
        collection_state
        state_payload_match
        actor_instances
        actor_payloads
        runtime_if_else
        runtime_scalar_priority
        runtime_payload_projection_if
        runtime_payload_projection_next_state
        runtime_state_payload_projection_if
        runtime_state_payload_projection_next_state
        runtime_nested_if_actions
        runtime_final_if_guarded_loop
        runtime_final_if_nested_if_actions
        runtime_final_if_nested_terminal_if
        runtime_guard_noop
        runtime_for_each
        runtime_for_each_empty
        runtime_for_each_if
        runtime_for_each_nested_if_actions
        runtime_guarded_for_each
        runtime_guarded_ref_loop
        runtime_guarded_ref_loop_jobs
        runtime_loop_element_projection
        actor_payload_match
        actor_payload_split_match
        actor_payload_split_signature
        actor_payload_split_signature_wildcard
        actor_payload_state_match_split
        actor_payload_state_match_wildcard
        nested_patterns
        actor_reply
        actor_emit_spawn_send
        effect_outcomes
        effect_outcome_mailbox_full
        effect_outcome_stopped_target
        effect_outcome_spawn_denied
        local_supervision_restart
        local_supervision_permanent_stop
        local_supervision_temporary
        local_supervision_transient_restart
        local_supervision_transient
        local_supervision_inactive_send_outcome
    )

    cargo_run=(cargo +{{stable_toolchain}} run)
    for example in "${examples[@]}"; do
        "${cargo_run[@]}" -p strata --bin strata -- check "examples/${example}.str"
        "${cargo_run[@]}" -p strata --bin strata -- build "examples/${example}.str"
        "${cargo_run[@]}" -p mantle-runtime --bin mantle -- run "target/strata/${example}.mta"
    done

    "${cargo_run[@]}" -p strata --bin strata -- composition-report examples/component_composition_main.str --format json >/dev/null
    "${cargo_run[@]}" -p strata --bin strata -- target-requirements examples/component_composition_main.str --format json >/dev/null
    "${cargo_run[@]}" -p mantle-runtime --bin mantle -- feature-declaration --format json >/dev/null
    "${cargo_run[@]}" -p mantle-runtime --bin mantle -- admit target/strata/component_composition_main.mta --format json >/dev/null
    "${cargo_run[@]}" -p mantle-runtime --bin mantle -- run target/strata/effect_outcome_spawn_denied.mta --deny-spawn-authority
    "${cargo_run[@]}" -p strata --bin strata -- check examples/effect_outcome_crashed_target.str
    "${cargo_run[@]}" -p strata --bin strata -- build examples/effect_outcome_crashed_target.str

source-to-runtime-failure-gates: build
    #!/usr/bin/env bash
    set -euo pipefail

    cargo_run=(cargo +{{stable_toolchain}} run)

    expect_check_failure() {
        local source="$1"
        local expected="$2"
        local label="$3"
        local output

        if output="$("${cargo_run[@]}" -p strata --bin strata -- check "$source" 2>&1)"; then
            echo "Error: $label was expected to fail source checks." >&2
            exit 1
        fi
        if [[ "$output" != *"$expected"* ]]; then
            echo "Error: $label failed for an unexpected reason." >&2
            printf '%s\n' "$output" >&2
            exit 1
        fi
    }

    "${cargo_run[@]}" -p strata --bin strata -- check examples/actor_panic_no_replay.str
    "${cargo_run[@]}" -p strata --bin strata -- build examples/actor_panic_no_replay.str
    trace="target/strata/actor_panic_no_replay.observability.jsonl"

    expect_check_failure examples/failures/effect_authority_missing.str \
        'step uses effect send but does not declare it' effect_authority_missing
    expect_check_failure examples/failures/function_return_if_statement.str \
        'source function choose return-if then branch must not perform statements' function_return_if_statement
    expect_check_failure examples/failures/source_local_binding_process_ref.str \
        'source-local binding worker_local must use a declared record, enum, scalar, list, or map type' source_local_binding_process_ref
    expect_check_failure examples/failures/source_local_binding_process_ref_carrier_enum.str \
        'source-local binding copy must use a declared record, enum, scalar, list, or map type without process-reference authority' source_local_binding_process_ref_carrier_enum
    expect_check_failure examples/failures/source_local_binding_process_ref_shadow.str \
        'source-local binding worker conflicts with a process reference binding' source_local_binding_process_ref_shadow
    expect_check_failure examples/failures/source_function_parameter_process_ref_shadow.str \
        'source function parameter worker conflicts with a process reference binding' source_function_parameter_process_ref_shadow
    expect_check_failure examples/failures/process_return_match_arm_nested_for.str \
        'nested for loops are not supported' process_return_match_arm_nested_for
    expect_check_failure examples/failures/process_return_match_arm_excessive_if.str \
        'statement-level if action nesting exceeds maximum depth' process_return_match_arm_excessive_if
    expect_check_failure examples/failures/process_return_match_arm_final_if_excessive_if.str \
        'statement-level if action nesting exceeds maximum depth' process_return_match_arm_final_if_excessive_if
    expect_check_failure examples/failures/scalar_overflow.str \
        'scalar arithmetic result 256 is outside U8 range' scalar_overflow
    expect_check_failure examples/failures/scalar_type_mismatch.str \
        'scalar literal 2_u64 has type U64, expected U32' scalar_type_mismatch
    expect_check_failure examples/failures/scalar_divide_by_zero.str \
        'scalar division by zero' scalar_divide_by_zero
    rm -f target/strata/scalar_runtime_divide_by_zero.mta target/strata/scalar_runtime_modulo_by_zero.mta
    expect_check_failure examples/failures/scalar_runtime_divide_by_zero.str \
        'scalar division by zero' scalar_runtime_divide_by_zero
    test ! -f target/strata/scalar_runtime_divide_by_zero.mta
    expect_check_failure examples/failures/scalar_runtime_modulo_by_zero.str \
        'scalar modulo by zero' scalar_runtime_modulo_by_zero
    test ! -f target/strata/scalar_runtime_modulo_by_zero.mta
    expect_check_failure examples/failures/scalar_unsuffixed_literal.str \
        'numeric value literals require an explicit scalar suffix' scalar_unsuffixed_literal
    expect_check_failure examples/failures/import_missing.str \
        'missing_import_target.str' import_missing
    expect_check_failure examples/failures/import_cycle_root.str \
        'import cycle import_cycle_root -> import_cycle_leaf -> import_cycle_root' import_cycle_root
    expect_check_failure examples/failures/import_unimported_root.str \
        'references type Job from module import_unimported_types without importing import_unimported_types' import_unimported_root

    rm -f "$trace"
    run_output=""
    if run_output="$("${cargo_run[@]}" -p mantle-runtime --bin mantle -- run target/strata/actor_panic_no_replay.mta 2>&1)"; then
        echo "Error: actor_panic_no_replay was expected to fail closed." >&2
        exit 1
    fi
    if [[ "$run_output" != *'mantle: error: process Worker panicked after consuming message Ping; message will not be replayed'* ]]; then
        echo "Error: actor_panic_no_replay failed for an unexpected reason." >&2
        printf '%s\n' "$run_output" >&2
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

quality: fmt-check check cfg-check test lint performance-smoke metadata-check toolchain-policy-check docs assurance source-to-runtime-gates diff-check

assurance: language-surface-assurance bounded-assurance-smoke

language-surface-assurance:
    cargo +{{stable_toolchain}} test -p strata-mantle-acceptance --test language_surface_assurance

bounded-assurance-smoke:
    cargo +{{stable_toolchain}} test -p strata process_return_match_arm_bounded_assurance --lib
    cargo +{{stable_toolchain}} test -p strata process_return_match_arm_prefix_properties --lib
    cargo +{{stable_toolchain}} test -p strata source_function_if_else --lib
    cargo +{{stable_toolchain}} test -p strata source_function_local_bindings --lib
    cargo +{{stable_toolchain}} test -p strata source_scalar_bounded --lib
    cargo +{{stable_toolchain}} test -p strata bounded_scalar_folding_matches_independent_model_and_binding_expansion --lib
    cargo +{{stable_toolchain}} test -p strata-mantle-acceptance process_return_match_arm_bounded_runtime --test source_to_runtime_gates
    cargo +{{stable_toolchain}} test -p strata-mantle-acceptance process_return_match_arm_action_block --test source_to_runtime_gates

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

install-ci-tools-linux: install-linux-metadata-tools install-docs-tools install-cfg-check-targets

install-linux-metadata-tools:
    #!/usr/bin/env bash
    set -euo pipefail

    sudo apt-get update
    sudo apt-get install -y jq libxml2-utils

install-docs-tools:
    rustup toolchain install {{stable_toolchain}} --profile minimal
    cargo +{{stable_toolchain}} install mdbook --version {{mdbook_version}} --locked --target-dir target/cargo-install
    cargo +{{stable_toolchain}} install mdbook-mermaid --version {{mdbook_mermaid_version}} --locked --target-dir target/cargo-install

install-cfg-check-targets:
    rustup target add --toolchain {{stable_toolchain}} {{cfg_check_targets}}

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
    for target in {{fuzz_targets}}; do cargo +{{nightly_toolchain}} fuzz build "$target"; done

fuzz-smoke:
    #!/usr/bin/env bash
    set -euo pipefail

    targets=( {{fuzz_smoke_targets}} )
    for target_spec in "${targets[@]}"; do
        target="${target_spec%%:*}"
        runs="${target_spec##*:}"
        corpus_dir="fuzz/corpus/$target"
        seed_dir="fuzz/seeds/$target"

        mkdir -p "$corpus_dir"
        cargo +{{nightly_toolchain}} fuzz run "$target" "$corpus_dir" "$seed_dir" -- -runs="$runs"
    done

fuzz-ci: fuzz-build fuzz-lint fuzz-smoke

miri-setup:
    cargo +{{nightly_toolchain}} miri setup

miri-smoke:
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact artifact_round_trips_and_validates_magic
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact map_projection_rejects_duplicate_expected_keys
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact validate_rejects_payload_dependent_map_template_key
    cargo +{{nightly_toolchain}} miri test -p strata parses_and_checks_hello
    cargo +{{nightly_toolchain}} miri test -p strata checks_source_function_subset_map_patterns
    cargo +{{nightly_toolchain}} miri test -p strata parses_checks_and_lowers_source_function_braced_return_if_else
    cargo +{{nightly_toolchain}} miri test -p strata parses_checks_and_lowers_immutable_source_local_bindings
    cargo +{{nightly_toolchain}} miri test -p strata source_program_checks_lowers_cross_unit_records_functions_and_processes
    cargo +{{nightly_toolchain}} miri test -p strata accepts_typed_local_one_for_one_supervision
    cargo +{{nightly_toolchain}} miri test -p strata property_generated_uniform_arm_prefix_shapes_lower_as_typed_actions
    cargo +{{nightly_toolchain}} miri test -p strata property_generated_selected_arm_action_block_shapes_lower_as_typed_actions
    cargo +{{nightly_toolchain}} miri test -p mantle-artifact validate_admits_lexical_supervisor_child_spawn_site
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime in_memory_host_runs_actor_without_filesystem_trace_sink
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime runtime_rejects_loaded_spawn_site_target_mismatched_with_authority_before_artifact_loaded
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime runtime_spawn_outcome_returns_denied_before_acceptance
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime bounded_restart_intensity_denies_second_restart_within_window
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime restarted_supervisor_child_stops_its_old_supervised_subtree
    cargo +{{nightly_toolchain}} miri test -p mantle-runtime loaded_admission_rejects_indirect_supervisor_cycle

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
