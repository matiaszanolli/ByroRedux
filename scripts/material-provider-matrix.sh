#!/usr/bin/env bash
# Provider-backed R5.5 material-role capture matrix.
#
# Runs one deterministic held scene for Oblivion, FNV, Skyrim SE, FO4 and
# Starfield. Each process is switched live through direct-only, material-lobe
# and material-role views, so all three captures share one loaded world. The
# complete mat.list / sampled mat.dump / tex.missing output is retained beside
# the images. Three runs are required by default and pixel-domain tolerances
# gate repeatability; SHA-256 hashes remain in the manifest as provenance. A
# missing title is SKIP (77), never a pass.
#
# Usage: scripts/material-provider-matrix.sh [runs] [bench_frames]
# Environment:
#   BYROREDUX_MATERIAL_MATRIX_OUT=target/material-provider-matrix
#   BYROREDUX_MATERIAL_MATRIX_GAMES="oblivion fnv skyrim_se fo4 starfield"
#   BYROREDUX_GAMES_ROOT=/mnt/data/SteamLibrary/steamapps/common
#   BYROREDUX_MATERIAL_MATRIX_TIMEOUT=600
# Run headless with: xvfb-run -a --server-args="-screen 0 1280x720x24" \
#   bash -c 'scripts/material-provider-matrix.sh 3 30'

set -euo pipefail

runs="${1:-3}"
frames="${2:-30}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
engine="${repo}/target/release/byroredux"
debugger="${repo}/target/release/byro-dbg"
games_root="${BYROREDUX_GAMES_ROOT:-/mnt/data/SteamLibrary/steamapps/common}"
out="${BYROREDUX_MATERIAL_MATRIX_OUT:-${repo}/target/material-provider-matrix}"
games_text="${BYROREDUX_MATERIAL_MATRIX_GAMES:-oblivion fnv skyrim_se fo4 starfield}"
timeout_seconds="${BYROREDUX_MATERIAL_MATRIX_TIMEOUT:-600}"
read -r -a games <<< "${games_text}"

if [[ ! "${runs}" =~ ^[1-9][0-9]*$ || ! "${frames}" =~ ^[1-9][0-9]*$ ]]; then
    echo "material-provider-matrix: runs and bench_frames must be positive integers" >&2
    exit 2
fi
if [[ ! "${timeout_seconds}" =~ ^[1-9][0-9]*$ ]]; then
    echo "material-provider-matrix: timeout must be a positive integer" >&2
    exit 2
fi
if [[ ! -x "${engine}" || ! -x "${debugger}" ]]; then
    echo "material-provider-matrix: build binaries first: cargo build --release -p byroredux -p byro-dbg" >&2
    exit 2
fi

mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
manifest="${out}/manifest.tsv"
printf 'game\trun\tmode\tsha256\tentities\tdraws\tmissing_unique\tsampled_materials\timage\n' > "${manifest}"

engine_pid=""
cleanup_engine() {
    if [[ -n "${engine_pid}" ]] && kill -0 "${engine_pid}" 2>/dev/null; then
        kill -TERM "${engine_pid}" 2>/dev/null || true
        wait "${engine_pid}" 2>/dev/null || true
    fi
    engine_pid=""
}
trap cleanup_engine EXIT INT TERM

game_dir() {
    case "$1" in
        oblivion) echo "${games_root}/Oblivion/Data" ;;
        fnv) echo "${games_root}/Fallout New Vegas/Data" ;;
        skyrim_se) echo "${games_root}/Skyrim Special Edition/Data" ;;
        fo4) echo "${games_root}/Fallout 4/Data" ;;
        starfield) echo "${games_root}/Starfield/Data" ;;
        *) return 1 ;;
    esac
}

game_args() {
    case "$1" in
        oblivion) args=(--game oblivion --cell ICMarketDistrictTheGildedCarafe) ;;
        fnv) args=(--game fnv --cell GSProspectorSaloonInterior) ;;
        skyrim_se) args=(--game skyrim_se --cell WhiterunBanneredMare) ;;
        fo4) args=(--game fo4 --cell MedTekResearch01) ;;
        starfield) args=(--game starfield --cell citycydoniamainlevel) ;;
        *) return 1 ;;
    esac
}

