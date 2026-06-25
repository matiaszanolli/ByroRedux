#!/usr/bin/env bash
# M41 Phase 2 equip smoke test — verify NPCs spawn with their default
# outfit on Skyrim SE (BSTriShape + LVLI dispatch via OTFT) and FO4
# (BA2 + LVLI). Pairs with the `--bench-hold` CLI flag (commit 73adffb)
# and the `Inventory` / `EquipmentSlots` debug-server registration
# (commit-this-patch).
#
# Workflow per cell:
#   1. Spawn the engine in the background under `--bench-frames N
#      --bench-hold` so the bench summary lands and the debug server
#      stays reachable.
#   2. Wait for the `bench-hold:` notice in the engine's stderr.
#   3. Pipe a command sequence into `byro-dbg` (it reads stdin
#      line-by-line, exits on EOF):
#        - `entities`        — total entity count (sanity vs roadmap)
#        - `find Inventory`  — count of actors with non-empty inventory
#        - `find EquipmentSlots` — count of actors with biped occupancy
#        - `tex.missing`     — should be small (≤ a handful) on a
#                              fully-loaded cell; large counts signal
#                              missing texture archives or sibling-BSA
#                              auto-load drift
#   4. SIGTERM the engine and collect its bench summary.
#
# Pre-fix (M41 Phase 2 / #896 Phase B.2 close-out): Skyrim+ NPCs
# silently spawned with no equipment because outfit `INAM` arrays
# referenced LVLI form IDs that the equip walk skipped (the loop only
# matched direct ARMO entries). Post-#896 + LVLI dispatch (be4663b),
# the resolver walks `OTFT.items` through `expand_leveled_form_id` to
# flatten leveled lists into base ARMO refs before the spawn-mesh
# match. This script is the verification gate that keeps the
# regression visible.
#
# Usage:
#   docs/smoke-tests/m41-equip.sh [skyrim|fo4|all]
#
# Exit: 0 on success, non-zero on any cell whose entity count, equip
# count, or tex.missing count falls outside the expected band.

set -euo pipefail

GAME="${1:-all}"

SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
FO4_DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"

# Debug server port — matches the CLI default. Override if the engine
# is launched with a custom `BYRO_DEBUG_PORT`.
PORT="${BYRO_DEBUG_PORT:-9876}"

# Bench window. 30 frames is enough for cell-load to settle + a few
# steady-state frames; the post-bench hold lets `byro-dbg` connect.
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

