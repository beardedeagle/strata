#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat >&2 <<'USAGE'
Usage:
  tools/performance-cli-footprint-compare.sh [--runs N] BASE_WORKTREE CURRENT_WORKTREE

Builds the release Strata and Mantle CLI binaries in both worktrees, compares
binary file sizes, then runs product CLI commands in fresh OS processes and
reports max-RSS median/min/max/p90 deltas in KiB. Measured product commands run
under a minimal environment, and the script emits metadata for the compared
worktrees.
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs=20

while [[ $# -gt 0 ]]; do
    case "$1" in
        --runs)
            if [[ $# -lt 2 ]]; then
                usage
                exit 2
            fi
            runs="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --)
            shift
            break
            ;;
        -*)
            echo "Error: unknown option $1" >&2
            usage
            exit 2
            ;;
        *)
            break
            ;;
    esac
done

if [[ $# -ne 2 ]]; then
    usage
    exit 2
fi

if [[ ! "$runs" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: --runs must be a positive integer, got $runs" >&2
    exit 2
fi

base_worktree="$1"
current_worktree="$2"
platform="$(uname -s)"
case "$platform" in
    Darwin|FreeBSD|OpenBSD|Linux) ;;
    *)
        echo "Error: unsupported platform for RSS capture: $platform" >&2
        exit 2
        ;;
esac

for worktree in "$base_worktree" "$current_worktree"; do
    if [[ ! -f "${worktree}/Cargo.toml" ]]; then
        echo "Error: ${worktree} is not a Strata worktree with Cargo.toml" >&2
        exit 2
    fi
    if [[ ! -f "${worktree}/examples/collection_state.str" ]]; then
        echo "Error: ${worktree} is missing examples/collection_state.str" >&2
        exit 2
    fi
done

output_dir="${STRATA_PERFORMANCE_CLI_OUTPUT_DIR:-${repo_root}/target/performance-cli-footprint}"
mkdir -p "$output_dir"
stamp="$(date +%Y%m%d%H%M%S)"
rss_csv="${output_dir}/cli-rss-samples-${stamp}.csv"
binary_csv="${output_dir}/binary-size-${stamp}.csv"
metadata_csv="${output_dir}/metadata-${stamp}.csv"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/strata-cli-footprint.XXXXXX")"

cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

stat_bytes() {
    local path="$1"
    case "$platform" in
        Darwin|FreeBSD|OpenBSD) stat -f '%z' "$path" ;;
        Linux) stat -c '%s' "$path" ;;
    esac
}

binary_path() {
    local worktree="$1"
    local binary="$2"
    printf '%s/target/release/%s\n' "$worktree" "$binary"
}

run_product_clean_env() {
    /usr/bin/env -i "PATH=${PATH:-/usr/bin:/bin}" "HOME=${HOME:-}" "TMPDIR=${TMPDIR:-/tmp}" "LC_ALL=C" "$@"
}

csv_field() {
    local value="${1//\"/\"\"}"
    printf '"%s"' "$value"
}

write_metadata_row() {
    csv_field "$1" >> "$metadata_csv"
    printf ',' >> "$metadata_csv"
    csv_field "$2" >> "$metadata_csv"
    printf '\n' >> "$metadata_csv"
}

git_head() {
    local worktree="$1"
    if ! (cd "$worktree" && git rev-parse --verify HEAD) 2>/dev/null; then
        printf 'unknown\n'
    fi
}

build_release_binaries() {
    local label="$1"
    local worktree="$2"
    local log_path="${tmp_dir}/${label}-build.log"
    if ! (
        cd "$worktree"
        cargo +stable build --release -p strata -p mantle-runtime
    ) >"$log_path" 2>&1; then
        cat "$log_path" >&2
        echo "Error: release build failed for ${label}" >&2
        exit 1
    fi
}

prepare_mantle_artifact() {
    local label="$1"
    local worktree="$2"
    local log_path="${tmp_dir}/${label}-prepare-artifact.log"
    if ! (
        cd "$worktree"
        run_product_clean_env "$(binary_path "$worktree" strata)" build examples/collection_state.str
    ) >"$log_path" 2>&1; then
        cat "$log_path" >&2
        echo "Error: artifact preparation failed for ${label}" >&2
        exit 1
    fi
}

write_binary_sizes() {
    printf 'label,binary,file_bytes\n' > "$binary_csv"
    for label in base current; do
        local worktree
        case "$label" in
            base) worktree="$base_worktree" ;;
            current) worktree="$current_worktree" ;;
        esac
        for binary in strata mantle; do
            local path
            path="$(binary_path "$worktree" "$binary")"
            if [[ ! -x "$path" ]]; then
                echo "Error: expected executable binary at $path" >&2
                exit 1
            fi
            printf '%s,%s,%s\n' "$label" "$binary" "$(stat_bytes "$path")" >> "$binary_csv"
        done
    done
}

