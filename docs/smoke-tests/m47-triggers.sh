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
# Assertion severity (#3160):
#
#   HARD — the cell populated at all (entity floor + a bench summary).
#   HARD — `REFRs recognized > 0`, but ONLY on the pinned default cell with
#          `--scripts-bsa` resolved. That combination is deterministic: the
#          cell is fixed, its scripted activators are vanilla content, and
#          this file's own header asserts recognition "should be > 0" there.
#          Previously this was SOFT on the stated grounds that the counts
#          "depend on the cell's content and the mod load order" — true
#          under an override, false for the pinned default, and the
#          consequence was that deleting `attach_vmad_scripts` outright, or
#          breaking `pex_archive_path`'s `scripts\…\.pex` normalisation so
#          every lookup missed, left this harness green. It was the domain's
#          only engine-side gate on real game data and it could not report a
#          regression in decompile → recognize → attach.
#   SOFT — trigger-volume count. Towns really are sparse, and unlike
#          recognition the default cell's header makes no claim about it.
#   SOFT — everything, under `BYROREDUX_TRIGGER_CELL`, where content
#          genuinely varies and the determinism argument above does not hold.
#
# Coverage: runs the interior pass, then an exterior `--grid`/`--radius`
# pass so the exterior REFR-walk reaches the attach path at all. The
# exterior pass is SOFT throughout — its content varies with the streamed
# radius — and is skipped by `BYROREDUX_SMOKE_SKIP_EXTERIOR=1`.
#
# Usage:
#   docs/smoke-tests/m47-triggers.sh
#   docs/smoke-tests/m47-triggers.sh --self-test   # decision table only,
#                                                  # no engine, no game data
#
# Exit: 0 unless a HARD assertion failed.

set -euo pipefail

# ── the recognition gate, as a pure decision ─────────────────────────
#
# #3160 — extracted so the severity rule is exercisable without a Vulkan
# device, real game data, or an engine launch. The harness's own
# completeness check asks for proof that the promoted assertion can go RED;
# `--self-test` proves it for every input combination rather than for one
# observed run, which is the part that was missing when "deleting
# `attach_vmad_scripts` leaves it green" was true.
#
# Args:  <recognized> <cell_is_pinned_default:0|1> <scripts_bsa_resolved:0|1>
# Echoes: PASS | WARN | FAIL
recognition_verdict() {
    local recognized="$1" pinned="$2" scripts_bsa="$3"
    if (( recognized > 0 )); then
        echo PASS
        return
    fi
    # Zero recognized. HARD only where the expectation is deterministic:
    # the pinned default cell with the script archive actually resolved.
    if (( pinned == 1 && scripts_bsa == 1 )); then
        echo FAIL
    else
        echo WARN
    fi
}

if [[ "${1:-}" == "--self-test" ]]; then
    fails=0
    check() {
        local want="$1" got
        got=$(recognition_verdict "$2" "$3" "$4")
        if [[ "$got" != "$want" ]]; then
            echo "  self-test FAIL: recognition_verdict($2,$3,$4) = $got, want $want"
            fails=1
        else
            echo "  ok: recognition_verdict($2,$3,$4) = $got"
        fi
    }
    echo "smoke[m47-triggers]: --self-test — recognition gate decision table"
    # The regression this gate exists to catch: attach deleted / .pex lookup
    # broken, on the pinned default with the archive present.
    check FAIL 0 1 1
    # Recognition working — the normal green path.
    check PASS 7 1 1
    check PASS 1 1 1
    # Overridden cell: content genuinely varies, so zero stays advisory.
    check WARN 0 0 1
    # No script archive resolved: zero is expected, not a regression.
    check WARN 0 1 0
    check WARN 0 0 0
    # ...and a working count is still a pass under every combination.
    check PASS 3 0 0
    if (( fails != 0 )); then
        echo "smoke[m47-triggers]: --self-test FAILED"
        exit 1
    fi
    echo "smoke[m47-triggers]: --self-test PASS"
    exit 0
fi

SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
# #3160 — whether the cell is the pinned default decides the recognition
# gate's severity, so capture it before defaulting rather than comparing the
# resolved name (an override that happens to name the default cell is still
# an override as far as reproducibility is concerned).
if [[ -n "${BYROREDUX_TRIGGER_CELL:-}" ]]; then
    CELL_IS_PINNED_DEFAULT=0
else
    CELL_IS_PINNED_DEFAULT=1
