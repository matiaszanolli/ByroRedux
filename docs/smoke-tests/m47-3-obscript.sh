#!/usr/bin/env bash
# M47.3 gate — legacy ObScript object / item scripts, live on FNV.
#
#   1. Boot the FNV profile into GSProspectorSaloonInterior with a player.
#      The interior job spawns its references BEFORE `populate_scene_runtime`,
#      so the object-script table has to exist by then (`ensure_obscript_tables`):
#      without it `obscript.objects` lists nothing in this cell (found live).
#   2. OnLoad — `GoodspringsRadioScript`: `if (VFreeformGoodsprings.bKnowAboutRadio == 0)
#      && (GetDestroyed == 0)` reads a remote quest variable and a getter, then
#      `SetDestroyed 1`. Gate: the effect log records it.
#   3. OnTriggerEnter — `SunnySmilesTriggerSCRIPT`: teleport the player into its
#      volume; the block runs once, `PlaySound` reaches the sound ingress and
#      `CheyenneRef.PlayIdle LooseDogGrowl` (an editor-ID string argument) resolves
#      the dog's `.kf` and plays it.
#   4. OnDeath — `GSSunnySmilesScript`: drive Sunny's Health below zero through the
#      shared SDK death commit; the block's `SetEnemy` (the effect bridge) and
#      `SetStage VMS16 110` run, nothing is declined.
#   5. Item OnAdd — `WeapTrapBaseballSCRIPT`: adding TrapBaseball to the player runs
#      `player.additem Baseball01 1 1; removeme` from the item's script.
#   4b. Resurrect + GetDeadCount, 4c. IsInInterior / GetInWorldspace / notes through `obscript.cmd`,
#   5b. a chem's script effect (NukaCola's ScriptEffectStart) through `inv.use`.
#   6. PlayGroup — a second boot into Lucky38BasementFloorB2: `OnLoadPlayForwardSCRIPT`
#      (`Begin OnLoad / Playgroup Forward 0`) on the force-field glow statics starts the
#      mesh's own `Forward` NiControllerSequence; nothing is declined.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19879}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-m47-3.XXXXXX)"
engine_pid=""

stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
}
trap stop_engine EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    echo "smoke[m47-3-obscript]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m47-3-obscript]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg \
        >"$2" 2>&1 <<CMDS
$1
.quit
CMDS
    sed -i 's/\\n/\n/g' "$2"
}

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux_scripting=info" \
    target/release/byroredux \
    --game fnv --cell GSProspectorSaloonInterior \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 3

objects() { debug_command "obscript.objects 40" "$LOG_DIR/objects.log" || fail "obscript.objects unavailable"; }
effects() { debug_command "quest.effects $1" "$LOG_DIR/effects.log" || fail "quest.effects unavailable"; }
row_for() { grep -E "ref $1 " "$LOG_DIR/objects.log" | head -1; }
entity_of() { row_for "$1" | sed -E 's/.*entity ([0-9]+) .*/\1/'; }

# ── 1. the table was there when the references spawned ────────────
objects
placed="$(grep -oE '[0-9]+ placed reference' "$LOG_DIR/objects.log" | head -1 | cut -d' ' -f1 || true)"
[[ -n "$placed" && "$placed" -ge 8 ]] \
    || fail "only ${placed:-0} placed references run an object script — the table was not installed before the references spawned: $(sed -n '1,4p' "$LOG_DIR/objects.log")"
echo "smoke[m47-3-obscript]: $placed placed references run object scripts"

# ── 2. OnLoad: remote variable + getter + SetDestroyed ────────────
radio="$(row_for 00109087)"
[[ "$radio" == *GoodspringsRadioScript* ]] || fail "the Goodsprings radio runs no object script"
effects 0x00109087
grep -Fq "SetDestroyed 00109087 true" "$LOG_DIR/effects.log" \
    || fail "OnLoad did not SetDestroyed the radio: $(cat "$LOG_DIR/effects.log")"
echo "smoke[m47-3-obscript]: OnLoad PASS — remote quest variable + GetDestroyed + SetDestroyed"

# ── 3. OnTriggerEnter: the player walks into Sunny's volume ───────
sunny_trigger="$(entity_of 0016AD06)"
[[ -n "$sunny_trigger" ]] || fail "SunnySmilesTriggerSCRIPT reference not found"
debug_command "cam.pos 274 3620 605" "$LOG_DIR/teleport.log" || fail "cam.pos unavailable"
sleep 3
objects
row_for 0016AD06 | grep -Fq "1 run(s)" \
    || fail "OnTriggerEnter did not run once: $(row_for 0016AD06)"
effects 0x0016AD06
grep -Fq "PlaySound" "$LOG_DIR/effects.log" || fail "OnTriggerEnter queued no PlaySound"
row_for 0016AD06 | grep -Fq "0 declined" || fail "OnTriggerEnter declined a command: $(row_for 0016AD06)"
grep -Fq "ObScript command 0x1190 on 0010588E" "$LOG_DIR/effects.log" \
    || fail "CheyenneRef.PlayIdle LooseDogGrowl was not run: $(cat "$LOG_DIR/effects.log")"
wait_log "$LOG_DIR/session.stderr" "SpecialIdle_Aroo"
echo "smoke[m47-3-obscript]: OnTriggerEnter PASS — one run, PlaySound queued, PlayIdle resolved its .kf"