write_metadata() {
    printf 'key,value\n' > "$metadata_csv"
    write_metadata_row platform "$platform"
    write_metadata_row runs "$runs"
    write_metadata_row environment "product CLI commands run with env -i plus PATH, HOME, TMPDIR, LC_ALL=C"
    write_metadata_row base_worktree "$base_worktree"
    write_metadata_row base_head "$(git_head "$base_worktree")"
    write_metadata_row current_worktree "$current_worktree"
    write_metadata_row current_head "$(git_head "$current_worktree")"
}

time_command() {
    local label="$1"
    local profile="$2"
    local run_index="$3"
    shift 3
    local log_path="${tmp_dir}/${label}-${profile}-${run_index}.time.log"
    local command_log_path="${tmp_dir}/${label}-${profile}-${run_index}.command.log"
    local -a clean_env
    clean_env=(/usr/bin/env -i "PATH=${PATH:-/usr/bin:/bin}" "HOME=${HOME:-}" "TMPDIR=${TMPDIR:-/tmp}" "LC_ALL=C")

    case "$platform" in
        Darwin|FreeBSD|OpenBSD)
            if ! /usr/bin/time -l "${clean_env[@]}" "$@" >"$command_log_path" 2>"$log_path"; then
                cat "$command_log_path" >&2
                cat "$log_path" >&2
                echo "Error: CLI profile ${profile} failed for ${label} run ${run_index}" >&2
                exit 1
            fi
            ;;
        Linux)
            if ! /usr/bin/time -v "${clean_env[@]}" "$@" >"$command_log_path" 2>"$log_path"; then
                cat "$command_log_path" >&2
                cat "$log_path" >&2
                echo "Error: CLI profile ${profile} failed for ${label} run ${run_index}" >&2
                exit 1
            fi
            ;;
    esac

    max_rss_kib_from_log "$log_path"
}

max_rss_kib_from_log() {
    local log_path="$1"
    case "$platform" in
        Darwin|FreeBSD|OpenBSD)
            local bytes
            bytes="$(awk '/maximum resident set size/ { value = $1 } END { print value }' "$log_path")"
            if [[ ! "$bytes" =~ ^[0-9]+$ ]]; then
                cat "$log_path" >&2
                echo "Error: could not parse max RSS bytes from $log_path" >&2
                exit 1
            fi
            printf '%s\n' "$(((bytes + 1023) / 1024))"
            ;;
        Linux)
            local kib
            kib="$(awk -F: '/Maximum resident set size/ { gsub(/^[ \t]+/, "", $2); value = $2 } END { print value }' "$log_path")"
            if [[ ! "$kib" =~ ^[0-9]+$ ]]; then
                cat "$log_path" >&2
                echo "Error: could not parse max RSS KiB from $log_path" >&2
                exit 1
            fi
            printf '%s\n' "$kib"
            ;;
    esac
}

run_cli_profile() {
    local label="$1"
    local worktree="$2"
    local profile="$3"
    local run_index="$4"
    local rss_kib

    case "$profile" in
        strata_check)
            rss_kib="$(cd "$worktree" && time_command "$label" "$profile" "$run_index" \
                "$(binary_path "$worktree" strata)" check examples/collection_state.str)"
            ;;
        strata_build)
            rss_kib="$(cd "$worktree" && time_command "$label" "$profile" "$run_index" \
                "$(binary_path "$worktree" strata)" build examples/collection_state.str)"
            ;;
        mantle_run)
            rss_kib="$(cd "$worktree" && time_command "$label" "$profile" "$run_index" \
                "$(binary_path "$worktree" mantle)" run target/strata/collection_state.mta)"
            ;;
        *)
            echo "Error: unknown CLI profile $profile" >&2
            exit 2
            ;;
    esac
    printf '%s,%s,%s,%s\n' "$label" "$profile" "$run_index" "$rss_kib" >> "$rss_csv"
}

