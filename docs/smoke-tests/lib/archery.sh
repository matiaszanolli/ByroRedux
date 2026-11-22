#!/usr/bin/env bash
# Shared body of the player-archery live gates (#7055, PRD `player-archery` §7.2 scenario 4): oblivion-archery.sh and
# skyrim-archery.sh. The caller sets GATE, PORT_DEFAULT, ENGINE_ARGS (array), BOW, ARROW (form ids), SHOOT_ARGS (the
# draw seconds `bow.shoot` takes, empty for a full draw), FEED (the `feed=` line `bow.status` must report) and
# LEVEL_SKILL (the skill `levelup` must show gaining 0.8 on a hit, empty where the game feeds none) and optionally HEALTH_MODE
# (`cond` where ObScript does not name the actor values), then sources this
# file. Owner-run: it needs a Vulkan device, a display and the game's data.
#
# In a cell with patrons: the bow and ten arrows are added and equipped (at least ten: the player may already carry some) (`inv.add`, `inv.equip`), `bow.status` names the
# bow, the arrow and the straight trajectory; a shot at the first patron `stealth.status` lists spends one arrow and
# lowers their Health, and the skill feed is what the game's row says; a shot at the ceiling spends one arrow, hurts
# nobody and feeds nothing; and a shot is no melee attack (`combat.status`'s `attacks` does not move).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-$PORT_DEFAULT}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d "/tmp/byro-$GATE.XXXXXX")"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[$GATE]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied"
fi
if pgrep -x byroredux >/dev/null; then
    fail 'another engine instance is already running'
fi
cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail 'build failed'
command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" "$ROOT/target/release/byro-dbg" >"$2" 2>&1 <<CMDS
$1
.quit
CMDS
    sed -i 's/\\n/\n/g' "$2"
}
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    "$ROOT/target/release/byroredux" "${ENGINE_ARGS[@]}" \
    --player --bench-frames 30 --bench-hold >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail 'engine exited before bench-hold'
    (( SECONDS < deadline )) || fail 'timed out waiting for bench-hold'
    sleep 0.25
done

banked() { sed -nE "s/^.*levelup: $2 [0-9]+ banked ([0-9.]+) .*/\1/p" "$1" | head -1; }
skill_value() { sed -nE "s/^.*levelup: $2 ([0-9]+) banked.*/\1/p" "$1" | head -1; }
# Whether `$3` (a skill) gained experience between two `levelup` reports: it banked more, or it ranked up.
gained() {
    python3 - "$(banked "$1" "$3")" "$(skill_value "$1" "$3")" "$(banked "$2" "$3")" "$(skill_value "$2" "$3")" <<'PY'
import sys
b0, v0, b1, v1 = float(sys.argv[1] or 0), int(sys.argv[2] or 0), float(sys.argv[3] or 0), int(sys.argv[4] or 0)
print("yes" if (v1 > v0 or b1 > b0 + 1e-6) else "no")
PY
}
# The patron's Health: Oblivion through the ObScript `GetAV`, Skyrim (whose actor values ObScript does not name) through `cond`
# on the Health AVIF 0x3E8. The caller sets HEALTH_MODE (`obscript` by default, or `cond`).
patron_health() {
    if [[ "${HEALTH_MODE:-obscript}" == cond ]]; then
        command "cond $1 GetActorValue 0x3E8" "$LOG_DIR/health-$1-$2.log" || fail 'cond GetActorValue failed'
        sed -nE 's/.* on entity [0-9]+ = (-?[0-9.]+).*/\1/p' "$LOG_DIR/health-$1-$2.log" | head -1
    else
        command "obscript.cmd $1 GetAV Health" "$LOG_DIR/health-$1-$2.log" || fail 'GetAV Health failed'
        sed -nE 's/.*0x[0-9a-f]+ -> (-?[0-9.]+).*/\1/p' "$LOG_DIR/health-$1-$2.log" | head -1
    fi
}
arrow_count() { sed -nE 's/.*arrow=[0-9A-Fa-f]+ count=([0-9]+).*/\1/p' "$1" | head -1; }
attacks() { sed -nE 's/.*(attacks=[0-9]+).*/\1/p' "$1" | head -1; }

command 'stealth.status' "$LOG_DIR/stealth.log" || fail 'stealth.status failed'
target="$(sed -nE 's/.*entity ([0-9]+) ref .*/\1/p' "$LOG_DIR/stealth.log" | head -1)"
[[ -n "$target" ]] || fail 'no patron to shoot at in the cell'
if [[ -n "$LEVEL_SKILL" ]]; then
    command 'levelup' "$LOG_DIR/levelup-before.log" || fail 'levelup failed'
    grep -E 'fed skills 16/21; unfed: ' "$LOG_DIR/levelup-before.log" | grep -Fq "$LEVEL_SKILL" && fail "$LEVEL_SKILL is still reported unfed"
