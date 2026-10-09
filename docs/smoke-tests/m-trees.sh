#!/usr/bin/env bash
# SpeedTree compatibility smoke test — Phase 1.7 close-out gate.
# Verifies that pre-Skyrim TREE REFRs spawn as renderable billboard
# placeholders end-to-end:
#
#   TREE record (Phase 1.1) → `.spt` parser (Phase 1.3) → SPT importer
#   (Phase 1.4) → cell loader extension switch (Phase 1.5)
#                                                → Billboard ECS entity
#
# Pre-fix every FNV / FO3 / Oblivion exterior cell silently dropped
# every TREE REFR because the cell loader fed `.spt` bytes into the NIF
# parser and the magic-header check rejected them. Mojave Wasteland
# rendered as flat sand with zero foliage.
#
# Workflow (mirrors `m41-equip.sh`):
#   1. Spawn the engine in the background under `--bench-frames N
#      --bench-hold` so the bench summary lands and the debug server
#      stays reachable.
#   2. Wait for the `bench-hold:` notice in the engine's stderr.
#   3. Pipe a command sequence into `byro-dbg`:
#        - `entities Billboard` — count of billboard-flagged entities.
#                                 Every SPT placeholder spawns one.
#        - `tex.missing`        — should be small. The leaf icon is
#                                 the only texture per tree placeholder
#                                 and is shared across REFRs.
#   4. SIGTERM the engine and collect its bench summary.
#
# Usage:
#   docs/smoke-tests/m-trees.sh [fnv|fo3|obl|all]
#
# Exit: 0 on success, non-zero on any cell whose entity count or
# Billboard count falls outside the expected band.

set -euo pipefail

GAME="${1:-all}"

FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
FO3_DATA="${BYROREDUX_FO3_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data}"
OBL_DATA="${BYROREDUX_OBLIVION_DATA:-/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data}"

PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