# ── 4. OnDeath through the effect bridge ──────────────────────────
sunny="$(entity_of 00104E85)"
[[ -n "$sunny" ]] || fail "Sunny's reference not found"
debug_command "setav $sunny 0x450 -1" "$LOG_DIR/kill.log" || fail "setav unavailable"
sleep 2
objects
row_for 00104E85 | grep -Fq "1 run(s), 0 declined" \
    || fail "OnDeath did not run cleanly: $(row_for 00104E85)"
effects 0x00104E85
grep -Fq "SetStage 00104EAE 110" "$LOG_DIR/effects.log" || fail "OnDeath did not fail VMS16"
grep -Fq "ObScript command 0x1078" "$LOG_DIR/effects.log" || fail "OnDeath did not SetEnemy"
echo "smoke[m47-3-obscript]: OnDeath PASS — SetEnemy (bridge) + SetStage, nothing declined"

# ── 4b. GetDeadCount counts the death; Resurrect brings Sunny back in place and keeps the count ──
cmd_answer() { debug_command "obscript.cmd $1" "$LOG_DIR/cmd.log" || fail "obscript.cmd unavailable"; grep -oE -- '-> [^"]*' "$LOG_DIR/cmd.log" | head -1; }
[[ "$(cmd_answer 'none 1054 r:104e84')" == "-> 1" ]] || fail "GetDeadCount GSSunnySmiles is not 1 after her death: $(cat "$LOG_DIR/cmd.log")"
cmd_answer "$sunny 108c 1" >/dev/null
sleep 2
debug_command "cond $sunny GetDead" "$LOG_DIR/dead.log" || fail "cond unavailable"
grep -Fq "= 0" "$LOG_DIR/dead.log" || fail "Resurrect did not bring Sunny back: $(cat "$LOG_DIR/dead.log")"
[[ "$(cmd_answer 'none 1054 r:104e84')" == "-> 1" ]] || fail "Resurrect lowered the death count"
debug_command "ragdoll.status $sunny" "$LOG_DIR/ragdoll.log" || fail "ragdoll.status unavailable"
grep -Fq "no active ragdoll" "$LOG_DIR/ragdoll.log" || fail "Sunny's ragdoll was not freed: $(cat "$LOG_DIR/ragdoll.log")"
echo "smoke[m47-3-obscript]: Resurrect PASS — alive in place, ragdoll freed, death count kept"

# ── 4c. Getters through the real host: interior, worldspace, notes ──
[[ "$(cmd_answer "$sunny 112c")" == "-> 1" ]] || fail "IsInInterior is not 1 in the saloon"
[[ "$(cmd_answer "$sunny 1136 r:3d43b")" == "-> 0" ]] || fail "GetInWorldspace is not 0 in an interior"
cmd_answer "none 117c r:101" >/dev/null
[[ "$(cmd_answer 'none 117e r:101')" == "-> 1" ]] || fail "AddNote / GetHasNote did not round-trip"
echo "smoke[m47-3-obscript]: getters PASS — IsInInterior, GetInWorldspace, AddNote/GetHasNote"

# ── 5. Item OnAdd + RemoveMe ──────────────────────────────────────
debug_command "inv.add 0x00033D84 1" "$LOG_DIR/inv.log" || fail "inv.add unavailable"
sleep 2
effects 0x00033D84
grep -Fq "ObScript command 0x1002 on 00000014" "$LOG_DIR/effects.log" \
    || fail "the item's OnAdd did not run player.additem: $(cat "$LOG_DIR/effects.log")"
echo "smoke[m47-3-obscript]: item OnAdd PASS — player.additem + removeme"

# ── 5b. A script effect: the NukaCola chem's ScriptEffectStart (`additem caps001 1`) runs on its user ──
debug_command "inv.add 0x0001519E 1" "$LOG_DIR/inv2.log" || fail "inv.add unavailable"
debug_command "inv.use 0x0001519E" "$LOG_DIR/use.log" || fail "inv.use unavailable"
sleep 2
effects 0x00000014
grep -Fq "ObScript command 0x1002 on 00000014" "$LOG_DIR/effects.log" \
    || fail "the chem's script effect did not run its additem: $(cat "$LOG_DIR/effects.log")"
echo "smoke[m47-3-obscript]: script effect PASS — NukaCola's ScriptEffectStart ran on the player"

# ── 6. PlayGroup on placed objects (second boot, another cell) ────
stop_engine
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux_scripting=info" \
    target/release/byroredux \
    --game fnv --cell Lucky38BasementFloorB2 \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/pg.stdout" 2>"$LOG_DIR/pg.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/pg.stderr" 'bench-hold:'
sleep 3
objects
row_for 00175D91 | grep -Fq "OnLoadPlayForwardSCRIPT" || fail "the force-field glow runs no object script: $(sed -n '1,4p' "$LOG_DIR/objects.log")"
row_for 00175D91 | grep -Fq "1 run(s), 0 declined" \
    || fail "PlayGroup was declined on the force-field glow: $(row_for 00175D91)"
effects 0x00175D91
grep -Fq "PlayGroup 00175D91 Forward" "$LOG_DIR/effects.log" \
    || fail "PlayGroup left no effect row: $(cat "$LOG_DIR/effects.log")"
echo "smoke[m47-3-obscript]: PlayGroup PASS — OnLoad started the Forward sequence, nothing declined"

echo "smoke[m47-3-obscript]: PASS"