fi

# Ready: the bow in hand and ten arrows equipped.
command "inv.add $BOW 1" "$LOG_DIR/add-bow.log" || fail 'inv.add (bow) failed'
command "inv.add $ARROW 10" "$LOG_DIR/add-arrows.log" || fail 'inv.add (arrows) failed'
command "inv.equip $BOW" "$LOG_DIR/equip-bow.log" || fail 'inv.equip (bow) failed'
command "inv.equip $ARROW" "$LOG_DIR/equip-arrows.log" || fail 'inv.equip (arrows) failed'
sleep 1
command 'bow.status' "$LOG_DIR/status-ready.log" || fail 'bow.status failed'
grep -Fq "bow=${BOW#0x}" "$LOG_DIR/status-ready.log" || fail "the bow is not readied: $(cat "$LOG_DIR/status-ready.log")"
# The player may already carry arrows of the type (Skyrim's does), so the shots are counted against the start.
start="$(arrow_count "$LOG_DIR/status-ready.log")"
[[ -n "$start" && "$start" -ge 10 ]] || fail "the ten added arrows are not equipped: $(cat "$LOG_DIR/status-ready.log")"
grep -Fq 'trajectory=straight' "$LOG_DIR/status-ready.log" || fail "the trajectory is not straight: $(cat "$LOG_DIR/status-ready.log")"
grep -Fq "$FEED" "$LOG_DIR/status-ready.log" || fail "the feed is not '$FEED': $(cat "$LOG_DIR/status-ready.log")"
command 'combat.status' "$LOG_DIR/combat-before.log" || fail 'combat.status failed'

# A shot at the patron: one arrow spent, the patron hurt.
hp_before="$(patron_health "$target" before)"
command "combat.approach $target
bow.shoot $SHOOT_ARGS" "$LOG_DIR/shoot.log" || fail 'bow.shoot failed'
grep -Fq 'released' "$LOG_DIR/shoot.log" || fail "the shot did not leave: $(cat "$LOG_DIR/shoot.log")"
sleep 2
command 'bow.status' "$LOG_DIR/status-shot.log" || fail 'bow.status failed'
[[ "$(arrow_count "$LOG_DIR/status-shot.log")" == $((start - 1)) ]] || fail "the shot did not spend exactly one arrow: $(cat "$LOG_DIR/status-shot.log")"
hp_after="$(patron_health "$target" after)"
python3 - "$hp_before" "$hp_after" <<'PY' || fail 'the arrow did not hurt the patron'
import sys
assert float(sys.argv[2]) < float(sys.argv[1]), sys.argv
PY
if [[ -n "$LEVEL_SKILL" ]]; then
    command 'levelup' "$LOG_DIR/levelup-hit.log" || fail 'levelup failed'
    [[ "$(gained "$LOG_DIR/levelup-before.log" "$LOG_DIR/levelup-hit.log" "$LEVEL_SKILL")" == yes ]] || fail "$LEVEL_SKILL gained no experience from an arrow that hit"
fi

# A shot at the ceiling: one more arrow spent, nobody hurt, nothing fed.
sleep 1
command 'input.look 0 80
bow.shoot '"$SHOOT_ARGS" "$LOG_DIR/miss.log" || fail 'ceiling shot failed'
grep -Fq 'released' "$LOG_DIR/miss.log" || fail "the ceiling shot did not leave: $(cat "$LOG_DIR/miss.log")"
sleep 2
command 'bow.status' "$LOG_DIR/status-miss.log" || fail 'bow.status failed'
[[ "$(arrow_count "$LOG_DIR/status-miss.log")" == $((start - 2)) ]] || fail "the ceiling shot did not spend one arrow: $(cat "$LOG_DIR/status-miss.log")"
if [[ -n "$LEVEL_SKILL" ]]; then
    command 'levelup' "$LOG_DIR/levelup-miss.log" || fail 'levelup failed'
    [[ "$(gained "$LOG_DIR/levelup-hit.log" "$LOG_DIR/levelup-miss.log" "$LEVEL_SKILL")" == no ]] || fail "a missed arrow fed $LEVEL_SKILL"
fi

# A shot is not a melee attack: combat.status's `attacks` counter is unchanged (the arrow that landed is a hit, so `hits` rises).
command 'combat.status' "$LOG_DIR/combat-after.log" || fail 'combat.status failed'
[[ -n "$(attacks "$LOG_DIR/combat-before.log")" && "$(attacks "$LOG_DIR/combat-before.log")" == "$(attacks "$LOG_DIR/combat-after.log")" ]] || fail 'a bow shot counted as a melee attack'

printf 'smoke[%s]: PASS -- arrow hit entity %s (Health %s -> %s), two arrows spent, the ceiling shot hurt nobody and fed nothing; artifacts: %s\n' \
    "$GATE" "$target" "$hp_before" "$hp_after" "$LOG_DIR"