entity_floor() {
    case "$1" in
        oblivion) echo 100 ;;
        fnv) echo 1800 ;;
        skyrim_se) echo 2500 ;;
        fo4) echo 16000 ;;
        # The first-render contract only promises 50 rendered REFRs. Keep the
        # floor tied to that documented minimum instead of today's much larger
        # Cydonia count, which changes as SF record coverage expands.
        starfield) echo 50 ;;
    esac
}

extract_material_ids() {
    python3 - "$1" <<'PY'
import json
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(errors="replace")
for line in text.splitlines():
    marker = line.find('"')
    if marker < 0:
        continue
    try:
        payload = json.loads(line[marker:])
    except json.JSONDecodeError:
        continue
    if not isinstance(payload, str) or "diffuse(rgb)" not in payload:
        continue
    count = 0
    for row in payload.splitlines()[1:]:
        match = re.match(r"\s*(\d+)\s+", row)
        if match:
            print(match.group(1))
            count += 1
            if count == 8:
                raise SystemExit
PY
}

for game in "${games[@]}"; do
    data_dir="$(game_dir "${game}")" || {
        echo "material-provider-matrix: unknown game '${game}'" >&2
        exit 2
    }
    if [[ ! -d "${data_dir}" ]]; then
        echo "material-provider-matrix: SKIP — required ${game} data missing at ${data_dir}" >&2
        exit 77
    fi
    game_args "${game}"
    game_out="${out}/${game}"
    mkdir -p "${game_out}"

    for run in $(seq 1 "${runs}"); do
        run_out="${game_out}/run-${run}"
        mkdir -p "${run_out}"
        engine_stdout="${run_out}/engine.stdout.log"
        engine_stderr="${run_out}/engine.stderr.log"
        list_log="${run_out}/mat-list.log"
        debug_log="${run_out}/debug.log"
        port=$((21000 + ($$ % 10000)))

        echo "material-provider-matrix: ${game} run ${run}/${runs}" >&2
        env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE \
            BYRO_DEBUG_PORT="${port}" RUST_LOG="${BYROREDUX_MATERIAL_MATRIX_LOG:-warn}" \
            timeout "${timeout_seconds}" "${engine}" \
            --games-root "${games_root}" "${args[@]}" \
            --upscaler taa --bench-frames "${frames}" \
            --bench-mode renderer-static --bench-hold \
            > "${engine_stdout}" 2> "${engine_stderr}" &
        engine_pid=$!

        deadline=$(( $(date +%s) + timeout_seconds ))
        while ! rg -q '^bench-hold:' "${engine_stderr}" 2>/dev/null; do
            if ! kill -0 "${engine_pid}" 2>/dev/null; then
                echo "material-provider-matrix: ${game} exited before bench-hold" >&2
                tail -40 "${engine_stderr}" >&2 || true
                exit 1
            fi
            if (( $(date +%s) > deadline )); then
                echo "material-provider-matrix: ${game} timed out waiting for bench-hold" >&2
                exit 1
            fi
            sleep 0.25
        done

        bench_line="$(awk '/^bench:/{line=$0} END{print line}' "${engine_stdout}")"
        if [[ -z "${bench_line}" ]]; then
            echo "material-provider-matrix: ${game} produced no bench line" >&2
            exit 1
        fi
        if rg -q 'was specified but 0 .* archives opened' "${engine_stderr}"; then
            echo "material-provider-matrix: ${game} archive provider failed to open" >&2
            exit 1
        fi
        entities="$(sed -n 's/.* entities=\([0-9][0-9]*\).*/\1/p' <<< "${bench_line}")"
        draws="$(sed -n 's/.* draws=\([0-9][0-9]*\).*/\1/p' <<< "${bench_line}")"
        floor="$(entity_floor "${game}")"
        if (( ${entities:-0} < floor )); then
            echo "material-provider-matrix: ${game} near-empty load: entities=${entities:-0}, floor=${floor}" >&2
            exit 1
        fi

        printf 'mat.list\nquit\n' \
            | env BYRO_DEBUG_PORT="${port}" "${debugger}" > "${list_log}" 2>&1
        mapfile -t material_ids < <(extract_material_ids "${list_log}")
        if (( ${#material_ids[@]} == 0 )); then
            echo "material-provider-matrix: ${game} mat.list exposed no material entities" >&2
            exit 1
        fi

        {
            for entity in "${material_ids[@]}"; do
                printf 'mat.dump %s\n' "${entity}"
            done
            printf 'tex.missing entities\n'
            for mode in direct_only material_lobe material_role; do
                printf 'render.debug %s\n' "${mode}"
                printf 'screenshot %s\n' "${run_out}/${mode}.png"
            done
            printf 'quit\n'
        } | env BYRO_DEBUG_PORT="${port}" "${debugger}" > "${debug_log}" 2>&1

        if ! rg -q 'base_color.*sRGB.*2D' "${debug_log}" \
            || ! rg -q 'normal.*linear.*2D' "${debug_log}"; then
            echo "material-provider-matrix: ${game} sampled dumps lack canonical role/view rows" >&2
            exit 1
        fi
        oracle_count="$(rg -c 'texture oracle: unavailable' "${debug_log}" || true)"
        if (( oracle_count == ${#material_ids[@]} )); then
            echo "material-provider-matrix: ${game} sampled materials never reached the texture oracle" >&2
            exit 1
        fi
        missing_unique="$(sed -n 's/.*\\n\([0-9][0-9]*\) unique missing textures:.*/\1/p' "${debug_log}" | head -1)"
        : "${missing_unique:=0}"

        for mode in direct_only material_lobe material_role; do
            image="${run_out}/${mode}.png"
            if [[ ! -s "${image}" ]]; then
                echo "material-provider-matrix: ${game}/${mode} screenshot missing" >&2
                exit 1
            fi
            hash="$(sha256sum "${image}" | awk '{print $1}')"
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "${game}" "${run}" "${mode}" "${hash}" "${entities}" "${draws:-0}" \
                "${missing_unique}" "${#material_ids[@]}" "${image#${out}/}" >> "${manifest}"
        done

        cleanup_engine
    done
done

if (( runs > 1 )); then
    for game in "${games[@]}"; do
        for mode in direct_only material_lobe material_role; do
            images=()
            for run in $(seq 1 "${runs}"); do
                images+=("${out}/${game}/run-${run}/${mode}.png")
            done
            if [[ "${mode}" == direct_only ]]; then
                max_changed=0.05
                max_mean=1.0
            else
                max_changed=0.002
                max_mean=0.2
            fi
            python3 "${repo}/scripts/png-stability.py" \
                --channel-tolerance 2 \
                --max-changed-fraction "${max_changed}" \
                --max-mean-absolute-error "${max_mean}" \
                "${images[@]}" | tee "${out}/${game}/${mode}-stability.txt"
        done
    done
fi

python3 - "${manifest}" "${runs}" <<'PY'
import csv
import sys
from collections import defaultdict

manifest, expected_runs = sys.argv[1], int(sys.argv[2])
groups = defaultdict(list)
with open(manifest, newline="") as stream:
    for row in csv.DictReader(stream, delimiter="\t"):
        groups[(row["game"], row["mode"])].append(row)

errors = []
for (game, mode), rows in sorted(groups.items()):
    hashes = {row["sha256"] for row in rows}
    if len(rows) != expected_runs:
        errors.append(f"{game}/{mode}: {len(rows)} runs, expected {expected_runs}")
    print(f"{game:10} {mode:14} runs={len(rows)} unique_hashes={len(hashes)} sha256={rows[0]['sha256'][:16]}")

if errors:
    raise SystemExit("material-provider-matrix failed:\n  " + "\n  ".join(errors))
PY

{
    printf 'revision=%s\n' "$(git -C "${repo}" rev-parse HEAD)"
    printf 'tree_dirty=%s\n' "$(if [[ -n "$(git -C "${repo}" status --porcelain)" ]]; then echo true; else echo false; fi)"
    printf 'timestamp_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'runs=%s\nframes=%s\n' "${runs}" "${frames}"
    nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || true
} > "${out}/metadata.txt"
find "${out}" -type f ! -name sha256sums.txt -print0 \
    | sort -z \
    | xargs -0 sha256sum > "${out}/sha256sums.txt"

echo "material-provider-matrix: PASS — artifacts retained at ${out}" >&2