extract_sorted_values() {
    local label="$1"
    local profile="$2"
    local output="$3"
    awk -F, -v label="$label" -v profile="$profile" \
        '$1 == label && $2 == profile { print $4 }' "$rss_csv" | sort -n > "$output"
    if [[ ! -s "$output" ]]; then
        echo "Error: no RSS samples found for ${label} ${profile}" >&2
        exit 1
    fi
}

median_from_sorted_file() {
    local values_file="$1"
    local count
    count="$(wc -l < "$values_file" | tr -d ' ')"
    if (( count % 2 == 1 )); then
        sed -n "$(((count + 1) / 2))p" "$values_file"
    else
        local lower upper
        lower="$(sed -n "$((count / 2))p" "$values_file")"
        upper="$(sed -n "$((count / 2 + 1))p" "$values_file")"
        awk -v lower="$lower" -v upper="$upper" 'BEGIN { printf "%.2f", (lower + upper) / 2 }'
    fi
}

p90_from_sorted_file() {
    local values_file="$1"
    local count rank
    count="$(wc -l < "$values_file" | tr -d ' ')"
    rank=$(((9 * count + 9) / 10))
    sed -n "${rank}p" "$values_file"
}

summarize_rss() {
    local label="$1"
    local profile="$2"
    local values_file="${tmp_dir}/${label}-${profile}-rss.txt"
    extract_sorted_values "$label" "$profile" "$values_file"
    local min max median p90
    min="$(sed -n '1p' "$values_file")"
    max="$(tail -n 1 "$values_file")"
    median="$(median_from_sorted_file "$values_file")"
    p90="$(p90_from_sorted_file "$values_file")"
    printf '%s,%s,%s,%s\n' "$median" "$min" "$max" "$p90"
}

binary_size() {
    local label="$1"
    local binary="$2"
    awk -F, -v label="$label" -v binary="$binary" \
        '$1 == label && $2 == binary { print $3 }' "$binary_csv"
}

echo "==> Building release binaries"
build_release_binaries base "$base_worktree"
build_release_binaries current "$current_worktree"
prepare_mantle_artifact base "$base_worktree"
prepare_mantle_artifact current "$current_worktree"
write_binary_sizes
write_metadata

printf 'label,profile,run,max_rss_kib\n' > "$rss_csv"
profiles=(strata_check strata_build mantle_run)
for profile in "${profiles[@]}"; do
    for ((run_index = 1; run_index <= runs; run_index++)); do
        run_cli_profile base "$base_worktree" "$profile" "$run_index"
        run_cli_profile current "$current_worktree" "$profile" "$run_index"
    done
done

printf 'binary,base_file_bytes,current_file_bytes,delta_file_bytes\n'
for binary in strata mantle; do
    base_bytes="$(binary_size base "$binary")"
    current_bytes="$(binary_size current "$binary")"
    delta_bytes="$((current_bytes - base_bytes))"
    printf '%s,%s,%s,%s\n' "$binary" "$base_bytes" "$current_bytes" "$delta_bytes"
done

printf 'profile,base_median_kib,current_median_kib,delta_median_kib,base_min_kib,current_min_kib,base_max_kib,current_max_kib,base_p90_kib,current_p90_kib,delta_p90_kib\n'
for profile in "${profiles[@]}"; do
    IFS=, read -r base_median base_min base_max base_p90 < <(summarize_rss base "$profile")
    IFS=, read -r current_median current_min current_max current_p90 < <(summarize_rss current "$profile")
    delta_median="$(awk -v current="$current_median" -v base="$base_median" 'BEGIN { printf "%.2f", current - base }')"
    delta_p90="$(awk -v current="$current_p90" -v base="$base_p90" 'BEGIN { printf "%.2f", current - base }')"
    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
        "$profile" "$base_median" "$current_median" "$delta_median" \
        "$base_min" "$current_min" "$base_max" "$current_max" \
        "$base_p90" "$current_p90" "$delta_p90"
done

echo "binary_size_csv=${binary_csv}"
echo "rss_samples_csv=${rss_csv}"
echo "metadata_csv=${metadata_csv}"