# Returns the PID of the background engine on success; cleans up on
# bench failure.
#
# Args: $1 = label, $2 = entity floor (hard), $3 = draws floor (hard),
#       $4 = tex.missing ceiling (soft warn), then engine CLI args.
run_cell () {
    local label="$1" ; shift
    local entities_floor="$1" ; shift
    local draws_floor="$1" ; shift
    local tex_miss_ceiling="$1" ; shift
    local equip_floor="$1" ; shift
    local engine_log="$LOG_DIR/$label.engine.log"
    local dbg_log="$LOG_DIR/$label.dbg.log"

    echo "═══════════════════════════════════════════════════════════════"
    echo "  smoke[$label]: launching engine in background"
    echo "═══════════════════════════════════════════════════════════════"

    # Engine to background. Stderr captures the `bench-hold:` notice
    # we poll for; stdout captures the `bench:` summary line.
    cargo run --release --quiet -- \
        "$@" \
        --bench-frames "$BENCH_FRAMES" \
        --bench-hold \
        > "$engine_log.stdout" 2> "$engine_log.stderr" &
    local engine_pid=$!

    # Inline kill+wait at every exit point. A `trap … RETURN` hook is
    # appealing but bash tears local variables down before the trap
    # fires, so any reference to a `local` flag inside the trap body
    # explodes under `set -u`. Explicit cleanup at the four call sites
    # is shorter and visibly correct.
    local kill_engine='kill -TERM "$engine_pid" 2>/dev/null || true; wait "$engine_pid" 2>/dev/null || true'

    # Wait up to 180s for `bench-hold:` to appear in stderr (cold cargo
    # build + cell load can be slow).
    local timeout=180
    local deadline=$(( $(date +%s) + timeout ))
    while ! grep -q "^bench-hold:" "$engine_log.stderr" 2>/dev/null; do
        if [[ $(date +%s) -gt $deadline ]]; then
            echo "smoke[$label]: TIMEOUT waiting for bench-hold (engine logs in $engine_log.stderr)"
            eval "$kill_engine"
            return 1
        fi
        if ! kill -0 "$engine_pid" 2>/dev/null; then
            echo "smoke[$label]: engine exited before bench-hold (logs in $engine_log.stderr)"
            tail -20 "$engine_log.stderr"
            return 1
        fi
        sleep 0.5
    done

    echo "smoke[$label]: engine ready, attaching byro-dbg on port $PORT"

    # One-shot byro-dbg command sequence. EOF closes the REPL.
    # `entities <Component>` filters the entity list by the named
    # component (per `parse_shorthand` in tools/byro-dbg/src/main.rs).
    # `find …` does NOT exist as a byro-dbg command — pre-fix this
    # script used `find` and got `Error: no entity named 'find'`.
    BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<EOF > "$dbg_log" 2>&1 || true
entities Inventory
entities EquipmentSlots
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

    # ── HARD assertions on the engine bench line ──────────────────
    # Format is space-separated `key=value` tokens (see
    # `byroredux/src/main.rs:1252+`). `entities=` and `draws=` are
    # the load-bearing floors — anything below means the cell didn't
    # populate properly (parse error, BSA miss, scene-build crash).
    local entities draws
    entities=$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)
    draws=$(echo "$bench_line" | grep -oE 'draws=[0-9]+' | head -1 | cut -d= -f2)
    : "${entities:=0}"
    : "${draws:=0}"

    local hard_fail=0
    if (( entities < entities_floor )); then
        echo "smoke[$label]: HARD FAIL — entities=$entities < floor $entities_floor"
        hard_fail=1
    else
        echo "smoke[$label]: PASS — entities=$entities >= $entities_floor"
    fi
    if (( draws < draws_floor )); then
        echo "smoke[$label]: HARD FAIL — draws=$draws < floor $draws_floor"
        hard_fail=1
    else
        echo "smoke[$label]: PASS — draws=$draws >= $draws_floor"
    fi

    # ── SOFT assertions on the byro-dbg session ───────────────────
    # `entities <Component>` output ends with `(N entities)` per
    # `display.rs:21`. `tex.missing` is a JSON-pretty-printed string
    # whose first line carries `N unique missing textures:`.
    # Soft because environment-dependent — mod load order or DLC
    # archive coverage shifts the counts without indicating a bug.
    local inv_count slots_count tex_miss
    inv_count=$(awk '/^\(.*entities\)/ { gsub(/[()]/,""); print $1; exit }' \
        <(grep -A 99999 "byro> Error: no entity named" "$dbg_log" 2>/dev/null \
          || head -200 "$dbg_log") || true)
    # Simpler + more robust: count the `(N entities)` summary lines
    # by their position in the dbg log. There are 2 `entities <X>`
    # invocations (Inventory, EquipmentSlots), each ending with one
    # summary. tex.missing has its own JSON-string format.
    inv_count=$(grep -oE '^\([0-9]+ entities\)' "$dbg_log" | sed -n '1p' | grep -oE '[0-9]+' || echo 0)
    slots_count=$(grep -oE '^\([0-9]+ entities\)' "$dbg_log" | sed -n '2p' | grep -oE '[0-9]+' || echo 0)
    : "${inv_count:=0}"
    : "${slots_count:=0}"

    tex_miss=$(grep -oE '[0-9]+ unique missing textures' "$dbg_log" | grep -oE '^[0-9]+' || echo 0)
    : "${tex_miss:=0}"

    echo "smoke[$label]: Inventory=$inv_count entities, EquipmentSlots=$slots_count entities, tex.missing=$tex_miss unique"
    # Equip-signal floor (SK-D3-02). Pre-fix the equip counts were WARN-only,
    # so a regression dropping ALL NPC gear still passed as long as the
    # static-mesh floor held — the one test that exercises the full outfit
    # chain couldn't catch a silent equip regression. When the caller gives a
    # positive `equip_floor`, enforce it as a HARD floor on the real equip
    # signals. Skyrim WhiterunBanneredMare authors 6 named residents — saadia,
    # brenuin, mikael, sinmir, amaundmotierreend, hulda — each carrying both
    # Inventory and EquipmentSlots, so its floor is 6. `equip_floor=0` keeps
    # the legacy soft-warn for cells whose equipped-NPC count isn't yet a
    # stable guard (FO4 humanoid actors are gated on the .hkx loader, M41.x).
    if (( equip_floor > 0 )); then
        if (( inv_count < equip_floor )); then
            echo "smoke[$label]: HARD FAIL — Inventory=$inv_count entities < equip floor $equip_floor (NPC gear dropped?)"
            hard_fail=1
        else
            echo "smoke[$label]: PASS — Inventory=$inv_count >= $equip_floor"
        fi
        if (( slots_count < equip_floor )); then
            echo "smoke[$label]: HARD FAIL — EquipmentSlots=$slots_count entities < equip floor $equip_floor (LVLI dispatch empty / outfit regression?)"
            hard_fail=1
        else
            echo "smoke[$label]: PASS — EquipmentSlots=$slots_count >= $equip_floor"
        fi
    else
        if (( inv_count == 0 )); then
            echo "smoke[$label]: WARN — zero entities have Inventory (NPCs not spawning, or component not registered)"
        fi
        if (( slots_count == 0 )); then
            echo "smoke[$label]: WARN — zero entities have EquipmentSlots (LVLI dispatch may be silently empty)"
        fi
    fi
    if (( tex_miss > tex_miss_ceiling )); then
        echo "smoke[$label]: WARN — tex.missing=$tex_miss > soft ceiling $tex_miss_ceiling (archive coverage gap?)"
    fi

    eval "$kill_engine"
    return $hard_fail
    return 0
}

