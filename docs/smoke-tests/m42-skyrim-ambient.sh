#!/usr/bin/env bash
# M42 gate — Skyrim ambient seating, live (PRD `ai-packages-completion` §7, "Skyrim sit live").
#
# WhiterunBanneredMare's patrons and innkeeper run Sandbox packages (procedure 18). At 19:00 the
# `sandbox_seat_system` must seat them on Skyrim's own sit-enter clip, the male `chair_leftenter.hkx`
# (AIP-SIT-002): before that clip was wired a Skyrim sandboxer stood at its location. The gate
# requires
#   - `SandboxBehavior` on the patrons and at least one `Seated`;
#   - the clip installed from `animations\male\chair_leftenter.hkx`;
#   - the seated actor's `AnimationPlayer` parked on the clip's last frame (`playing: false`) —
#     Skyrim's ambient actors have no player until the seat inserts one;
#   - a screenshot of one patron, kept with the logs. The pose (thighs on the cushion, root on the
#     furniture's floor) is read by eye: the script does not measure it.
GATE=m42-skyrim-ambient
PROFILE=skyrim_se
DATA_ENV=BYROREDUX_SKYRIMSE_DATA
DATA_DEFAULT="/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data"
DATA_FILES="Skyrim.esm Skyrim@-@Meshes0.bsa Skyrim@-@Textures0.bsa Skyrim@-@Animations.bsa"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/m42_live.sh"
m42_live_init

boot_cell WhiterunBanneredMare mare
debug_command 'time.set 19' "$LOG_DIR/mare-time.log" || fail 'time.set unavailable'
seated="$(wait_until_count Seated 1 'a patron to sit down at 19:00')"
sandboxing="$(count_entities SandboxBehavior mare-sandbox)"
(( sandboxing >= 1 )) || fail "$seated actor(s) seated but none carries SandboxBehavior"
grep -Fq "chair_leftenter.hkx" "$LOG_DIR/mare.stderr" \
    || fail "the Skyrim sit-enter clip was not installed: $(grep -a 'Skyrim character clip' "$LOG_DIR/mare.stderr" | head -3)"

patron="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/wait-Seated.log" | head -1 | cut -d' ' -f2)"
debug_command "inspect $patron" "$LOG_DIR/mare-patron.log" || fail 'inspect unavailable'
python3 - "$LOG_DIR/mare-patron.log" <<'PY' || fail "the seated patron's AnimationPlayer is not parked on the sit clip's last frame ($LOG_DIR/mare-patron.log)"
import re, sys
text = open(sys.argv[1]).read()
block = re.search(r"AnimationPlayer:\s*(\{.*?\n\s*\})", text, re.S)
assert block, "no AnimationPlayer on the seated actor"
player = block.group(1)
assert re.search(r'"playing":\s*false', player), "the player is still playing"
local = float(re.search(r'"local_time":\s*([-0-9.e]+)', player).group(1))
assert 1.5 < local < 2.1, f"local_time {local} is not the 1.77 s clip's last frame"
PY
log_line="$(grep -a '\[sandbox\] seat' "$LOG_DIR/mare.stderr" | head -1)"
[[ -n "$log_line" ]] || fail "no '[sandbox] seat' line: the seat system never logged an assignment"
debug_command "cam.tp $patron" "$LOG_DIR/mare-cam.log" || true
sleep 2
capture mare-seated.png
echo "smoke[$GATE]: Sit PASS — $seated seated of $sandboxing sandboxing on chair_leftenter.hkx, player parked (screenshot: $LOG_DIR/mare-seated.png)"
echo "smoke[$GATE]: PASS; artifacts: $LOG_DIR"
