#!/usr/bin/env bash
# New Vegas from the beginning: a new game through Doc Mitchell's opening to Goodsprings.
#
#   1. `--new-game` boots Doc Mitchell's house; the engine starts the intro quest VCG00,
#      whose stage 0 moves the player to VCG01PlayerStartMarkerREF and hands over to VCG01.
#   2. Doc's lines chain on their own (SayTo → INFO End result scripts → SetStage).
#   3. The five character-creation screens open on the native page and are answered
#      through `chargen` (name, sex/race — VCG01's `Begin MenuMode 1036` moves 36 → 40
#      while it is open — SPECIAL from the Vit-o-matic, tag skills, traits).
#   4. The player does what the route asks: walk into the tester's trigger, use the tester,
#      sit on the couch for the psych test (a conversation), walk to the exit trigger.
#   5. Doc's outro runs on its follow-up lines (TCFU): Pip-Boy, the Vault 21 jumpsuit, the
#      starting gear; the Hardcore box is answered; VCG01 ends at 200 and VCG02 starts.
#   6. The choices hold (GetIsSex / GetIsRace / SPECIAL; Perception is not cut by the
#      Concussion, whose Perception -4 waits on a crippled head), the jumpsuit is worn, the
#      front door leads to Goodsprings, and no far-away greeter opens a conversation there.
#   7. VCG02's shooting tutorial: enabling VCG02BottleMarkerREF (Sunny's line, stood in
#      for) brings its withheld bottles in live (#5065); three pistol shots run their
#      `OnHitWith` and complete objective 10.
#
# Headless (Xvfb). FNV only. The intro movie (FNVIntro.bik) has no decoder and is logged.
#
#   docs/smoke-tests/fnv-new-game.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture fnv-new-game fnv
smoke_require_fixture_fields NG_CELL NG_INTRO_QUEST NG_QUEST NG_START_MARKER NG_NEXT_QUEST \
    NG_TESTER_TRIGGER NG_TESTER NG_COUCH_TRIGGER NG_COUCH NG_EXIT_TRIGGER NG_DOOR NG_DOOR_GRID \
    NG_NAME NG_RACE NG_TAGS NG_VAULT_SUIT NG_BOTTLE_MARKER NG_BOTTLES NG_PISTOL NG_AMMO
smoke_require_data

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-600}"

LOG_DIR="$(mktemp -d /tmp/byro-fnv-new-game.XXXXXX)"
engine_pid=""
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    # The pid is the xvfb-run wrapper; the engine is the port's listener.
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    (( keep_artifacts == 0 )) && rm -rf "$LOG_DIR"
    return 0
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[fnv-new-game]: FAIL -- $*"
    echo "smoke[fnv-new-game]: artifacts retained at $LOG_DIR"
    tail -30 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}