# Args: $1 = label, $2 = entity floor (hard), $3 = billboard floor (hard),
#       $4 = tex.missing ceiling (soft warn), then engine CLI args.
run_cell () {
    local label="$1" ; shift
    local entities_floor="$1" ; shift
    local billboards_floor="$1" ; shift
    local tex_miss_ceiling="$1" ; shift
    local engine_log="$LOG_DIR/$label.engine.log"
    local dbg_log="$LOG_DIR/$label.dbg.log"

    echo "═══════════════════════════════════════════════════════════════"
    echo "  smoke[$label]: launching engine in background"
    echo "═══════════════════════════════════════════════════════════════"

    # Release binaries gate the debug server behind this opt-in
    # (63c0aee3b); without it the held engine binds no port and byro-dbg
    # can never attach.
    export BYRO_DEBUG_SERVER=1
    cargo run --release --quiet -- \
        "$@" \
        --bench-frames "$BENCH_FRAMES" \
        --bench-hold \
        > "$engine_log.stdout" 2> "$engine_log.stderr" &
    local engine_pid=$!

    local kill_engine='kill -TERM "$engine_pid" 2>/dev/null || true; wait "$engine_pid" 2>/dev/null || true'

    local timeout=180
    local deadline=$(( $(date +%s) + timeout ))
    while ! grep -q "^bench-hold:" "$engine_log.stderr" 2>/dev/null; do
        if [[ $(date +%s) -gt $deadline ]]; then
            echo "smoke[$label]: TIMEOUT waiting for bench-hold (logs: $engine_log.stderr)"
            eval "$kill_engine"
            return 1
        fi
        if ! kill -0 "$engine_pid" 2>/dev/null; then
            echo "smoke[$label]: engine exited before bench-hold (logs: $engine_log.stderr)"
            tail -20 "$engine_log.stderr"
            return 1
        fi
        sleep 0.5
    done

    echo "smoke[$label]: engine ready, attaching byro-dbg on port $PORT"

    BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<EOF > "$dbg_log" 2>&1 || true
entities Billboard
tex.missing
quit
EOF

    echo "smoke[$label]: byro-dbg session complete, captured to $dbg_log"
    echo
    echo "── byro-dbg session log [$label] ───────────────────────────────"
    cat "$dbg_log"
    echo "── engine bench summary [$label] ───────────────────────────────"
    local bench_line
    bench_line=$(grep "^bench:" "$engine_log.stdout" || true)
    if [[ -z "$bench_line" ]]; then
        echo "  (no bench: line found)"
        eval "$kill_engine"
        echo "smoke[$label]: FAIL — no bench summary"
        return 1
    fi
    echo "$bench_line"
    echo

    # ── HARD assertions ───────────────────────────────────────────
    local entities draws
    entities=$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)
    draws=$(echo "$bench_line" | grep -oE 'draws=[0-9]+' | head -1 | cut -d= -f2)
    : "${entities:=0}"
    : "${draws:=0}"

    # #5142 — a missing `entities` answer is an attach failure, not an
    # empty world: the old `|| echo 0` parse read a dead byro-dbg session
    # as "0 billboards" and pointed the HARD FAIL at the (healthy) .spt
    # extension switch.
    # #5360 — `(no entities)` is a *valid* answer (count 0): byro-dbg
    # prints it for an empty EntityList, and in piped-stdin mode it rides
    # after the unterminated `byro> ` prompt, so the match is unanchored.
    # Treating it as "never answered" inverted the diagnosis — a broken
    # .spt route (the gate's own target regression) was blamed on the
    # attach.
    if ! grep -qE '\((no|[0-9]+) entities\)' "$dbg_log"; then
        echo "smoke[$label]: HARD FAIL — byro-dbg never answered 'entities Billboard' \
(no '(N entities)' or '(no entities)' row in $dbg_log — the attach itself failed; see the session log above)"
        eval "$kill_engine"
        return 1
    fi
    local billboards
    billboards=$(grep -oE '\((no|[0-9]+) entities\)' "$dbg_log" | sed -n '1p' \
        | sed 's/(no entities)/(0 entities)/' | grep -oE '[0-9]+' || echo 0)
    : "${billboards:=0}"

    local tex_miss
    tex_miss=$(grep -oE '[0-9]+ unique missing textures' "$dbg_log" | grep -oE '^[0-9]+' || echo 0)
    : "${tex_miss:=0}"

    local hard_fail=0
    if (( entities < entities_floor )); then
        echo "smoke[$label]: HARD FAIL — entities=$entities < floor $entities_floor"
        hard_fail=1
    else
        echo "smoke[$label]: PASS — entities=$entities >= $entities_floor"
    fi
    if (( billboards < billboards_floor )); then
        echo "smoke[$label]: HARD FAIL — Billboard entities=$billboards < floor $billboards_floor"
        echo "                 (every TREE REFR should spawn a billboard placeholder; \
zero indicates the .spt extension switch isn't routing — see Phase 1.5)"
        hard_fail=1
    else
        echo "smoke[$label]: PASS — Billboard entities=$billboards >= $billboards_floor"
    fi

    # ── SOFT assertions ───────────────────────────────────────────
    echo "smoke[$label]: tex.missing=$tex_miss unique"
    if (( tex_miss > tex_miss_ceiling )); then
        echo "smoke[$label]: WARN — tex.missing=$tex_miss > soft ceiling $tex_miss_ceiling"
        echo "                 (leaf-icon textures may not be in the textures-bsa)"
    fi

    eval "$kill_engine"
    return $hard_fail
}

