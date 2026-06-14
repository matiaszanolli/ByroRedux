#!/usr/bin/env bash
# M41.x ragdoll smoke test — verify a Bethesda/Havok ragdoll parses,
# threads, builds on our Rapier solver, and activates end-to-end on a
# real FNV humanoid (Doc Mitchell in GSDocMitchellHouse).
#
# What the engine does at cell load (no command needed):
#   - parses the FNV `_male/skeleton.nif` Havok ragdoll (18 bodies + 17
#     joints: bhkRagdoll + bhkMalleableConstraint-wrapped),
#   - threads it into ImportedScene.ragdoll,
#   - resolves bone names against the spawned skeleton and attaches a
#     `RagdollTemplate` to the actor root — logging
#     `Attached RagdollTemplate (N bodies) to root entity <ID>`.
#
# This script then drives the activation:
#   1. Spawn the engine under `--bench-frames N --bench-hold`.
#   2. Wait for `bench-hold:`.
#   3. Scrape the attach log for the root entity id + body count.
#   4. Pipe `ragdoll <id>` into byro-dbg; the command builds the Rapier
#      multibody and returns `now simulating <N> bodies`.
#   5. Hard-assert: a template attached, the command reported >= MIN_BODIES,
#      and the engine stayed up (bench summary present → no solver blow-up).
#
# The VISUAL payoff (Doc Mitchell crumpling) is watched live; this script
# is the automatable gate that keeps the parse→build→activate chain honest.
#
# Usage:
#   docs/smoke-tests/m41-ragdoll.sh
#
# Exit: 0 on success, non-zero on any failed hard assertion.

set -euo pipefail

FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
CELL="${BYROREDUX_RAGDOLL_CELL:-GSDocMitchellHouse}"
# A vanilla FNV humanoid ragdoll has 18 bodies; floor well below that to
# absorb a body whose bone name fails to resolve without masking a
# collapse (a 2-body result would mean the graph barely threaded).
MIN_BODIES="${BYROREDUX_RAGDOLL_MIN_BODIES:-10}"

if [[ ! -f "$FNV_DATA/FalloutNV.esm" ]]; then
    echo "smoke[ragdoll]: SKIP — FalloutNV.esm not at $FNV_DATA"
    exit 0
fi

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT
ENGINE_LOG="$LOG_DIR/engine"
DBG_LOG="$LOG_DIR/dbg.log"

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[ragdoll]: launching FNV $CELL in background"
echo "═══════════════════════════════════════════════════════════════"

# RUST_LOG=info so the loader's `Attached RagdollTemplate …` line lands.
RUST_LOG="${RUST_LOG:-byroredux=info}" cargo run --release --quiet -- \
    --esm "$FNV_DATA/FalloutNV.esm" \
    --cell "$CELL" \
    --bsa "$FNV_DATA/Fallout - Meshes.bsa" \
    --textures-bsa "$FNV_DATA/Fallout - Textures.bsa" \
    --textures-bsa "$FNV_DATA/Fallout - Textures2.bsa" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    > "$ENGINE_LOG.stdout" 2> "$ENGINE_LOG.stderr" &
engine_pid=$!
kill_engine='kill -TERM "$engine_pid" 2>/dev/null || true; wait "$engine_pid" 2>/dev/null || true'

# Wait up to 180s for bench-hold (cold build + cell load).
deadline=$(( $(date +%s) + 180 ))
while ! grep -q "^bench-hold:" "$ENGINE_LOG.stderr" 2>/dev/null; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[ragdoll]: TIMEOUT waiting for bench-hold"
        tail -20 "$ENGINE_LOG.stderr" || true
        eval "$kill_engine"; exit 1
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[ragdoll]: engine exited before bench-hold"
        tail -20 "$ENGINE_LOG.stderr" || true
        exit 1
    fi
    sleep 0.5
done

# Scrape the attach log: `Attached RagdollTemplate (N bodies) to root entity <ID>`.
attach_line=$(grep -E "Attached RagdollTemplate \([0-9]+ bodies\) to root entity [0-9]+" \
    "$ENGINE_LOG.stderr" | head -1 || true)
if [[ -z "$attach_line" ]]; then
    echo "smoke[ragdoll]: HARD FAIL — no RagdollTemplate attached (no skeleton ragdoll threaded?)"
    echo "  (recent engine log:)"; tail -20 "$ENGINE_LOG.stderr" || true
    eval "$kill_engine"; exit 1
fi
attached_bodies=$(echo "$attach_line" | grep -oE '\([0-9]+ bodies\)' | grep -oE '[0-9]+')
root_id=$(echo "$attach_line" | grep -oE 'root entity [0-9]+' | grep -oE '[0-9]+')
echo "smoke[ragdoll]: $attach_line"
echo "smoke[ragdoll]: template root entity = $root_id ($attached_bodies bodies)"

echo "smoke[ragdoll]: attaching byro-dbg on port $PORT → ragdoll $root_id"
BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<EOF > "$DBG_LOG" 2>&1 || true
ragdoll $root_id
quit
EOF

echo
echo "── byro-dbg session log ────────────────────────────────────────"
cat "$DBG_LOG"
echo "────────────────────────────────────────────────────────────────"

# `ragdoll` reports `now simulating <N> bodies on Rapier` on success.
sim_bodies=$(grep -oE 'now simulating [0-9]+ bodies' "$DBG_LOG" | grep -oE '[0-9]+' | head -1 || echo 0)
: "${sim_bodies:=0}"

# Engine must still be alive after the command (no solver explosion crash).
engine_alive=0
if kill -0 "$engine_pid" 2>/dev/null; then engine_alive=1; fi

bench_line=$(grep "^bench:" "$ENGINE_LOG.stdout" || true)
eval "$kill_engine"

hard_fail=0
if (( attached_bodies < MIN_BODIES )); then
    echo "smoke[ragdoll]: HARD FAIL — template has $attached_bodies bodies < floor $MIN_BODIES"
    hard_fail=1
else
    echo "smoke[ragdoll]: PASS — template threaded $attached_bodies bodies (>= $MIN_BODIES)"
fi
if (( sim_bodies < MIN_BODIES )); then
    echo "smoke[ragdoll]: HARD FAIL — ragdoll built only $sim_bodies bodies < floor $MIN_BODIES"
    hard_fail=1
else
    echo "smoke[ragdoll]: PASS — Rapier multibody built $sim_bodies bodies"
fi
if (( engine_alive == 1 )); then
    echo "smoke[ragdoll]: PASS — engine survived activation (no solver blow-up)"
else
    echo "smoke[ragdoll]: HARD FAIL — engine died during/after activation"
    hard_fail=1
fi
[[ -n "$bench_line" ]] && echo "  bench: $bench_line"

if (( hard_fail != 0 )); then
    echo "smoke[ragdoll]: FAIL"
    exit 1
fi
echo "smoke[ragdoll]: PASS — parse→thread→build→activate chain green. Watch Doc Mitchell crumple live."
