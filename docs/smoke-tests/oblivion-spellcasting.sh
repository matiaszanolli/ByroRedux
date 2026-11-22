#!/usr/bin/env bash
# Owner-run live gate for the player's spell cast, player-spellcasting PRD §7.2 scenario 3 (#7081).
# Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA. Artifacts remain in the printed folder.
#
# In Anvil's Count's Arms: Flare (000A97DF, Destruction, Target) is readied and cast at the first patron
# `stealth.status` lists. The projectile lands (`cast.status` says `impact=landed`), the patron loses Health, and
# `levelup` shows Destruction's experience rise by the SKIL second use value (1.2) while `levelup` no longer lists the
# six schools as unfed. A cast at the ceiling misses, pays its Magicka and feeds nothing. Heal Minor Wounds (00000136,
# Restoration, Self) feeds Restoration (0.6). The console's `magic.cast` starts the same effects and feeds nothing, and
# `combat.status` is unchanged by every cast. The negative leg (PSC-TGT-002): Burning Touch (0004C91E, Destruction,
# Touch) cast at the room's middle cupboard, a container whose collider the touch ray reaches, lands nothing, pays its
# Magicka and feeds nothing.
set -euo pipefail

DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo 'smoke[oblivion-spellcasting]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)'
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19882}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-oblivion-spellcasting.XXXXXX)"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[oblivion-spellcasting]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
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
    "$ROOT/target/release/byroredux" --esm "$DATA/Oblivion.esm" --cell AnvilTheCountsArms \
    --bsa "$DATA/Oblivion - Meshes.bsa" --textures-bsa "$DATA/Oblivion - Textures - Compressed.bsa" \
    --player --bench-frames 30 --bench-hold >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail 'engine exited before bench-hold'
    (( SECONDS < deadline )) || fail 'timed out waiting for bench-hold'
    sleep 0.25
done

FLARE=0x000A97DF
HEAL=0x00000136
TOUCH=0x0004C91E
skill_value() { sed -nE "s/^.*levelup: $2 ([0-9]+) banked.*/\1/p" "$1" | head -1; }
banked() { sed -nE "s/^.*levelup: $2 [0-9]+ banked ([0-9.]+) .*/\1/p" "$1" | head -1; }
# Whether `$2` (a skill) gained experience between two `levelup` reports: it banked more, or it ranked up.
gained() {
    python3 - "$(banked "$1" "$3")" "$(skill_value "$1" "$3")" "$(banked "$2" "$3")" "$(skill_value "$2" "$3")" <<'PY'
import sys
b0, v0, b1, v1 = float(sys.argv[1] or 0), int(sys.argv[2] or 0), float(sys.argv[3] or 0), int(sys.argv[4] or 0)
print("yes" if (v1 > v0 or b1 > b0 + 1e-6) else "no")
PY
}
patron_health() { command "obscript.cmd $1 GetAV Health" "$LOG_DIR/health-$1-$2.log" || fail 'GetAV Health failed'; sed -nE 's/.*0x[0-9a-f]+ -> (-?[0-9.]+).*/\1/p' "$LOG_DIR/health-$1-$2.log" | head -1; }

command 'levelup' "$LOG_DIR/before.log" || fail 'levelup failed'
grep -Fq 'fed skills 16/21; unfed: ' "$LOG_DIR/before.log" || fail 'levelup did not report 16 of 21 skills fed'
for school in Alteration Conjuration Destruction Illusion Mysticism Restoration; do
    grep -E 'fed skills 16/21; unfed: ' "$LOG_DIR/before.log" | grep -Fq "$school" && fail "$school is still reported unfed"
