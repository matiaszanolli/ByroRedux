#!/usr/bin/env bash
# M42 gate — Oblivion package procedures, live (PRD `ai-packages-completion` §7, "Oblivion live").
#
#   1. Eat seats the actor (AIP-SIT-001). Oblivion has no Sandbox procedure; its seating comes from
#      Eat. BravilCastlePrivateQuartersNorthWing runs `BravilCastleFormalDinner` (20:00-24:00, In
#      Cell) for Regulus and Gellius Terentius, and a guard eats at 18:00. At 21:00 the dinner
#      packages install `EatBehavior` and the diners must end up `Seated`, on the sit-enter clip
#      `chair_leftenter.kf` (the clip that was `None` before AIP-SIT-001 — the actors walked to the
#      table and never sat). A screenshot of one diner is kept with the logs for the pose.
#
#   2. Find a container (AIP-FIND-007, type 6). TempleMothCatacombs02's priest runs
#      `TempleMothPriestStorage20x2`: it must install `FindPackageBehavior`, start a `FindProgress`
#      with a container as its target, and then actually walk: the route to the container climbs a
#      47-degree ramp at its start (where the walk used to stand for ever, the vertical support
#      correction cancelling the climb) and runs under low ceilings (where one airborne tick used to
#      set it down on the roof, 595 to 916). The gate requires the priest to be 500 units from its
#      start after 30 s. Reaching the container and raising the `ActivateEvent` is NOT asserted: the
#      route ends at a closed door, and NPCs do not open doors (PRD AIP-NG-004). The script prints
#      that as a NOTE; `STRICT_FIND=1` makes the missing "activates" line a failure.
#
# The chair half of AIP-FIND-007 has no vanilla cell to boot into: the placed Find-a-chair packages
# name a base object that the cell does not place (GottlefontHouse's bench), or an exterior tree.
# It stays unit-gated (`systems::find_package`).
GATE=m42-oblivion-packages
PROFILE=oblivion
DATA_ENV=BYROREDUX_OBLIVION_DATA
DATA_DEFAULT="/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data"
DATA_FILES="Oblivion.esm Oblivion@-@Meshes.bsa Oblivion@-@Textures@-@Compressed.bsa"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/m42_live.sh"
m42_live_init

# ── 1. Eat seats the dinner guests ──────────────────────────────────
boot_cell BravilCastlePrivateQuartersNorthWing bravil
debug_command 'time.set 21' "$LOG_DIR/bravil-time.log" || fail 'time.set unavailable'
seated="$(wait_until_count Seated 1 'a diner to sit down at 21:00')"
eating="$(count_entities EatBehavior bravil-eat)"
(( eating >= 1 )) || fail "$seated actor(s) seated but none carries EatBehavior: the seats are not the Eat package's"
grep -Fq "registered from 'meshes\\characters\\_male\\idleanims\\chair_leftenter.kf'" "$LOG_DIR/bravil.stderr" \
    || fail "the sit-enter clip is not chair_leftenter.kf: $(grep -a 'KF clip' "$LOG_DIR/bravil.stderr" | head -3)"
diner="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/wait-Seated.log" | head -1 | cut -d' ' -f2)"
debug_command "cam.tp $diner" "$LOG_DIR/bravil-cam.log" || true
sleep 2
capture bravil-seated.png
echo "smoke[$GATE]: Eat PASS — $seated seated, $eating eating, on chair_leftenter.kf (screenshot: $LOG_DIR/bravil-seated.png)"
stop_engine

# ── 2. Find: the Moth priest searches for a container and walks to it ──
boot_cell TempleMothCatacombs02 moth
debug_command 'time.set 20' "$LOG_DIR/moth-time.log" || fail 'time.set unavailable'
hunters="$(wait_until_count FindPackageBehavior 1 'the priest to install FindPackageBehavior at 20:00')"
priest="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/wait-FindPackageBehavior.log" | head -1 | cut -d' ' -f2)"
progress="$(wait_until_count FindProgress 1 'the priest to start a FindProgress')"

# The priest's world position and Find target, from `inspect`.
priest_state() {
    debug_command "inspect $priest" "$LOG_DIR/moth-priest.log" || fail 'inspect unavailable'
    python3 - "$LOG_DIR/moth-priest.log" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
pos = re.search(r'GlobalTransform:\s*\{\s*"translation":\s*\[\s*([-0-9.e]+),\s*([-0-9.e]+),\s*([-0-9.e]+)', text, re.S)
target = re.search(r'FindProgress:\s*\{\s*"target":\s*([^,\s]+)', text, re.S)
print(pos.group(1), pos.group(2), pos.group(3), target.group(1) if target else "none")
PY
}
read -r x0 y0 z0 target0 <<<"$(priest_state)"
sleep 30
read -r x1 y1 z1 target1 <<<"$(priest_state)"
walked="$(python3 -c "import math; print(int(math.dist(($x0, $z0), ($x1, $z1))))")"
(( walked >= 500 )) || fail "the priest walked only $walked units in 30 s (from $x0,$y0,$z0 to $x1,$y1,$z1): it is held at the ramp again"
python3 -c "import sys; sys.exit(0 if abs($y1 - $y0) < 400 else 1)" \
    || fail "the priest's height moved from $y0 to $y1: it was set down on a roof or fell through a floor"
[[ "$target0$target1" != nullnull ]] || fail "the priest started a FindProgress but never picked a container"
if grep -Eq 'Find package: actor [0-9]+ activates' "$LOG_DIR/moth.stderr"; then
    echo "smoke[$GATE]: Find PASS — the priest walked $walked units, reached a container and activated it"
elif [[ "${STRICT_FIND:-0}" == 1 ]]; then
    fail "the priest walked $walked units but never activated a container: $(grep -a 'Find package' "$LOG_DIR/moth.stderr" | head -2)"
else
    echo "smoke[$GATE]: Find PASS (partial) — the priest picked a container and walked $walked units; NOTE: no activation yet, the route ends at a closed door (STRICT_FIND=1 to require it)"
fi
echo "smoke[$GATE]: PASS; artifacts: $LOG_DIR"