skyrim_run () {
    if [[ ! -f "$SKYRIM_DATA/Skyrim.esm" ]]; then
        echo "smoke[skyrim]: SKIP — Skyrim.esm not at $SKYRIM_DATA"
        return 0
    fi
    # Skyrim ships textures across Textures0-7.bsa; the
    # `open_with_numeric_siblings` auto-load rule only kicks in for
    # archives WITHOUT a digit before `.bsa`, and `Textures0.bsa`
    # already has one — so every archive must be passed explicitly.
    # Pre-fix the smoke script passed only Textures0-3 and the
    # tex.missing report exposed missing setdressing textures
    # whose canonical home is Textures4-7.
    # Thresholds: ROADMAP recorded 1932 entities at WhiterunBanneredMare
    # (M32.5 close); floor 1200 absorbs entity-count drift without
    # masking a parse-or-spawn collapse. Draws floor at 700 (similar
    # margin vs the ~700-1000 range observed).
    # Soft tex.missing ceiling at 30 — Whiterun ships textures across
    # Textures0-7.bsa and the script now passes all of them, so any
    # remaining miss after the archive expansion is environment drift.
    # equip_floor 6 — the named Bannered Mare residents (saadia, brenuin,
    # mikael, sinmir, amaundmotierreend, hulda) must each spawn with both
    # Inventory and EquipmentSlots, or the outfit chain regressed (SK-D3-02).
    run_cell skyrim 1200 700 30 6 \
        --esm "$SKYRIM_DATA/Skyrim.esm" \
        --cell WhiterunBanneredMare \
        --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
        --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures1.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures2.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures3.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures4.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures5.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures6.bsa" \
        --textures-bsa "$SKYRIM_DATA/Skyrim - Textures7.bsa"
}

fo4_run () {
    if [[ ! -f "$FO4_DATA/Fallout4.esm" ]]; then
        echo "smoke[fo4]: SKIP — Fallout4.esm not at $FO4_DATA"
        return 0
    fi
    # FO4 archive layout (vanilla install):
    #   Meshes.ba2 + MeshesExtra.ba2  — precombined / setdressing
    #                                    meshes need both
    #   Textures1-9.ba2 + TexturesPatch.ba2 — same sibling-auto-load
    #                                    gap as Skyrim (Textures1.ba2
    #                                    has a digit, rule doesn't fire)
    #   Materials.ba2                  — BGSM material chain; resolves
    #                                    via --materials-ba2 (separate
    #                                    flag; --textures-bsa won't
    #                                    surface BGSM lookups)
    # Pre-fix the smoke script passed only Textures1-2 and no Materials,
    # surfacing 213× officeboxpapers01_d.dds + 133× hightechdecaldebris01
    # + 46× metallocker01.bgsm in tex.missing. The .bgsm misses came
    # from the missing Materials.ba2; the texture misses came from the
    # archive coverage gap.
    # Thresholds: 2026-05-08 smoke run observed 10809 entities / 8162
    # draws on MedTekResearch01. Floor at 5000/4000 absorbs ~half the
    # observed volume — anything below that is a regression on the
    # cell-load critical path (parse error, BSA miss, or M40 streaming
    # state corruption). Soft tex.missing ceiling at 20 — with the full
    # Textures1-9 + TexturesPatch + Materials archive set passed below,
    # any remaining miss after vanilla expansion is environment drift.
    # Pre-archive-expansion the same cell reported 47 unique misses
    # (213× officeboxpapers01_d.dds dominating); post-expansion should
    # drop into the single digits.
    # equip_floor 0 — FO4 humanoid-actor equip is gated on the .hkx loader
    # (M41.x), so MedTekResearch01's equipped-NPC count isn't yet a stable
    # guard; keep the equip signals as soft WARN until it is (SK-D3-02).
    run_cell fo4 5000 4000 20 0 \
        --esm "$FO4_DATA/Fallout4.esm" \
        --cell MedTekResearch01 \
        --bsa "$FO4_DATA/Fallout4 - Meshes.ba2" \
        --bsa "$FO4_DATA/Fallout4 - MeshesExtra.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures1.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures2.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures3.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures4.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures5.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures6.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures7.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures8.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - Textures9.ba2" \
        --textures-bsa "$FO4_DATA/Fallout4 - TexturesPatch.ba2" \
        --materials-ba2 "$FO4_DATA/Fallout4 - Materials.ba2"
}

# Both cells run even on hard failure so a regression on one game
# doesn't mask drift on the other. Accumulate exit codes; final
# script exit reflects the OR. `|| rc=$?` opts out of `set -e` for
# the per-cell call.
total_rc=0
case "$GAME" in
    skyrim) skyrim_run || total_rc=$? ;;
    fo4)    fo4_run    || total_rc=$? ;;
    all)
        skyrim_run || total_rc=$?
        fo4_run    || total_rc=$(( total_rc | $? ))
        ;;
    *)      echo "Usage: $0 [skyrim|fo4|all]"; exit 2 ;;
esac

if (( total_rc != 0 )); then
    echo "smoke: FAIL — at least one cell hit a HARD assertion (rc=$total_rc)"
    exit "$total_rc"
fi
echo "smoke: PASS — all hard assertions met."
