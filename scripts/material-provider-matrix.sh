#!/usr/bin/env bash
# Provider-backed R5.5 material-role capture matrix.
#
# Runs one deterministic scene for Oblivion, FNV, Skyrim SE, FO4 and Starfield.
# Each process is switched live through direct-only, material-lobe
# and material-role views, so all three captures share one loaded world. The
# complete mat.list / sampled mat.dump / tex.missing output is retained beside
# the images. Three runs are required by default and pixel-domain tolerances
# gate repeatability; SHA-256 hashes remain in the manifest as provenance. A
# missing title is SKIP (77), never a pass.
#
# Usage: scripts/material-provider-matrix.sh [runs] [warmup_frames]
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
    echo "material-provider-matrix: runs and warmup_frames must be positive integers" >&2
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
        # AAAMarkers is a shipped interior using the same Starfield BA2/CDB
        # provider stack as city cells, with a compact 150-ish-mesh scene that
        # keeps live debug-oracle frames responsive in CI.
        starfield) args=(--game starfield --cell aaamarkers) ;;
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
            BYRO_DEBUG_PORT="${port}" \
            RUST_LOG="${BYROREDUX_MATERIAL_MATRIX_LOG:-warn,byroredux::app_events=info}" \
            timeout "${timeout_seconds}" "${engine}" \
            --games-root "${games_root}" "${args[@]}" \
            --upscaler taa --render-debug-mode material_lobe \
            --bench-frames 100000000 --bench-mode renderer-static \
            > "${engine_stdout}" 2> "${engine_stderr}" &
        engine_pid=$!

        deadline=$(( $(date +%s) + timeout_seconds ))
        while ! rg -q 'Engine ready — entering game loop' "${engine_stderr}" 2>/dev/null; do
            if ! kill -0 "${engine_pid}" 2>/dev/null; then
                echo "material-provider-matrix: ${game} exited before the render loop" >&2
                tail -40 "${engine_stderr}" >&2 || true
                exit 1
            fi
            if (( $(date +%s) > deadline )); then
                echo "material-provider-matrix: ${game} timed out waiting for the render loop" >&2
                exit 1
            fi
            sleep 0.25
        done

        if rg -q 'was specified but 0 .* archives opened' "${engine_stderr}"; then
            echo "material-provider-matrix: ${game} archive provider failed to open" >&2
            exit 1
        fi

        warmup_log="${run_out}/warmup.log"
        {
            for _ in $(seq 1 "${frames}"); do
                # The protocol handles one request per scheduler drain, so N
                # successful stats replies are an exact N-frame warmup.
                printf 'stats\n'
            done
            printf 'quit\n'
        } | env BYRO_DEBUG_PORT="${port}" "${debugger}" > "${warmup_log}" 2>&1

        printf 'stats\nrender.debug material_lobe\nmat.list 8\nquit\n' \
            | env BYRO_DEBUG_PORT="${port}" "${debugger}" > "${list_log}" 2>&1
        entities="$(sed -n 's/.*Entities:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "${list_log}" | head -1)"
        draws="$(sed -n 's/.*Draws:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "${list_log}" | head -1)"
        floor="$(entity_floor "${game}")"
        if (( ${entities:-0} < floor )); then
            echo "material-provider-matrix: ${game} near-empty load: entities=${entities:-0}, floor=${floor}" >&2
            exit 1
        fi

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
            for mode in material_lobe material_role direct_only; do
                printf 'render.debug %s\n' "${mode}"
                if [[ "${mode}" == direct_only ]]; then
                    settle_frames=30
                else
                    settle_frames=5
                fi
                for _ in $(seq 1 "${settle_frames}"); do
                    printf 'stats\n'
                done
                printf 'screenshot %s\n' "${run_out}/${mode}.png"
            done
            printf 'quit\n'
        } | env BYRO_DEBUG_PORT="${port}" "${debugger}" > "${debug_log}" 2>&1

        if ! rg -q 'base_color.*sRGB.*2D' "${debug_log}" \
            || ! rg -q 'normal.*linear.*2D' "${debug_log}"; then
            echo "material-provider-matrix: ${game} sampled dumps lack canonical role/view rows" >&2
            exit 1
        fi
        case "${game}" in
            oblivion|fnv|skyrim_se)
                if ! rg -q 'nif-texture-set' "${debug_log}"; then
                    echo "material-provider-matrix: ${game} exposed no inline NIF texture provenance" >&2
                    exit 1
                fi
                ;;
            fo4)
                if ! rg -q 'present[[:space:]]+(bgsm|bgem)' "${debug_log}"; then
                    echo "material-provider-matrix: fo4 exposed no BGSM/BGEM-filled texture role" >&2
                    exit 1
                fi
                ;;
            starfield)
                if ! rg -q 'material_path=.*\.mat' "${debug_log}"; then
                    echo "material-provider-matrix: starfield exposed no CDB-backed .mat reference" >&2
                    exit 1
                fi
                ;;
        esac
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
                if [[ "${game}" == oblivion ]]; then
                    # The Market District's many alpha-tested leaves produce
                    # broad low-amplitude RT-reservoir noise across processes.
                    # Bound aggregate error instead of requiring pixel identity.
                    max_changed=0.45
                    max_mean=3.0
                else
                    max_changed=0.08
                    max_mean=2.5
                fi
            else
                max_changed=0.006
                max_mean=0.6
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
    printf 'runs=%s\nwarmup_frames=%s\n' "${runs}" "${frames}"
    nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || true
} > "${out}/metadata.txt"
find "${out}" -type f ! -name sha256sums.txt -print0 \
    | sort -z \
    | xargs -0 sha256sum > "${out}/sha256sums.txt"

echo "material-provider-matrix: PASS — artifacts retained at ${out}" >&2
