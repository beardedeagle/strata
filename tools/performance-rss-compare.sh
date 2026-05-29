#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat >&2 <<'USAGE'
Usage:
  tools/performance-rss-compare.sh [--runs N] BASE_WORKTREE CURRENT_WORKTREE [PROFILE ...]

Profiles default to:
  collection_state.check_lower
  collection_state.in_memory_runtime

Additional supported profile when both worktrees contain the example:
  local_supervision_restart.in_memory_runtime
  imports_main.check_lower

Each sample runs one profile in a fresh cargo test process and reports RSS
median/min/max/p90 deltas in KiB.
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
probe_source="${repo_root}/tools/performance_rss_probe.rs"
runs=30

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

if [[ $# -lt 2 ]]; then
    usage
    exit 2
fi

if [[ ! "$runs" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: --runs must be a positive integer, got $runs" >&2
    exit 2
fi

base_worktree="$1"
current_worktree="$2"
shift 2

profiles=("$@")
if [[ ${#profiles[@]} -eq 0 ]]; then
    profiles=(
        "collection_state.check_lower"
        "collection_state.in_memory_runtime"
    )
fi

for worktree in "$base_worktree" "$current_worktree"; do
    if [[ ! -f "${worktree}/Cargo.toml" ]]; then
        echo "Error: ${worktree} is not a Strata worktree with Cargo.toml" >&2
        exit 2
    fi
    if [[ ! -f "${worktree}/crates/strata-mantle-acceptance/tests/performance_smoke/allocation_meter.rs" ]]; then
        echo "Error: ${worktree} does not contain the performance smoke allocation meter" >&2
        exit 2
    fi
done

if [[ ! -f "$probe_source" ]]; then
    echo "Error: missing RSS probe source at $probe_source" >&2
    exit 2
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/strata-rss-compare.XXXXXX")"
output_dir="${STRATA_PERFORMANCE_RSS_OUTPUT_DIR:-${repo_root}/target/performance-rss}"
mkdir -p "$output_dir"
csv_path="${output_dir}/rss-samples-$(date +%Y%m%d%H%M%S).csv"
probe_targets=()

cleanup() {
    for probe_target in "${probe_targets[@]}"; do
        rm -f "$probe_target"
    done
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

install_probe() {
    local worktree="$1"
    local target="${worktree}/crates/strata-mantle-acceptance/tests/performance_rss_probe.rs"
    if [[ -e "$target" || -L "$target" ]]; then
        echo "Error: refusing to overwrite existing probe target $target" >&2
        exit 2
    fi
    local target_dir
    local temp_target
    target_dir="$(dirname "$target")"
    temp_target="${target_dir}/.performance_rss_probe.$$"
    install -m 0644 "$probe_source" "$temp_target"
    mv -n "$temp_target" "$target"
    if [[ -e "$temp_target" ]]; then
        rm -f "$temp_target"
        echo "Error: refusing to overwrite existing probe target $target" >&2
        exit 2
    fi
    probe_targets+=("$target")
}

install_probe "$base_worktree"
install_probe "$current_worktree"

printf 'label,profile,run,current_rss_kib,wall_nanos,cpu_nanos,allocation_count,allocated_bytes,net_live_bytes_delta,peak_live_bytes_over_start\n' > "$csv_path"

field_value() {
    local line="$1"
    local key="$2"
    local token
    for token in $line; do
        if [[ "$token" == "${key}="* ]]; then
            printf '%s\n' "${token#*=}"
            return 0
        fi
    done
    return 1
}

run_sample() {
    local label="$1"
    local worktree="$2"
    local profile="$3"
    local run_index="$4"
    local log_path="${tmp_dir}/${label}-${profile}-${run_index}.log"

    if ! (
        cd "$worktree"
        STRATA_RSS_PROBE_PROFILE="$profile" \
            cargo +stable test -p strata-mantle-acceptance --test performance_rss_probe \
            -- --ignored --nocapture rss_probe_runs_selected_profile
    ) >"$log_path" 2>&1; then
        cat "$log_path" >&2
        echo "Error: RSS probe failed for ${label} ${profile} run ${run_index}" >&2
        exit 1
    fi

    local line
    line="$(awk '/^RSS_PROBE_METRICS / { line = $0 } END { print line }' "$log_path")"
    if [[ -z "$line" ]]; then
        cat "$log_path" >&2
        echo "Error: RSS probe did not emit metrics for ${label} ${profile} run ${run_index}" >&2
        exit 1
    fi

    local current_rss_kib wall_nanos cpu_nanos allocation_count allocated_bytes net_live peak_live
    current_rss_kib="$(field_value "$line" current_rss_kib)"
    wall_nanos="$(field_value "$line" wall_nanos)"
    cpu_nanos="$(field_value "$line" cpu_nanos)"
    allocation_count="$(field_value "$line" allocation_count)"
    allocated_bytes="$(field_value "$line" allocated_bytes)"
    net_live="$(field_value "$line" net_live_bytes_delta)"
    peak_live="$(field_value "$line" peak_live_bytes_over_start)"

    if [[ ! "$current_rss_kib" =~ ^[0-9]+$ ]]; then
        cat "$log_path" >&2
        echo "Error: RSS probe reported non-numeric current_rss_kib=$current_rss_kib" >&2
        exit 1
    fi

    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
        "$label" "$profile" "$run_index" "$current_rss_kib" "$wall_nanos" "$cpu_nanos" \
        "$allocation_count" "$allocated_bytes" "$net_live" "$peak_live" >> "$csv_path"
}

for profile in "${profiles[@]}"; do
    for ((run_index = 1; run_index <= runs; run_index++)); do
        run_sample base "$base_worktree" "$profile" "$run_index"
        run_sample current "$current_worktree" "$profile" "$run_index"
    done
done

extract_sorted_values() {
    local label="$1"
    local profile="$2"
    local column="$3"
    local output="$4"
    awk -F, -v label="$label" -v profile="$profile" -v column="$column" \
        '$1 == label && $2 == profile { print $column }' "$csv_path" | sort -n > "$output"
    if [[ ! -s "$output" ]]; then
        echo "Error: no samples found for ${label} ${profile}" >&2
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
    extract_sorted_values "$label" "$profile" 4 "$values_file"
    local min max median p90
    min="$(sed -n '1p' "$values_file")"
    max="$(tail -n 1 "$values_file")"
    median="$(median_from_sorted_file "$values_file")"
    p90="$(p90_from_sorted_file "$values_file")"
    printf '%s,%s,%s,%s\n' "$median" "$min" "$max" "$p90"
}

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

echo "raw_samples_csv=${csv_path}"