done
command 'stealth.status' "$LOG_DIR/stealth.log" || fail 'stealth.status failed'
target="$(sed -nE 's/.*entity ([0-9]+) ref .*/\1/p' "$LOG_DIR/stealth.log" | head -1)"
[[ -n "$target" ]] || fail 'no patron to cast at in the cell'
command 'spell.list' "$LOG_DIR/list.log" || fail 'spell.list failed'
grep -Fq "${FLARE#0x}" "$LOG_DIR/list.log" || command "obscript.cmd 0x14 0x101c r:$FLARE" "$LOG_DIR/addspell.log"
command 'combat.status' "$LOG_DIR/combat-before.log" || fail 'combat.status failed'

# Flare at the patron: the projectile lands, the patron is hurt, Destruction is fed once.
command "spell.ready $FLARE" "$LOG_DIR/ready.log" || fail 'spell.ready failed'
grep -Fq 'readied Flare' "$LOG_DIR/ready.log" || fail "Flare was not readied: $(cat "$LOG_DIR/ready.log")"
hp_before="$(patron_health "$target" before)"
command "combat.approach $target
spell.cast" "$LOG_DIR/cast.log" || fail 'spell.cast failed'
grep -Fq 'projectile launched' "$LOG_DIR/cast.log" || fail "the cast did not launch a projectile: $(cat "$LOG_DIR/cast.log")"
sleep 2
command 'cast.status' "$LOG_DIR/status.log" || fail 'cast.status failed'
grep -Fq 'impact=landed' "$LOG_DIR/status.log" || fail "the projectile did not land: $(cat "$LOG_DIR/status.log")"
grep -Fq 'fed Destruction' "$LOG_DIR/status.log" || fail "the landing did not feed Destruction: $(cat "$LOG_DIR/status.log")"
hp_after="$(patron_health "$target" after)"
python3 - "$hp_before" "$hp_after" <<'PY' || fail 'Flare did not hurt the patron'
import sys
assert float(sys.argv[2]) < float(sys.argv[1]), sys.argv
PY
command 'levelup' "$LOG_DIR/after-flare.log" || fail 'levelup failed'
[[ "$(gained "$LOG_DIR/before.log" "$LOG_DIR/after-flare.log" Destruction)" == yes ]] || fail 'Destruction gained no experience from a landed Flare'
for other in Alteration Conjuration Illusion Mysticism Restoration; do
    [[ "$(gained "$LOG_DIR/before.log" "$LOG_DIR/after-flare.log" $other)" == no ]] || fail "$other gained experience from a Destruction cast"
done

# A miss: aimed at the ceiling the projectile ends on a solid, the Magicka is paid, nothing is fed.
sleep 1
command 'input.look 0 80
spell.cast' "$LOG_DIR/miss.log" || fail 'miss cast failed'
sleep 2
command 'cast.status' "$LOG_DIR/miss-status.log" || fail 'cast.status failed'
grep -Fq 'impact=missed' "$LOG_DIR/miss-status.log" || fail "the ceiling shot did not miss: $(cat "$LOG_DIR/miss-status.log")"
command 'levelup' "$LOG_DIR/after-miss.log" || fail 'levelup failed'
[[ "$(gained "$LOG_DIR/after-flare.log" "$LOG_DIR/after-miss.log" Destruction)" == no ]] || fail 'a missed Flare fed Destruction'

# A Self spell feeds its school at the release: Heal Minor Wounds, Restoration (0.6).
sleep 1
command "spell.ready $HEAL" "$LOG_DIR/ready-heal.log" || fail 'spell.ready failed'
command 'spell.cast' "$LOG_DIR/heal.log" || fail 'Self cast failed'
grep -Fq 'fed Restoration' "$LOG_DIR/heal.log" || fail "the Self cast did not feed Restoration: $(cat "$LOG_DIR/heal.log")"
command 'levelup' "$LOG_DIR/after-heal.log" || fail 'levelup failed'
[[ "$(gained "$LOG_DIR/after-miss.log" "$LOG_DIR/after-heal.log" Restoration)" == yes ]] || fail 'Restoration gained no experience from a Self cast'

