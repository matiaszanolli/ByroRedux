#!/usr/bin/env bash
# M47.2 compiled-script + trigger-volume smoke test — verify the engine
# decompiles vanilla `.pex` scripts at cell load (recognizer chain) and
# spawns invisible trigger volumes from `XPRM` primitives. Pairs with the
# `--scripts-bsa` flag and the `M47.2 scripts:` cell-load summary line
# (this milestone).
#
# What it checks, end to end:
#   1. A Skyrim cell loads WITH `--scripts-bsa "Skyrim - Misc.bsa"`, so the
#      REFR-attach path can resolve each scripted REFR's VMAD-named `.pex`,
#      decompile it, and run it through the recognizer chain.
#   2. The cell-load summary line
#        `M47.2 scripts: N REFRs recognized, M trigger volumes spawned`
#      reports how many REFRs got canonical ECS behavior and how many
#      invisible trigger boxes spawned from `XPRM` box/sphere primitives.
#
# What it does NOT check: the runtime crossing (player walks into a volume
# → OnTriggerEnterEvent → quest advance). That needs the player teleported
# into a volume, which byro-dbg can't drive; the detection + dispatch are
# covered by unit tests (`trigger.rs`, `quest_advance/tests.rs`). This
# smoke is the engine-side spawn + attach gate on real game data.
#
# Cell choice: the default (WhiterunBanneredMare) loads reliably and has
# scripted activators, so `REFRs recognized` should be > 0. Trigger
# VOLUMES are sparse in towns — for a trigger-heavy gate, point the script
# at a quest dungeon (ambush / trap / boundary triggers):
#   BYROREDUX_TRIGGER_CELL=BleakFallsBarrow01 docs/smoke-tests/m47-triggers.sh
#
# Both M47.2 counts are SOFT (WARN, no exit-code change) — their values
# depend on the cell's content and the mod load order, not on engine
# correctness. The HARD gate is that the cell loaded at all (entity floor +
# a bench summary), matching the README severity model.
#
# Usage:
#   docs/smoke-tests/m47-triggers.sh
#
# Exit: 0 unless the cell failed to load (entity floor / missing bench).

set -euo pipefail

SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
CELL="${BYROREDUX_TRIGGER_CELL:-WhiterunBanneredMare}"

# Entity floor — the HARD gate that the cell populated at all. Set well
# below observed counts (WhiterunBanneredMare ~1900) so content drift
# doesn't trip a false fail; a quest dungeon override may sit lower, so
# the floor is deliberately conservative.
ENTITY_FLOOR="${BYROREDUX_ENTITY_FLOOR:-300}"

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

if [[ ! -f "$SKYRIM_DATA/Skyrim.esm" ]]; then
    echo "smoke[m47-triggers]: SKIP — Skyrim.esm not at $SKYRIM_DATA"
    exit 0
fi
SCRIPTS_BSA="$SKYRIM_DATA/Skyrim - Misc.bsa"
if [[ ! -f "$SCRIPTS_BSA" ]]; then
    echo "smoke[m47-triggers]: SKIP — 'Skyrim - Misc.bsa' (script archive) not at $SKYRIM_DATA"
    exit 0
fi

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[m47-triggers]: cell '$CELL', launching engine in background"
echo "═══════════════════════════════════════════════════════════════"

engine_log="$LOG_DIR/engine.log"
dbg_log="$LOG_DIR/dbg.log"

# Engine to background. log::info (incl. the `M47.2 scripts:` summary)
# lands on stderr; the `bench:` summary lands on stdout.
cargo run --release --quiet -- \
    --esm "$SKYRIM_DATA/Skyrim.esm" \
    --cell "$CELL" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
    --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    --scripts-bsa "$SCRIPTS_BSA" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    > "$engine_log.stdout" 2> "$engine_log.stderr" &
engine_pid=$!