fi
CELL="${BYROREDUX_TRIGGER_CELL:-WhiterunBanneredMare}"
# Exterior pass (#3160) — the interior `--cell` route never reaches the
# exterior REFR walk's fragment-population path.
EXTERIOR_GRID="${BYROREDUX_SMOKE_GRID:-0,0}"
EXTERIOR_RADIUS="${BYROREDUX_SMOKE_RADIUS:-2}"

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
# `--scripts-bsa` is passed unconditionally above and the file's existence
# was checked before launch, so it is resolved whenever we get this far.
verdict=$(recognition_verdict "$recognized" "$CELL_IS_PINNED_DEFAULT" 1)
case "$verdict" in
    PASS)
        echo "smoke[m47-triggers]: PASS — recognized=$recognized REFRs (decompile → recognize → attach is live)"
        ;;
    FAIL)
        echo "smoke[m47-triggers]: HARD FAIL — zero REFRs recognized on the pinned default cell"
        echo "                     '$CELL' with the script archive resolved. That cell's vanilla"
        echo "                     scripted activators are deterministic, so this is an engine"
        echo "                     regression in decompile → recognize → attach — most likely"
        echo "                     attach_vmad_scripts or pex_archive_path's .pex normalisation."
        hard_fail=1
        ;;
    *)
        echo "smoke[m47-triggers]: WARN — zero REFRs recognized. Either this cell has no scripted"
        echo "                     objects the catalog claims, or --scripts-bsa didn't resolve the .pex."
        echo "                     Advisory only: the cell is overridden, so content varies."
        ;;
esac
if (( triggers == 0 )); then
    echo "smoke[m47-triggers]: WARN — zero trigger volumes. Towns are sparse; try a quest dungeon:"
    echo "                     BYROREDUX_TRIGGER_CELL=BleakFallsBarrow01 $0"
fi

eval "$kill_engine"

if (( hard_fail != 0 )); then
    echo "smoke[m47-triggers]: FAIL — a HARD assertion failed (rc=$hard_fail)"
    exit "$hard_fail"
fi
echo "smoke[m47-triggers]: PASS — interior: cell loaded and scripts attached."

# ── exterior pass (#3160) ────────────────────────────────────────────
#
# `--cell` never reaches the exterior REFR walk, so before this the whole
# harness (and its `m43-quest-runtime.sh` sibling) covered only the interior
# route. SOFT throughout: what a streamed radius contains varies with the
# grid and the radius, so the determinism argument that justifies the
# interior HARD gate does not carry over.
if [[ "${BYROREDUX_SMOKE_SKIP_EXTERIOR:-0}" == "1" ]]; then
    echo "smoke[m47-triggers]: exterior pass skipped (BYROREDUX_SMOKE_SKIP_EXTERIOR=1)"
    exit 0
fi

echo
echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[m47-triggers]: exterior grid $EXTERIOR_GRID r=$EXTERIOR_RADIUS"
echo "═══════════════════════════════════════════════════════════════"

ext_log="$LOG_DIR/exterior.log"
cargo run --release --quiet -- \
    --esm "$SKYRIM_DATA/Skyrim.esm" \
    --grid "$EXTERIOR_GRID" \
    --radius "$EXTERIOR_RADIUS" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
    --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    --scripts-bsa "$SCRIPTS_BSA" \
    --bench-frames "$BENCH_FRAMES" \
    > "$ext_log.stdout" 2> "$ext_log.stderr" || true

ext_m47=$(grep -oE 'M47\.2 scripts: [0-9]+ REFRs recognized, [0-9]+ trigger volumes spawned' \
    "$ext_log.stderr" | tail -1 || true)
if [[ -n "$ext_m47" ]]; then
    echo "  $ext_m47"
    ext_recognized=$(echo "$ext_m47" | grep -oE '[0-9]+ REFRs' | grep -oE '[0-9]+')
else
    ext_recognized=0
    echo "  (no 'M47.2 scripts:' line — zero recognized scripts and zero trigger volumes)"
fi
: "${ext_recognized:=0}"
if (( ext_recognized == 0 )); then
    echo "smoke[m47-triggers]: WARN — exterior grid $EXTERIOR_GRID r=$EXTERIOR_RADIUS recognized"
    echo "                     zero REFRs. Advisory: streamed content varies with grid/radius."
else
    echo "smoke[m47-triggers]: exterior recognized=$ext_recognized REFRs"
fi

echo "smoke[m47-triggers]: PASS — interior HARD gates met; exterior pass advisory."
