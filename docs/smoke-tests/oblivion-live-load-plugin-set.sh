#!/usr/bin/env bash
# Owner-run live gate for #7039 (SAVE-1 / SAVE-2 of the 2026-11-17 /audit-save): a save made under one
# Oblivion plugin set, loaded live into an engine running another, in both directions. Requires a Vulkan
# device, display and BYROREDUX_OBLIVION_DATA (Oblivion.esm, Knights.esp, DLCThievesDen.esp and the two
# archives); exits 77 without them. Artifacts remain in the printed folder.
#
# The engine's actor-value records take the lowest slot no record occupies, so the plugin set moves them:
# Oblivion.esm alone -> Regular(2); with Knights.esp and DLCThievesDen.esp -> Regular(3). A live load
# re-installs the load order stored in the save, so loading across the two moves the slot under a running
# engine. What it checks, from the console:
#   * the engine boots at the expected slot, and the live load installs the other one and rebuilds the
#     process-lifetime rulesets (SAVE-2);
#   * the player's values (Endurance, Health, Magicka, Fatigue) and the HUD bar come back under the new keys;
#   * the derived pools still follow their inputs afterwards (ModAV Endurance moves Fatigue and Health), which
#     fails if the rulesets or the player's keys are stale;
#   * the next save is accepted, and a save made after the load loads back across the plugin sets again, with
#     its values (a stale key is refused by the save's identity preflight).
#   * after each live load the player drinks PotionRestoreHealth (0x98496, Restore Health 35, instant) and
#     Health rises by 35: the consume path reads ActorVitals.health, so a stale key (SAVE-1) refuses the potion
#     where the console's GetAV / DamageAV, which resolve by name, would not notice (#7045).
set -euo pipefail

DATA="${BYROREDUX_OBLIVION_DATA:-}"
need=(Oblivion.esm Knights.esp DLCThievesDen.esp "Oblivion - Meshes.bsa" "Oblivion - Textures - Compressed.bsa")
if [[ -z "$DATA" ]]; then
    echo 'smoke[oblivion-live-load-plugin-set]: SKIP -- set BYROREDUX_OBLIVION_DATA to the Oblivion Data directory'
    exit 77
fi
for file in "${need[@]}"; do
    if [[ ! -f "$DATA/$file" ]]; then
        echo "smoke[oblivion-live-load-plugin-set]: SKIP -- $DATA/$file missing"
        exit 77
    fi
done
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release"
PORT="${BYRO_DEBUG_PORT:-19884}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-oblivion-live-load.XXXXXX)"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[oblivion-live-load-plugin-set]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied"
fi
if pgrep -x byroredux >/dev/null; then
    fail 'another engine instance is already running'
fi
cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail 'build failed'

# run <command> -> the debug server's answer lines on stdout.
run() {
    printf '%s\n.quit\n' "$1" | env BYRO_DEBUG_PORT="$PORT" timeout "$TIMEOUT" "$BIN/byro-dbg" 2>&1 \
        | sed 's/\\n/\n/g' | sed -n 's/^byro> //p'
}
# av <name> -> the player's current value.
av() { run "obscript.cmd 0x14 GetAV $1" | sed -nE 's/.*0x100e -> (-?[0-9.]+).*/\1/p' | tail -1; }
# expect <label> <actual> <expected>
expect() {
    awk -v a="$2" -v e="$3" 'BEGIN { d = a - e; exit !(a != "" && d < 0.01 && d > -0.01) }' \
        || fail "$1: got '${2:-nothing}', expected $3"
}
slot_of() { grep -h 'Installed 72 engine-supplied' "$1" | sed -nE 's/.*Regular\(([0-9]+)\).*/\1/p' | tail -1; }
# launch <phase> <A|B>: A = Oblivion.esm alone, B = Oblivion.esm + Knights.esp + DLCThievesDen.esp.
launch() {
    local phase="$1" set="$2" plugins
    if [[ "$set" == A ]]; then
        plugins=(--esm "$DATA/Oblivion.esm")
    else
        plugins=(--master "$DATA/Oblivion.esm" --master "$DATA/Knights.esp" --esm "$DATA/DLCThievesDen.esp")
    fi
    : >"$LOG_DIR/$phase.log"
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
        BYROREDUX_SAVE_DIR="$LOG_DIR/saves" RUST_LOG=warn,byroredux::save_io=info,byroredux::cell_loader=info \
        "$BIN/byroredux" "${plugins[@]}" --cell AnvilTheCountsArms \
        --bsa "$DATA/Oblivion - Meshes.bsa" --textures-bsa "$DATA/Oblivion - Textures - Compressed.bsa" \
        --player --hud --bench-frames 30 --bench-hold >"$LOG_DIR/$phase.log" 2>&1 &
    engine_pid=$!
    local deadline=$((SECONDS + TIMEOUT))
    until grep -Fq 'bench-hold:' "$LOG_DIR/$phase.log"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "$phase: engine exited before bench-hold"
        (( SECONDS < deadline )) || fail "$phase: timed out waiting for bench-hold"
        sleep 0.25
    done
}
# live_load <phase> <slot>: queue the load and wait for it to finish.
live_load() {
    local phase="$1" slot="$2" before
    before="$(grep -c 'save load: restored player pose' "$LOG_DIR/$phase.log" || true)"
    run "load $slot" | grep -Fq "queued load of slot $slot" || fail "$phase: load $slot was not queued"
    local deadline=$((SECONDS + TIMEOUT))
    until (( $(grep -c 'save load: restored player pose' "$LOG_DIR/$phase.log" || true) > before )); do
        grep -q 'save load: .*abort\|save load: .*ABORT\|load ABORTED' "$LOG_DIR/$phase.log" && fail "$phase: load $slot aborted"
        (( SECONDS < deadline )) || fail "$phase: timed out loading slot $slot"
        sleep 0.5
    done
    sleep 2
}
# state <phase> <label> <endurance> <health> <magicka> <fatigue>
state() {
    expect "$2 Endurance" "$(av Endurance)" "$3"
    expect "$2 Health" "$(av Health)" "$4"
    expect "$2 Magicka" "$(av Magicka)" "$5"
    expect "$2 Fatigue" "$(av Fatigue)" "$6"
}
saved() { run "save $1" | grep -Fq "saved slot $1" || fail "save $1 was not accepted"; }
# potion <label>: damage Health by 40 and drink PotionRestoreHealth; Health must rise by its authored 35.
potion() {
    run 'inv.add 0x98496 1' | grep -Fq 'inv.add: 1 x' || fail "$1: the potion was not added"
    run 'obscript.cmd 0x14 DamageAV Health 40' >/dev/null
    local before
    before="$(av Health)"
    run 'inv.use 0x98496' | grep -Fq 'queued use' || fail "$1: the potion use was not queued"
    sleep 2
    expect "$1 potion Health" "$(av Health)" "$(awk -v b="$before" 'BEGIN { print b + 35 }')"
}

