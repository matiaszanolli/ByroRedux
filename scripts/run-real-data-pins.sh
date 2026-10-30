#!/usr/bin/env bash
# Run every closed-HIGH/CRITICAL real-data pin, one cargo process per pin.
#
#   scripts/run-real-data-pins.sh              # all pins in .github/real-data-pins.txt
#   scripts/run-real-data-pins.sh skyrim       # only pins whose line contains "skyrim"
#   scripts/run-real-data-pins.sh --list       # print the pins and exit
#
# The pin list is a data file shared with `.github/workflows/real-data-gates.yml`
# (the `closed-high-pins` job just calls this script), so the maintainer's local
# run and the self-hosted runner are the same command (#6182). This repository
# has no registered runner today (#5905), so the local run is the one that
# actually happens: `/audit-runtime` and any audit that touches a pinned area
# should run it and report pass / fail beside its own findings.
#
# One pin per process is the memory bound: a single `parse_real_esm.rs` process
# that loads every master spikes past 20 GB. The runner refuses a green no-op, so
# a typo'd title fails instead of passing vacuously.
#
# Game data: on the CI runner `GAMES_ROOT` (and optionally `SKYRIMLE_DATA`) point
# at the installs and are expanded into the per-game `BYROREDUX_*_DATA`
# variables. Locally, leave `GAMES_ROOT` unset and the tests resolve their
# installs through the usual `BYROREDUX_*_DATA` variables / `test_paths`
# defaults. `BYROREDUX_REQUIRE_GAME_DATA` defaults to 1 (#3850), so an absent
# install is a failure rather than a silent skip; export 0 to override.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
pins_file=".github/real-data-pins.txt"
[ -f "${pins_file}" ] || { echo "missing ${pins_file}" >&2; exit 2; }

# Pins are "<lane> <title>" lines; blank lines and `#` comments are ignored.
mapfile -t pins < <(grep -vE '^[[:space:]]*(#|$)' "${pins_file}")

if [ "${1:-}" = "--list" ]; then
    printf '%s\n' "${pins[@]}"
    exit 0
fi
filter="${1:-}"

export BYROREDUX_REQUIRE_GAME_DATA="${BYROREDUX_REQUIRE_GAME_DATA:-1}"
if [ -n "${GAMES_ROOT:-}" ]; then
    export BYROREDUX_OBLIVION_DATA="${GAMES_ROOT}/Oblivion/Data"
    export BYROREDUX_FO3_DATA="${GAMES_ROOT}/Fallout 3 goty/Data"
    export BYROREDUX_FNV_DATA="${GAMES_ROOT}/Fallout New Vegas/Data"
    export BYROREDUX_SKYRIMSE_DATA="${GAMES_ROOT}/Skyrim Special Edition/Data"
    export BYROREDUX_FO4_DATA="${GAMES_ROOT}/Fallout 4/Data"
    export BYROREDUX_FO76_DATA="${GAMES_ROOT}/Fallout76/Data"
    export BYROREDUX_STARFIELD_DATA="${GAMES_ROOT}/Starfield/Data"
fi
if [ -n "${SKYRIMLE_DATA:-}" ]; then
    export BYROREDUX_SKYRIMLE_DATA="${SKYRIMLE_DATA}"
fi

log_dir="${PINS_LOG_DIR:-target/real-data-gates}"
mkdir -p "${log_dir}"
# GitHub Actions folds log groups; locally the markers are harmless text.
in_ci="${GITHUB_ACTIONS:-}"
group() { [ -n "${in_ci}" ] && echo "::group::$1" || echo "== $1"; }
endgroup() { [ -n "${in_ci}" ] && echo "::endgroup::" || true; }
error() { [ -n "${in_ci}" ] && echo "::error::$1" || echo "ERROR: $1" >&2; }

failed=0
ran=0
for pin in "${pins[@]}"; do
    if [ -n "${filter}" ] && [[ "${pin}" != *"${filter}"* ]]; then
        continue
    fi
    lane="${pin%% *}"
    title="${pin#* }"
    case "${lane}" in
        plugin-lib) target="-p byroredux-plugin --lib" ;;
        plugin-esm) target="-p byroredux-plugin --test parse_real_esm" ;;
        scripting-lib) target="-p byroredux-scripting --lib" ;;
        scripting-catalog) target="-p byroredux-scripting --test condition_catalog_real_data" ;;
        bin) target="-p byroredux --bin byroredux" ;;
        nif-corpus) target="-p byroredux-nif --test normal_synthesis_corpus --test translation_completeness --test parse_real_nifs" ;;
        *) error "unknown lane '${lane}' in ${pins_file} (pin '${title}')"; failed=1; continue ;;
    esac
    ran=$((ran + 1))
    log="${log_dir}/pins-${title}.log"
    group "${title}"
    # shellcheck disable=SC2086 — ${target} is a word list by design.
    if ! cargo test --release --no-fail-fast ${target} "${title}" \
        -- --ignored --test-threads=1 --nocapture 2>&1 | tee "${log}"; then
        endgroup
        error "closed-HIGH pin '${title}' failed"
        failed=1
        continue
    fi
    endgroup
    # Refuse the green no-op, as every other job in this lane does. Summed from
    # the per-binary summaries: --nocapture splits a printing test's
    # `test ... ok` line (#6319).
    ran_ok=$(scripts/count-passed-tests.sh "${log}")
    if [ "${ran_ok}" -eq 0 ]; then
        error "closed-HIGH pin '${title}' matched nothing (green no-op)"
        failed=1
    else
        echo "pin '${title}': ${ran_ok} test(s) ran"
    fi
done

if [ "${ran}" -eq 0 ]; then
    error "no pin matched filter '${filter}'"
    exit 1
fi
if [ "${failed}" -eq 0 ]; then
    echo "all ${ran} closed-HIGH pins green"
fi
exit "${failed}"