fnv_run () {
    if [[ ! -f "$FNV_DATA/FalloutNV.esm" ]]; then
        echo "smoke[fnv]: SKIP — FalloutNV.esm not at $FNV_DATA"
        return 0
    fi
    # Mojave Wasteland exterior — grid 0,0 with radius 3 covers
    # ~7×7 cells centred on the world origin. Vanilla FNV ships 3
    # TREE base records (Joshua tree, creosote bush, dead tree —
    # see crates/plugin/tests/parse_real_esm.rs::parse_rate_fnv_esm)
    # that fan out into hundreds of REFRs across the wasteland.
    # Placeholder billboards are tiny (4 verts, 2 tris) so even
    # at 3 base records × N REFRs the draw cost stays in the noise.
    #
    # Thresholds:
    # - entities floor 200: any populated FNV exterior cell pulls in
    #   at least 200 statics + actors before tree REFRs are counted.
    #   Sub-200 means cell-load collapsed.
    # - Billboard floor 1: at least one TREE REFR must round-trip
    #   through the Phase 1.5 extension switch. Pre-Phase-1.5 this
    #   was zero on every FNV exterior cell.
    # - tex.missing ceiling 50: leaf icons ride in Textures.bsa.
    #   Anything above 50 indicates BSA coverage drift, not a SPT
    #   pipeline bug.
    run_cell fnv 200 1 50 \
        --esm "$FNV_DATA/FalloutNV.esm" \
        --grid 0,0 --radius 3 \
        --bsa "$FNV_DATA/Fallout - Meshes.bsa" \
        --textures-bsa "$FNV_DATA/Fallout - Textures.bsa"
}

fo3_run () {
    if [[ ! -f "$FO3_DATA/Fallout3.esm" ]]; then
        echo "smoke[fo3]: SKIP — Fallout3.esm not at $FO3_DATA"
        return 0
    fi
    # DC wasteland exterior — grid 0,0 covers central DC ruins.
    # FO3 ships 9 TREE base records (DC swamp foliage + dead trees).
    # Same threshold rationale as FNV; FO3 is denser so we can ask
    # for more billboards.
    run_cell fo3 200 5 50 \
        --esm "$FO3_DATA/Fallout3.esm" \
        --grid 0,0 --radius 3 \
        --bsa "$FO3_DATA/Fallout - Meshes.bsa" \
        --textures-bsa "$FO3_DATA/Fallout - Textures.bsa"
}

obl_run () {
    if [[ ! -f "$OBL_DATA/Oblivion.esm" ]]; then
        echo "smoke[obl]: SKIP — Oblivion.esm not at $OBL_DATA"
        return 0
    fi
    # Cyrodiil exterior — grid 0,0 with radius 3 sits in the western
    # Great Forest belt. Oblivion holds 113 of the era's 133 vanilla
    # `.spt` files (AUDIT_SPEEDTREE_2026-09-29), so this is the densest
    # SpeedTree corpus of the three titles — the arm the harness lacked
    # while it gated only FNV/FO3 (#5142).
    #
    # Thresholds (measured live 2026-09-30: entities=25063,
    # Billboards=1416, tex.missing=1 at grid 0,0 radius 3):
    # - entities floor 1000: any populated Cyrodiil exterior grid clears
    #   it on statics alone; sub-1000 means cell-load collapsed.
    # - Billboard floor 100: the Great Forest belt places TREE REFRs in
    #   every cell; pre-Phase-1.5 this was zero.
    # - tex.missing ceiling 50: leaf icons ride in the Compressed
    #   textures BSA; above 50 indicates BSA coverage drift.
    run_cell obl 1000 100 50 \
        --esm "$OBL_DATA/Oblivion.esm" \
        --grid 0,0 --radius 3 \
        --bsa "$OBL_DATA/Oblivion - Meshes.bsa" \
        --textures-bsa "$OBL_DATA/Oblivion - Textures - Compressed.bsa"
}

total_rc=0
case "$GAME" in
    fnv)    fnv_run    || total_rc=$? ;;
    fo3)    fo3_run    || total_rc=$? ;;
    obl)    obl_run    || total_rc=$? ;;
    all)
        fnv_run    || total_rc=$?
        fo3_run    || total_rc=$(( total_rc | $? ))
        obl_run    || total_rc=$(( total_rc | $? ))
        ;;
    *)      echo "Usage: $0 [fnv|fo3|obl|all]"; exit 2 ;;
esac

if (( total_rc != 0 )); then
    echo "smoke: FAIL — at least one cell hit a HARD assertion (rc=$total_rc)"
    exit "$total_rc"
fi
echo "smoke: PASS — every TREE REFR rendered through the SpeedTree pipeline."