# 1. Boot under B (engine slot 3), give the player a distinctive state, save it (slot 1).
launch b1 B
[[ "$(slot_of "$LOG_DIR/b1.log")" == 3 ]] || fail 'set B should place the engine records at Regular(3)'
state b1 'B baseline' 40 80 100 150
run 'obscript.cmd 0x14 ModAV Endurance 5' >/dev/null
run 'obscript.cmd 0x14 DamageAV Health 10' >/dev/null
sleep 1
state b1 'B edited' 45 80 100 155
saved 1
stop_engine

# 2. Boot under A (engine slot 2), save its own state (slot 3), then live-load the B save across the move.
launch a1 A
[[ "$(slot_of "$LOG_DIR/a1.log")" == 2 ]] || fail 'set A should place the engine records at Regular(2)'
state a1 'A baseline' 40 80 100 150
saved 3
live_load a1 1
[[ "$(slot_of "$LOG_DIR/a1.log")" == 3 ]] || fail 'the live load did not install the save set at Regular(3)'
grep -Fq 'load order changed since the actor-value resources were built; rebuilding them' "$LOG_DIR/a1.log" \
    || fail 'the process-lifetime rulesets were not rebuilt on the load-order change (SAVE-2)'
state a1 'A<-B loaded' 45 80 100 155
run hud.status | grep -Eq 'health=0\.8[89]' || fail 'the HUD health bar does not read 80 of 90 after the load'
run 'obscript.cmd 0x14 ModAV Endurance 5' >/dev/null
sleep 1
# The derived pools follow Endurance under the new keys: maximum Health 100 (current 90), Fatigue 160.
expect 'A<-B ModAV Fatigue' "$(av Fatigue)" 160
expect 'A<-B ModAV Health' "$(av Health)" 90
saved 2
potion 'A<-B loaded'
stop_engine

# 3. Boot under B again and load the A save (slot 3) back across the move in the other direction, then
#    load the save made after the first load (slot 2): a stale key in it would be refused or mislabelled.
launch b2 B
[[ "$(slot_of "$LOG_DIR/b2.log")" == 3 ]] || fail 'set B should place the engine records at Regular(3) again'
live_load b2 3
[[ "$(slot_of "$LOG_DIR/b2.log")" == 2 ]] || fail 'the reverse live load did not install the save set at Regular(2)'
grep -Fq 'load order changed since the actor-value resources were built; rebuilding them' "$LOG_DIR/b2.log" \
    || fail 'the rulesets were not rebuilt on the reverse load-order change (SAVE-2)'
state b2 'B<-A loaded' 40 80 100 150
run 'obscript.cmd 0x14 ModAV Endurance 5' >/dev/null
sleep 1
expect 'B<-A ModAV Fatigue' "$(av Fatigue)" 155
saved 4
potion 'B<-A loaded'
live_load b2 2
[[ "$(slot_of "$LOG_DIR/b2.log")" == 3 ]] || fail 'loading the re-saved slot did not return to Regular(3)'
state b2 'B<-resaved loaded' 50 90 100 160
potion 'B<-resaved loaded'
printf 'smoke[oblivion-live-load-plugin-set]: PASS -- A(2)<->B(3) both directions; artifacts: %s\n' "$LOG_DIR"