pass() { echo "smoke[fnv-new-game]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[fnv-new-game]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

# `debug_command <commands> [logfile]` (newline-separated commands).
debug_command() {
    local commands="$1" out="${2:-/dev/stdout}"
    { printf '%s\n' "$commands"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$out" 2>&1
}
require() {
    local file="$1" needle="$2" description="$3"
    grep -Fq -- "$needle" "$file" || fail "$description (missing '$needle')"
    pass "$description"
}
stage_of() {
    debug_command "quest.show $1" "$LOG_DIR/stage.tmp"
    grep -o 'current-stage: [0-9]*' "$LOG_DIR/stage.tmp" | grep -o '[0-9]*' || echo -1
}

cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
    RUST_LOG="warn,byroredux::app_step=info,byroredux::cell_loader::enabled=info,byroredux::systems::npc_dialogue=info,byroredux::npc_spawn::loot_appearance=info,byroredux_scripting::obscript_results=info,byroredux::player_seat=info,byroredux::systems::travel=info" \
    "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$NG_CELL" --new-game --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

deadline=$((SECONDS + 240))
until grep -q 'bench-hold:' "$LOG_DIR/engine.stderr" 2>/dev/null; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
pass "engine reached the held new game"

# ── 1 — the intro quest the engine starts moves the player to Doc's ──────────
deadline=$((SECONDS + 60))
until grep -q "Scripted player MoveTo: → resident reference $NG_START_MARKER" "$LOG_DIR/engine.stderr"; do
    (( SECONDS < deadline )) || fail "VCG00 never moved the player to the start marker"
    sleep 1
done
pass "VCG00 moved the player to VCG01PlayerStartMarkerREF"

# ── 2..5 — drive the opening ─────────────────────────────────────────────────
declare -A screens=() done_steps=()
quiet=0
deadline=$((SECONDS + TIMEOUT))
while :; do
    (( SECONDS < deadline )) || fail "VCG01 stalled at stage $(stage_of "$NG_QUEST") ($(printf '%s ' "${!screens[@]}")screens answered)"
    stage=$(stage_of "$NG_QUEST")
    (( stage >= 200 )) && break
    debug_command chargen "$LOG_DIR/chargen.status"
    if grep -q ') is open' "$LOG_DIR/chargen.status"; then
        screen=$(grep -o 'chargen: [A-Za-z]*' "$LOG_DIR/chargen.status" | head -1 | cut -d' ' -f2)
        case "$screen" in
            Name) answer="name $NG_NAME" ;;
            RaceSex) answer="race female $NG_RACE" ;;
            Special)
                # Every attribute at its base, the five spare points on Strength.
                # byro-dbg prints the reply's line breaks as `\n` escapes.
                read -r -a values <<<"$(sed 's/\\n/\n/g' "$LOG_DIR/chargen.status" | grep -o 'special:.*' | head -1 | grep -o '=[0-9]*' | tr -d '=' | tr '\n' ' ')"
                (( ${#values[@]} == 7 )) || fail "the SPECIAL screen listed ${#values[@]} attributes"
                sum=0; for v in "${values[@]}"; do sum=$((sum + v)); done
                values[0]=$((values[0] + 40 - sum))
                answer="special ${values[*]}" ;;
            TagSkills) answer="tags $NG_TAGS" ;;
            Traits) answer="traits" ;;
            *) fail "unexpected character-creation screen '$screen'" ;;
        esac
        debug_command "chargen $answer" "$LOG_DIR/chargen.answer"
        require "$LOG_DIR/chargen.answer" "answered $screen" "the $screen screen opened and took its answer"
        screens[$screen]=1
        sleep 1
        continue
    fi
    smoke_answer_message_boxes "$LOG_DIR/answer.log"
    # A conversation that offers topics once the speaker has finished: take the first.
    debug_command dialogue.status "$LOG_DIR/dialogue.status"
    offer=$(grep -o 'offers topic 0x[0-9A-Fa-f]*' "$LOG_DIR/dialogue.status" | head -1 | awk '{print $3}' || true)
    if [[ -n "$offer" ]] && ! grep -q 'speaking=true' "$LOG_DIR/dialogue.status"; then
        quiet=$((quiet + 1))
        if (( quiet >= 2 )); then
            debug_command "dialogue.select $offer" "$LOG_DIR/dialogue.select"
            quiet=0
        fi
    else
        quiet=0
    fi
    player=$(debug_command player.status /dev/stdout | grep -o 'player=[0-9]*' | head -1 | cut -d= -f2)
    if (( stage == 55 )) && [[ -z "${done_steps[tester]:-}" ]]; then
        debug_command "obscript.cmd $player 109e r:$NG_TESTER_TRIGGER" /dev/null; done_steps[tester]=1
    fi
    if (( stage == 60 )) && [[ -z "${done_steps[use]:-}" ]] &&
        debug_command "quest.show $NG_QUEST" /dev/stdout | grep -q '30: displayed=true completed=false'; then
        debug_command "script.activate $NG_TESTER" /dev/null; done_steps[use]=1
    fi
    if (( stage == 80 )) && [[ -z "${done_steps[couch]:-}" ]]; then
        debug_command "obscript.cmd $player 109e r:$NG_COUCH_TRIGGER" /dev/null
        sleep 2
        debug_command "script.activate $NG_COUCH" /dev/null; done_steps[couch]=1
    fi
    if (( stage == 110 )) && [[ -z "${done_steps[exit]:-}" ]]; then
        debug_command "obscript.cmd $player 109e r:$NG_EXIT_TRIGGER" /dev/null; done_steps[exit]=1
    fi
    sleep 2
done
pass "VCG01 reached stage 200"
for screen in Name RaceSex Special TagSkills Traits; do
    [[ -n "${screens[$screen]:-}" ]] || fail "the $screen screen never opened"
done
pass "every character-creation screen opened on the page"
require "$LOG_DIR/engine.stderr" "follows up with" "Doc's outro ran on its follow-up lines"
require "$LOG_DIR/engine.stderr" "player sits on furniture" "the player sat on the couch"
require "$LOG_DIR/engine.stderr" "Travel package: actor" "Doc sat down at the end of his Travel package"

