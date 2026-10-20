#!/usr/bin/env bash
# FO4 ragdoll activation and XRGD-posed corpses, live (#6070, split from #5015).
#
# The FO4 ragdoll decode and the human / feral-ghoul `XRGD` corpse poses were
# verified against the data files only. This drives them in the engine, in two
# interiors, each session under `BYRO_VALIDATION=1`:
#
#   MedTekResearch01   the authored corpse Loot_CorpseFeralGhoul01 (REFR
#                      00106A72, 18 XRGD entries) is posed from its XRGD and
#                      simulates an 18-body ragdoll; then the P2 fixture's
#                      LvlFeralGhoul (REFR 00084CEC) dies to real swings.
#   ShamrockTaphouse01 the authored corpse Loot_CorpseSettlerMale (REFR
#                      000F123A, 19 XRGD entries: 18 bodies + the marker) the
#                      same; then a LvlRaiderMelee (REFR 0007432D, a human) and
#                      a LvlMoleratAggro512Delayed (REFR 000F2A45, a creature
#                      whose rig is not posable) die to real swings.
#
# Every ragdoll must report all bodies live, finite, and no invalid-solve
# recovery; every session must be validation-clean. Not gated: the death
# hitch (no per-frame timing is read back).
#
#   docs/smoke-tests/fo4-ragdoll.sh
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19882}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-fo4-ragdoll.XXXXXX)"
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
    echo "smoke[fo4-ragdoll]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
ARCHIVES=(
    "Fallout4 - Meshes.ba2" "Fallout4 - MeshesExtra.ba2" "Fallout4 - Textures1.ba2"
    "Fallout4 - Materials.ba2"
)
for f in "Fallout4.esm" "${ARCHIVES[@]}"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[fo4-ragdoll]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done
# The validation leg is part of this gate. Captured first: `vulkaninfo` can
# exit non-zero after printing, and `grep -q` closing the pipe early would
# flip the check under `pipefail` (the p5-f5-f9 probe).
vulkan_layer_table="$( (vulkaninfo --summary 2>/dev/null) || true)"
if ! grep -q "VK_LAYER_KHRONOS_validation" <<<"$vulkan_layer_table"; then
    echo "smoke[fo4-ragdoll]: SKIP -- missing the Vulkan Khronos validation layer"
    exit 77
fi
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg \
        >"$2" 2>&1 <<EOF
$1
.quit
EOF
}

# The entity a REFR spawned, found by its `mesh.info` FormID line among the
# actor roots named `$1` (the resolved NPC_'s editor id, lowercased).
entity_of_ref() {
    local name="$1" refr="$2" candidate
    debug_command "entities Inventory" "$LOG_DIR/entities.log" || fail "entities unavailable"
    while read -r candidate; do
        debug_command "mesh.info $candidate" "$LOG_DIR/mesh.log" || continue
        if grep -Fq "REFR FormID:       $refr" "$LOG_DIR/mesh.log"; then
            echo "$candidate"
            return 0
        fi
    done < <(sed -nE "s/^ *Entity ([0-9]+) \"$name\".*/\1/p" "$LOG_DIR/entities.log")
    return 1
}

# `ragdoll.status` for `$1` must show `$2` bodies (or any count when empty),
# all live, finite and never recovered.
check_ragdoll() {
    local actor="$1" bodies="$2" what="$3"
    debug_command "ragdoll.status $actor" "$LOG_DIR/ragdoll.log" || fail "ragdoll.status unavailable"
    grep -Eq "bodies=${bodies:-[0-9]+} live=${bodies:-[0-9]+} complete=true finite=true" \
        "$LOG_DIR/ragdoll.log" \
        || fail "$what ragdoll is not ${bodies:-fully} live and finite: $(cat "$LOG_DIR/ragdoll.log")"
    grep -Fq "recoveries_total=0" "$LOG_DIR/ragdoll.log" \
        || fail "$what ragdoll needed an invalid-solve recovery (#4772)"
    sed -nE 's/.*(bodies=[0-9]+).*(max_distance=[0-9.]+).*/\1 \2/p' "$LOG_DIR/ragdoll.log"
}

