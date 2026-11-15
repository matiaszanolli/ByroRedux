#!/usr/bin/env bash
# M65 gate — Robotics Expert, live on FNV (PRD §7.2 scenario 7, #6912; the Perk Entry Points PRD's A8 live leg).
#
#   1. Boot the FNV profile into Lucky38BasementFloorB2 (the Phase 0 robot cell, design §13.2) with a player.
#      The robot is ACRE 0015B8C2 (a Lucky38SecuritronMkI), found in `stealth.status` by its position.
#   2. Give the player Robotics Expert (`AddPerk 00031DC2`, `obscript.cmd`) and the stance (`stealth.sneak on`).
#   3. Stand beside the robot facing it (`combat.approach`) under a Stealth Boy's Chameleon, so it is not
#      detecting the player (`GetDetected` 0, the perk's target tab) and the interaction ray targets it.
#   4. `input.press activate` raises the activate choice; `ui.answer 1` picks "Deactivate" (button 0 is the
#      default activation); the robot reads `GetUnconscious` 1.
#
# Exits 77 when the FNV data is missing.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19881}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-m65-robotics.XXXXXX)"
ROBOT_AT="(4128, 8936, 2290)"
ROBOTICS_EXPERT="00031DC2"
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
    echo "smoke[m65-robotics-expert]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m65-robotics-expert]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

ask() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg <<CMDS 2>&1 | sed 's/\\n/\n/g'
$1
.quit
CMDS
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
    RUST_LOG="warn,byroredux::perk_activate=info" \
    target/release/byroredux \
    --game fnv --cell Lucky38BasementFloorB2 \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 2

player="$(ask player.status | grep -o 'player=[0-9]*' | head -1 | cut -d= -f2)"
[[ -n "$player" ]] || fail "no player entity"
robot="$(ask stealth.status | grep -F "at $ROBOT_AT" | sed -E 's/.*entity ([0-9]+) .*/\1/' | head -1)"
[[ -n "$robot" ]] || fail "the robot at $ROBOT_AT is not a detector: $(ask stealth.status | head -5)"
echo "smoke[m65-robotics-expert]: player $player, robot entity $robot"

ask "obscript.cmd $player 1176 r:$ROBOTICS_EXPERT" >"$LOG_DIR/addperk.log"
ask "stealth.sneak on" | grep -Fq "stealth: sneaking" || fail "the stance was refused"

# The securitrons stand in a row against a wall, facing the room, in the light: a robot sees a player at its
# flank (190° cone). A Stealth Boy (Chameleon, FNV AVIF 000005DF) removes visual contact (SNK-DET, the formula's
# stealth-boy factor), which is how a player gets beside a lit robot unseen.
ask "combat.approach $robot" >"$LOG_DIR/approach.log"
grep -Fq "placed in melee range" "$LOG_DIR/approach.log" || fail "combat.approach failed: $(cat "$LOG_DIR/approach.log")"
ask "setav $player 0x000005DF 100" >"$LOG_DIR/chameleon.log"
sleep 1
detected="$(ask "cond $robot GetDetected 0x14" | grep -o '= [0-9]*' | tr -d '= ')"
target="$(ask interaction.status | grep -o 'target=[^ ]*' | head -1)"
echo "smoke[m65-robotics-expert]: beside the robot under Chameleon: GetDetected=$detected $target"
[[ "$detected" == "0" ]] || fail "the robot still detects the player: $(ask stealth.status | grep -F "entity $robot ")"
[[ "$target" == *"$robot"* ]] || fail "the interaction ray does not target the robot: $(ask interaction.status)"
placed="melee range, under Chameleon"

ask "input.press activate" >"$LOG_DIR/press.log"
sleep 1
ask "ui.answer 1" >"$LOG_DIR/answer.log"
grep -Fqv "no message box is waiting" "$LOG_DIR/answer.log" || fail "no activate choice was offered: $(cat "$LOG_DIR/answer.log")"
sleep 1
unconscious="$(ask "obscript.cmd $robot 10f2" | grep -o '0x10f2 -> [0-9.]*' | awk '{print $3}')"
[[ "$unconscious" == "1" ]] || fail "the robot is not unconscious after Deactivate: GetUnconscious=$unconscious; $(cat "$LOG_DIR/answer.log")"
echo "smoke[m65-robotics-expert]: PASS -- Robotics Expert deactivated the robot from $placed"