# ── 6 — the result ───────────────────────────────────────────────────────────
debug_command "quest.show $NG_QUEST" "$LOG_DIR/vcg01.quest"
require "$LOG_DIR/vcg01.quest" "state: completed" "VCG01 is completed"
debug_command "quest.show $NG_NEXT_QUEST" "$LOG_DIR/vcg02.quest"
require "$LOG_DIR/vcg02.quest" "current-stage: 5" "VCG02 started at stage 5"
player=$(debug_command player.status /dev/stdout | grep -o 'player=[0-9]*' | head -1 | cut -d= -f2)
debug_command "prid $player
cond . GetIsSex 1
cond . GetIsRace 0x$NG_RACE
cond . GetActorValue 5
cond . GetActorValue 6" "$LOG_DIR/identity.log"
require "$LOG_DIR/identity.log" "GetIsSex(0x1, 0) on entity $player = 1" "the chosen sex holds"
require "$LOG_DIR/identity.log" "GetIsRace(0x$(printf '%X' "$((16#$NG_RACE))"), 0) on entity $player = 1" "the chosen race holds"
require "$LOG_DIR/identity.log" "GetActorValue(0x5, 0) on entity $player = 10" "the spare points went to Strength"
require "$LOG_DIR/identity.log" "GetActorValue(0x6, 0) on entity $player = 5" "Perception is not cut by an uncrippled Concussion"
deadline=$((SECONDS + 30))
until grep -q "imported armor.vaultsuit" "$LOG_DIR/engine.stderr"; do
    (( SECONDS < deadline )) || fail "the Vault 21 jumpsuit was never put on the player's body"
    sleep 1
done
pass "the jumpsuit Doc hands over is worn"

debug_command "script.activate $NG_DOOR" "$LOG_DIR/door.log"
deadline=$((SECONDS + 120))
until debug_command player.status "$LOG_DIR/player.status" && grep -Fq "grid=$NG_DOOR_GRID grounded=true" "$LOG_DIR/player.status"; do
    (( SECONDS < deadline )) || fail "the front door never led to Goodsprings ($(cat "$LOG_DIR/player.status"))"
    sleep 2
done
pass "the front door leads to Goodsprings"
sleep 30
debug_command dialogue.status "$LOG_DIR/outside.status"
if grep -q '^byro> "npc [0-9]*: topic' "$LOG_DIR/outside.status"; then
    fail "a conversation opened by itself outside: $(head -2 "$LOG_DIR/outside.status")"
fi
pass "no greeter opened a conversation from afar"

# ── 7 — the shooting tutorial: bottles a resident Enable() brings in, shot ───
# Sunny's line enables VCG02BottleMarkerREF while Goodsprings is loaded; its enable
# children (the bottles) were spawned withheld and must get their content live (#5065).
debug_command "obscript.cmd $NG_BOTTLE_MARKER 1021" "$LOG_DIR/enable.log"
first_bottle=${NG_BOTTLES%% *}
deadline=$((SECONDS + 30))
until grep -q "Enabled reference $first_bottle: its content spawned" "$LOG_DIR/engine.stderr"; do
    (( SECONDS < deadline )) || fail "enabling the bottle marker never brought the bottles in ($(cat "$LOG_DIR/enable.log"))"
    sleep 1
done
pass "a resident Enable() of the bottle marker spawns its bottles"
debug_command "obscript.cmd $player 1002 r:$NG_PISTOL 1
obscript.cmd $player 1002 r:$NG_AMMO 30" /dev/null
sleep 1
debug_command inventory.status "$LOG_DIR/inventory.log"
grep -q "equipped_weapon=0x$NG_PISTOL" "$LOG_DIR/inventory.log" || debug_command "inv.equip $NG_PISTOL" /dev/null
sleep 1
numbers() { grep -o "$1 *([^)]*)" "$2" | head -1 | grep -oE '[-+]?[0-9]+(\.[0-9]+)?' | tr '\n' ' '; }
for bottle in $NG_BOTTLES; do
    debug_command "mesh.info 0x$bottle" "$LOG_DIR/bottle.$bottle"
    read -r bx by bz <<<"$(numbers 'Transform.global: *pos' "$LOG_DIR/bottle.$bottle")"
    [[ -n "${bz:-}" ]] || fail "bottle $bottle has no placement ($(head -3 "$LOG_DIR/bottle.$bottle"))"
    debug_command "phys.census $bx $bz 40" "$LOG_DIR/census.$bottle"
    grep -q "form=0x${bottle#00}" "$LOG_DIR/census.$bottle" || fail "bottle $bottle has no collision body"
    # Stand 400 units west-north-west of the bottle (clear of the fence it sits on), aim, fire.
    debug_command "cam.pos $(awk -v x="$bx" -v y="$by" -v z="$bz" 'BEGIN { printf "%.1f %.1f %.1f", x - 382, y + 60, z + 118 }')" /dev/null
    sleep 4
    debug_command cam.where "$LOG_DIR/cam.where"
    read -r cx cy cz <<<"$(numbers 'position:' "$LOG_DIR/cam.where")"
    debug_command "input.look $(awk -v bx="$bx" -v by="$by" -v bz="$bz" -v cx="$cx" -v cy="$cy" -v cz="$cz" 'BEGIN {
        dx = bx - cx; dy = by + 6 - cy; dz = bz - cz; deg = 180 / atan2(0, -1)
        printf "%.2f %.2f", atan2(-dx, -dz) * deg, atan2(dy, sqrt(dx * dx + dz * dz)) * deg }')" /dev/null
    sleep 1
    debug_command "input.press attack" /dev/null
    sleep 3
done
debug_command "quest.show $NG_NEXT_QUEST" "$LOG_DIR/vcg02.bottles"
require "$LOG_DIR/vcg02.bottles" "nTargetCount=3" "three pistol shots hit three bottles (OnHitWith)"
grep -Eq ' 10: displayed=(true|false) completed=true' "$LOG_DIR/vcg02.bottles" ||
    fail "the shooting objective (10) did not complete ($(grep ' 10:' "$LOG_DIR/vcg02.bottles"))"
pass "the shooting objective completes"
echo "smoke[fnv-new-game]: PASS -- intro -> character creation -> the opening -> Goodsprings -> the shooting tutorial"