# Swing at `$1` until the combat trace names it killed; at most 60 swings.
kill_by_swings() {
    local target="$1"
    for swing in $(seq 1 60); do
        debug_command "combat.approach $target" "$LOG_DIR/approach.log" || fail "approach failed"
        debug_command "input.press attack" "$LOG_DIR/press.log" || fail "attack failed"
        sleep 0.6
        debug_command "combat.status" "$LOG_DIR/combat.log" || fail "combat.status failed"
        if grep -q "last_target=$target .*killed=true" "$LOG_DIR/combat.log"; then
            grep -oE "ragdoll (activated \([0-9]+ bodies\)|unavailable: .*)" "$LOG_DIR/combat.log" \
                | tail -1
            return 0
        fi
    done
    fail "60 swings at $target never killed it"
}

launch() {
    local cell="$1"
    stop_engine
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 BYRO_VALIDATION=1 \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
        RUST_LOG="warn,byroredux::combat=debug" \
        target/release/byroredux \
        --esm "$DATA/Fallout4.esm" \
        --bsa "$DATA/Fallout4 - Meshes.ba2" --bsa "$DATA/Fallout4 - MeshesExtra.ba2" \
        --textures-bsa "$DATA/Fallout4 - Textures1.ba2" \
        --materials-ba2 "$DATA/Fallout4 - Materials.ba2" \
        --cell "$cell" --player --bench-frames 30 --bench-hold \
        >"$LOG_DIR/$cell.stdout" 2>"$LOG_DIR/$cell.stderr" &
    engine_pid=$!
    local deadline=$((SECONDS + TIMEOUT))
    until grep -Fq 'bench-hold:' "$LOG_DIR/$cell.stderr"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "$cell: engine exited before bench-hold"
        (( SECONDS < deadline )) || fail "$cell: timeout waiting for bench-hold"
        sleep 0.25
    done
    sleep 2
}

# An authored Starts-Dead corpse is posed from its XRGD before its ragdoll
# seeds, at the Late-stage death sink.
check_authored_corpse() {
    local cell="$1" name="$2" refr="$3"
    local corpse
    corpse="$(entity_of_ref "$name" "$refr")" || fail "$cell: authored corpse $refr not found"
    grep -Eq "actor $corpse: death reconciled; posed 18 bones from XRGD; ragdoll activated \(18 bodies\)" \
        "$LOG_DIR/$cell.stderr" \
        || fail "$cell: corpse $refr (entity $corpse) was not posed from XRGD: $(grep -F "actor $corpse:" "$LOG_DIR/$cell.stderr" | tail -1)"
    local summary
    summary="$(check_ragdoll "$corpse" 18 "$refr")"
    echo "smoke[fo4-ragdoll]: $cell corpse $refr posed 18 bones from XRGD; $summary"
}

check_kill() {
    local cell="$1" name="$2" refr="$3" bodies="$4"
    local target activation
    target="$(entity_of_ref "$name" "$refr")" || fail "$cell: $name $refr not found"
    activation="$(kill_by_swings "$target")"
    [[ "$activation" == "ragdoll activated ("* ]] || fail "$cell: $name died without a ragdoll: $activation"
    sleep 1
    local summary
    summary="$(check_ragdoll "$target" "$bodies" "$name")"
    echo "smoke[fo4-ragdoll]: $cell $name killed, $activation; $summary"
}

check_validation() {
    local cell="$1"
    grep -Fq 'Vulkan VALIDATION ENABLED' "$LOG_DIR/$cell.stderr" \
        || fail "$cell: validation leg did not run (BYRO_VALIDATION ignored?)"
    if grep -E 'Validation Error|VUID-' "$LOG_DIR/$cell.stdout" "$LOG_DIR/$cell.stderr" >/dev/null; then
        grep -E 'Validation Error|VUID-' "$LOG_DIR/$cell.stdout" "$LOG_DIR/$cell.stderr" | head -5 >&2
        fail "$cell: Vulkan validation errors during the session"
    fi
    echo "smoke[fo4-ragdoll]: $cell validation clean"
}

launch MedTekResearch01
check_authored_corpse MedTekResearch01 loot_corpseferalghoul01 0x106A72
check_kill MedTekResearch01 lvlferalghoul 0x084CEC ""
check_validation MedTekResearch01

launch ShamrockTaphouse01
check_authored_corpse ShamrockTaphouse01 loot_corpsesettlermale 0x0F123A
check_kill ShamrockTaphouse01 lvlraidermelee 0x07432D ""
check_kill ShamrockTaphouse01 lvlmolerataggro512delayed 0x0F2A45 ""
check_validation ShamrockTaphouse01

echo "smoke[fo4-ragdoll]: PASS"
