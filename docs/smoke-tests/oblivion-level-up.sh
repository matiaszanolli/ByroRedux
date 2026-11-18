#!/usr/bin/env bash
# Owner-run live gate for Oblivion skill-use leveling, PRD §7.2 scenario 3 (banking and rank-up; #7050).
# Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA. Artifacts remain in the printed folder.
#
# In Anvil's Count's Arms: `levelup` names the Player record's class and each skill's banked experience and
# cost; `ModPCSkill Blade 2` raises the base skill by two ranks, counts them toward the level, and tallies
# Strength; `AdvancePCSkill Sneak 1` tallies Agility and, Sneak being no major skill of the class, not the level.
# Phase 2b (#7051): a jump banks Acrobatics (0.3), moving undetected in the sneak stance banks Sneak (0.75 a second),
# and `levelup` lists the unfed skills.
set -euo pipefail

DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo 'smoke[oblivion-level-up]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)'
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19882}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-oblivion-level-up.XXXXXX)"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[oblivion-level-up]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
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

# The value of a skill in a `levelup` report ("levelup: Blade 15 banked 0.00 / cost ...").
skill_value() { sed -nE "s/^.*levelup: $2 ([0-9]+) banked.*/\1/p" "$1" | head -1; }

command 'levelup' "$LOG_DIR/before.log" || fail 'levelup failed'
grep -Eq 'levelup: class [A-Za-z0-9_]+ \((Combat|Magic|Stealth)\), major skills: ' "$LOG_DIR/before.log" \
    || fail 'levelup did not name the class and its major skills'
grep -Fq 'major increases 0/10' "$LOG_DIR/before.log" || fail 'the level count did not start at 0/10'
blade_before="$(skill_value "$LOG_DIR/before.log" Blade)"
sneak_before="$(skill_value "$LOG_DIR/before.log" Sneak)"
[[ -n "$blade_before" && -n "$sneak_before" ]] || fail 'levelup reported no Blade / Sneak'

command 'obscript.cmd none ModPCSkill Blade 2' "$LOG_DIR/mod.log" || fail 'ModPCSkill failed'
grep -q '\[declined:' "$LOG_DIR/mod.log" && fail 'ModPCSkill was declined'
command 'obscript.cmd none AdvancePCSkill Sneak 1' "$LOG_DIR/advance.log" || fail 'AdvancePCSkill failed'
grep -q '\[declined:' "$LOG_DIR/advance.log" && fail 'AdvancePCSkill was declined'

command 'levelup' "$LOG_DIR/after.log" || fail 'levelup failed after the raises'
blade_after="$(skill_value "$LOG_DIR/after.log" Blade)"
sneak_after="$(skill_value "$LOG_DIR/after.log" Sneak)"
(( blade_after == blade_before + 2 )) || fail "Blade $blade_before -> $blade_after, expected +2 (Amount is ranks)"
(( sneak_after == sneak_before + 1 )) || fail "Sneak $sneak_before -> $sneak_after, expected +1"
grep -Eq 'Blade [0-9]+ banked 0\.00 .* major' "$LOG_DIR/after.log" || fail 'ModPCSkill left experience banked, or Blade is no major skill'
python3 - "$LOG_DIR/after.log" <<'PY' || fail 'the level count and the attribute tallies did not follow the rank-ups'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
major = re.search(r"major increases (\d+)/10", text)
assert major and int(major.group(1)) == 2, text
tallies = dict(re.findall(r"(\w+) (\d+)(?:,|$)", re.search(r"attribute tallies: (.*)", text).group(1)))
assert tallies["Strength"] == "2" and tallies["Agility"] == "1", tallies
PY
# Phase 2b: the emitters. `levelup` reports 8 of 21 skills fed and names what is not.
grep -Fq 'fed skills 8/21; unfed: ' "$LOG_DIR/after.log" || fail 'levelup did not report the fed skills'
for unfed in Security Speechcraft Mercantile Alchemy Armorer Block Marksman Alteration; do
    grep -E 'fed skills 8/21; unfed: ' "$LOG_DIR/after.log" | grep -Fq "$unfed" || fail "$unfed is not reported unfed"
done
banked() { sed -nE "s/^.*levelup: $2 [0-9]+ banked ([0-9.]+) .*/\1/p" "$1" | head -1; }
# Running (forward and Sprint together) cannot be held from the console: `input.hold` holds one key. The stance
# rules are unit-tested (`skill_use_emit::tests`); here the sneak stance and a jump run live.
# Sneaking undetected: the stance, then forward for a few seconds.
command 'stealth.sneak on' "$LOG_DIR/sneak-on.log" || fail 'stealth.sneak failed'
sneak_before_run="$(skill_value "$LOG_DIR/after.log" Sneak)"
sleep 1
command 'input.hold forward 600' "$LOG_DIR/hold-sneak.log" || fail 'input.hold forward failed (sneak)'
sleep 5
command 'stealth.status' "$LOG_DIR/stealth-mid.log" || fail 'stealth.status failed'
sleep 7
command 'stealth.status' "$LOG_DIR/stealth-end.log" || fail 'stealth.status failed'
command 'levelup' "$LOG_DIR/sneak.log" || fail 'levelup failed after the sneak'
sneak_after="$(skill_value "$LOG_DIR/sneak.log" Sneak)"
sneak_banked="$(banked "$LOG_DIR/sneak.log" Sneak)"
# Sneak banks only while undetected (UESP; the worst detection level below Noticed). The cell's patrons may see the
# player: then the skill must not move, and the leg proves the gate the other way round.
worst() { sed -nE 's/.*worst level ([A-Za-z]+).*/\1/p' "$1" | head -1; }
mid_level="$(worst "$LOG_DIR/stealth-mid.log")"
end_level="$(worst "$LOG_DIR/stealth-end.log")"
python3 - "$sneak_banked" "$sneak_after" "$sneak_before_run" "$mid_level" "$end_level" <<'PY' || fail 'the Sneak skill did not follow the detection level'
import sys
banked, after, before = float(sys.argv[1] or 0), int(sys.argv[2] or 0), int(sys.argv[3] or 0)
levels = sys.argv[4:6]
undetected = all(level in ("Lost", "Unseen") for level in levels)
gained = banked > 0 or after > before
print(f"smoke[oblivion-level-up]: sneak leg, detection {levels}, gained={gained}")
assert gained == undetected or (gained and not undetected and any(l in ("Lost", "Unseen") for l in levels)), (banked, after, before, levels)
PY
# A jump.
command 'input.press jump' "$LOG_DIR/jump.log" || fail 'input.press jump failed'
sleep 2
command 'levelup' "$LOG_DIR/jumped.log" || fail 'levelup failed after the jump'
python3 - "$(banked "$LOG_DIR/jumped.log" Acrobatics)" <<'PY' || fail 'a jump banked no Acrobatics experience'
import sys
assert float(sys.argv[1] or 0) > 0, sys.argv
PY
printf 'smoke[oblivion-level-up]: PASS -- Blade %s -> %s, Sneak %s -> %s; artifacts: %s\n' \
    "$blade_before" "$blade_after" "$sneak_before" "$sneak_after" "$LOG_DIR"