# The console's `magic.cast` starts the effects and feeds nothing.
sleep 1
command "magic.cast $FLARE $target" "$LOG_DIR/magic-cast.log" || fail 'magic.cast failed'
sleep 1
command 'levelup' "$LOG_DIR/after-console.log" || fail 'levelup failed'
[[ "$(gained "$LOG_DIR/after-heal.log" "$LOG_DIR/after-console.log" Destruction)" == no ]] || fail 'the console cast fed Destruction'

# A Touch cast at a container lands nothing and feeds nothing (PSC-TGT-002). The middle cupboard (middlecupboard01 at
# -88.8 63.2 -440.9) is reached from its open +X side, 100 BU away; the -X side is behind architecture. The interaction
# prompt must name the cupboard as a Container before the cast, so a miss into empty air cannot pass for this leg.
sleep 1
command 'cam.pos 11.2 123.7 -440.9' "$LOG_DIR/chest-pos.log" || fail 'cam.pos failed'
sleep 2
command 'input.look 90 -26' "$LOG_DIR/chest-look.log" || fail 'input.look failed'
sleep 1
command 'interaction.status' "$LOG_DIR/chest-aim.log" || fail 'interaction.status failed'
chest="$(sed -nE 's/.*target=([0-9]+) kind=Container .*/\1/p' "$LOG_DIR/chest-aim.log" | head -1)"
[[ -n "$chest" ]] || fail "the aim is not on a container: $(cat "$LOG_DIR/chest-aim.log")"
command 'spell.list' "$LOG_DIR/list-touch.log" || fail 'spell.list failed'
grep -Fq "${TOUCH#0x}" "$LOG_DIR/list-touch.log" || command "obscript.cmd 0x14 0x101c r:$TOUCH" "$LOG_DIR/addspell-touch.log"
command "spell.ready $TOUCH" "$LOG_DIR/ready-touch.log" || fail 'spell.ready failed'
grep -Fq 'readied Burning Touch' "$LOG_DIR/ready-touch.log" || fail "Burning Touch was not readied: $(cat "$LOG_DIR/ready-touch.log")"
command 'spell.cast' "$LOG_DIR/chest.log" || fail 'Touch cast failed'
grep -Fq 'started Burning Touch' "$LOG_DIR/chest.log" || fail "the Touch cast did not start: $(cat "$LOG_DIR/chest.log")"
grep -Fq 'effects none applied, no target' "$LOG_DIR/chest.log" || fail "the Touch cast landed on the container: $(cat "$LOG_DIR/chest.log")"
grep -Fq 'fed ' "$LOG_DIR/chest.log" && fail "the Touch cast at a container fed a school: $(cat "$LOG_DIR/chest.log")"
grep -Eq 'magicka [1-9][0-9.]*,' "$LOG_DIR/chest.log" || fail "the Touch cast at a container paid no Magicka: $(cat "$LOG_DIR/chest.log")"
command 'levelup' "$LOG_DIR/after-chest.log" || fail 'levelup failed'
[[ "$(gained "$LOG_DIR/after-console.log" "$LOG_DIR/after-chest.log" Destruction)" == no ]] || fail 'a Touch cast at a container fed Destruction'

# A cast is not an attack: combat.status's counters are unchanged.
command 'combat.status' "$LOG_DIR/combat-after.log" || fail 'combat.status failed'
counters() { sed -nE 's/.*(attacks=[0-9]+ hits=[0-9]+ kills=[0-9]+).*/\1/p' "$1" | head -1; }
[[ "$(counters "$LOG_DIR/combat-before.log")" == "$(counters "$LOG_DIR/combat-after.log")" ]] || fail 'a spell cast changed the combat counters'

printf 'smoke[oblivion-spellcasting]: PASS -- Flare landed on entity %s (Health %s -> %s), Destruction and Restoration fed, miss, console cast and Touch at container %s fed nothing; artifacts: %s\n' \
    "$target" "$hp_before" "$hp_after" "$chest" "$LOG_DIR"