kill_engine='kill -TERM "$engine_pid" 2>/dev/null || true; wait "$engine_pid" 2>/dev/null || true'

# Wait up to 180s for `bench-hold:` on stderr (cold build + cell load).
deadline=$(( $(date +%s) + 180 ))
while ! grep -q "^bench-hold:" "$engine_log.stderr" 2>/dev/null; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[m47-triggers]: TIMEOUT waiting for bench-hold (logs in $engine_log.stderr)"
        eval "$kill_engine"
        exit 1
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[m47-triggers]: engine exited before bench-hold (logs in $engine_log.stderr)"
        tail -20 "$engine_log.stderr"
        exit 1
    fi
    sleep 0.5
done

echo "smoke[m47-triggers]: engine ready, attaching byro-dbg on port $PORT"

# Total entity count for the sanity floor. TriggerVolume / QuestAdvance
# aren't in the debug-server registry, so the M47.2 counts come from the
# engine summary log, not from `entities <Component>`.
BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<EOF > "$dbg_log" 2>&1 || true
entities
quit
EOF

echo
echo "── engine M47.2 summary ────────────────────────────────────────"
# The cell-load summary line (references.rs): only emitted when at least
# one script was recognized or one trigger volume spawned.
m47_line=$(grep -oE 'M47\.2 scripts: [0-9]+ REFRs recognized, [0-9]+ trigger volumes spawned' \
    "$engine_log.stderr" | tail -1 || true)
if [[ -n "$m47_line" ]]; then
    echo "  $m47_line"
else
    echo "  (no 'M47.2 scripts:' line — zero recognized scripts and zero trigger volumes in this cell)"
fi

echo "── engine bench summary ────────────────────────────────────────"
bench_line=$(grep "^bench:" "$engine_log.stdout" || true)
if [[ -z "$bench_line" ]]; then
    echo "  (no bench: line found)"
    eval "$kill_engine"
    echo "smoke[m47-triggers]: FAIL — no bench summary (cell did not load)"
    exit 1
fi
echo "$bench_line"
echo

# ── HARD assertion: the cell populated at all ────────────────────────
entities=$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)
: "${entities:=0}"
hard_fail=0
if (( entities < ENTITY_FLOOR )); then
    echo "smoke[m47-triggers]: HARD FAIL — entities=$entities < floor $ENTITY_FLOOR (cell '$CELL' didn't load)"
    hard_fail=1
else
    echo "smoke[m47-triggers]: PASS — entities=$entities >= $ENTITY_FLOOR"
fi

# ── SOFT assertions: the M47.2 recognition + trigger counts ──────────
recognized=0
triggers=0
if [[ -n "$m47_line" ]]; then
    recognized=$(echo "$m47_line" | grep -oE '[0-9]+ REFRs' | grep -oE '[0-9]+')
    triggers=$(echo "$m47_line" | grep -oE '[0-9]+ trigger' | grep -oE '[0-9]+')
fi
: "${recognized:=0}"
: "${triggers:=0}"

echo "smoke[m47-triggers]: recognized=$recognized REFRs, trigger_volumes=$triggers (cell '$CELL')"
if (( recognized == 0 )); then
    echo "smoke[m47-triggers]: WARN — zero REFRs recognized. Either this cell has no scripted"
    echo "                     objects the catalog claims, or --scripts-bsa didn't resolve the .pex."
fi
if (( triggers == 0 )); then
    echo "smoke[m47-triggers]: WARN — zero trigger volumes. Towns are sparse; try a quest dungeon:"
    echo "                     BYROREDUX_TRIGGER_CELL=BleakFallsBarrow01 $0"
fi

eval "$kill_engine"

if (( hard_fail != 0 )); then
    echo "smoke[m47-triggers]: FAIL — cell did not load (rc=$hard_fail)"
    exit "$hard_fail"
fi
echo "smoke[m47-triggers]: PASS — cell loaded; see M47.2 counts above for recognition/trigger coverage."
