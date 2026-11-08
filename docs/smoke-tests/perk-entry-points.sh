#!/usr/bin/env bash
# M24.2 Phase 2 gate (docs/prd/perk-entry-points.md §7.2 scenario 6, A6) — taking a perk, live.
#
#   fnv     FNV, Doc Mitchell's house (Goodsprings): `levelup xp` to the first perk level, then
#           1. `levelup perk` on a perk whose record conditions fail is REFUSED and names the failing
#              function (Black Widow / Lady Killer: one needs a female player and the other a male, so
#              whichever the player is, exactly one is refused with `GetIsSex`; PERK-GATE-001);
#           2. `levelup perk` on Swift Learner is ACCEPTED (its Intelligence >= 4 is a
#              `GetPermanentActorValue` condition the evaluator models);
#           3. `perk.compose 9 100 player` shows the Swift Learner entry applied (100 -> 110;
#              PERK-DX-001), and `perk.hooks inert` lists the GECK's Not Implemented hooks (PERK-DX-002).
#   skyrim  Skyrim SE, HaltedStreamCamp01: take a perk with an Ability entry (Mara's Blessing,
#           T02MaraReward) and `inspect` on the player shows its spell in `SpellList` exactly once —
#           after taking it, and again after F5 / F9 (PERK-BODY-001/-002, PERK-DX-006).
#
# Both legs also assert the install summary line (PERK-DX-004) and that the engine did not panic.
#
# Usage: docs/smoke-tests/perk-entry-points.sh [fnv|skyrim]    (default: both)
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19887}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-perk-entry-points.XXXXXX)"
engine_pid=""
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

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
    echo "smoke[perk-entry-points]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# SKIP != PASS (#5427): a runner without the game's data measures nothing.
require_data() { # <label> <data dir> <file>...
    local label="$1" dir="$2"
    shift 2
    for f in "$@"; do
        if [[ ! -f "$dir/$f" ]]; then
            echo "smoke[perk-entry-points]: SKIP -- $label: missing $dir/$f"
            return 1
        fi
    done
}

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail "release build failed"

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Eq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.5
    done
}

debug_command() { # <command> <output file>
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF2
$1
.quit
EOF2
    sed -i 's/\\n/\n/g' "$2"
}

launch() { # <label> <engine args...>
    local label="$1"
    shift
    mkdir -p "$LOG_DIR/$label-saves"
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SAVE_DIR="$LOG_DIR/$label-saves" \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/$label-settings.toml" \
        RUST_LOG="warn,byroredux::scene=info,byroredux::save_io=info" \
        target/release/byroredux "$@" --player --bench-frames 30 --bench-hold \
        >"$LOG_DIR/$label.stdout" 2>"$LOG_DIR/$label.stderr" &
    engine_pid=$!
    wait_log "$LOG_DIR/$label.stderr" 'bench-hold:'
    sleep 3
}

finish() { # <label>
    if grep -Fq 'panicked' "$LOG_DIR/$1.stderr"; then
        fail "$1: the engine panicked: $(grep -F 'panicked' "$LOG_DIR/$1.stderr" | head -1)"
    fi
    stop_engine
}

# ── FNV ────────────────────────────────────────────────────────────────────────────────────────
run_fnv() {
    local data="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
    require_data fnv "$data" FalloutNV.esm "Fallout - Meshes.bsa" || return 77
    launch fnv --game fnv --cell "${BYRO_SMOKE_FNV_CELL:-GSDocMitchellHouse}"

    # PERK-DX-004 — one install line naming the dialect, even for a game with perks.
    grep -Eq 'perk entry points: dialect Fallout3NV, 176 perks' "$LOG_DIR/fnv.stderr" \
        || fail "fnv: no install summary line: $(grep -F 'perk entry points' "$LOG_DIR/fnv.stderr" | head -1)"

    # 1400 XP reaches level 4: perk levels 2 and 4 bank two perk points.
    debug_command 'levelup xp 1400' "$LOG_DIR/fnv-xp.log" || fail 'fnv: levelup unavailable'
    debug_command 'levelup' "$LOG_DIR/fnv-status.log" || fail 'fnv: levelup unavailable'
    grep -Eq 'perk_points=2' "$LOG_DIR/fnv-status.log" \
        || fail "fnv: expected two banked perk points: $(grep -F 'levelup:' "$LOG_DIR/fnv-status.log" | head -1)"

    # Black Widow (0x94EB8) needs a female player, Lady Killer (0x94EB9) a male one (GetIsSex).
    local refused=0 accepted=0 perk
    for perk in 94EB8 94EB9; do
        debug_command "levelup perk $perk" "$LOG_DIR/fnv-gate-$perk.log" || fail 'fnv: levelup unavailable'
        if grep -Fq 'requires GetIsSex' "$LOG_DIR/fnv-gate-$perk.log"; then
            refused=$((refused + 1))
        elif grep -Fq 'now rank 1' "$LOG_DIR/fnv-gate-$perk.log"; then
            accepted=$((accepted + 1))
        else
            fail "fnv: unexpected answer for perk $perk: $(sed -n '1,3p' "$LOG_DIR/fnv-gate-$perk.log")"
        fi
    done
    [[ "$refused" == 1 && "$accepted" == 1 ]] \
        || fail "fnv: expected exactly one of Black Widow / Lady Killer refused naming GetIsSex (refused=$refused accepted=$accepted)"

    # Swift Learner (0x31DD3): Intelligence >= 4, a GetPermanentActorValue condition. One point is left.
    debug_command 'levelup perk 31DD3' "$LOG_DIR/fnv-swift.log" || fail 'fnv: levelup unavailable'
    grep -Fq 'perk 00031DD3 now rank 1' "$LOG_DIR/fnv-swift.log" \
        || fail "fnv: Swift Learner was not accepted: $(sed -n '1,3p' "$LOG_DIR/fnv-swift.log")"

    # PERK-DX-001 — hook 9 (Adjust Experience Points): Swift Learner rank 1 multiplies by 1.1.
    debug_command 'perk.compose 9 100 player' "$LOG_DIR/fnv-compose.log" || fail 'fnv: perk.compose unavailable'
    grep -Fq 'applied  perk 00031DD3' "$LOG_DIR/fnv-compose.log" \
        || fail "fnv: perk.compose does not show Swift Learner applied: $(cat "$LOG_DIR/fnv-compose.log")"
    grep -Eq '100 -> 110' "$LOG_DIR/fnv-compose.log" \
        || fail "fnv: perk.compose did not yield 110: $(cat "$LOG_DIR/fnv-compose.log")"

    # PERK-DX-002 — the GECK's Not Implemented hooks are catalogued Inert.
    debug_command 'perk.hooks inert' "$LOG_DIR/fnv-hooks.log" || fail 'fnv: perk.hooks unavailable'
    grep -Fq 'Adjust Range Penalty' "$LOG_DIR/fnv-hooks.log" \
        || fail "fnv: perk.hooks inert does not list Adjust Range Penalty: $(sed -n '1,4p' "$LOG_DIR/fnv-hooks.log")"

    finish fnv
    echo "smoke[perk-entry-points]: PASS (fnv) -- one of Black Widow / Lady Killer refused naming GetIsSex; Swift Learner accepted; perk.compose 9 100 -> 110"
}

# ── Skyrim ─────────────────────────────────────────────────────────────────────────────────────
run_skyrim() {
    local data="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
    require_data skyrim "$data" Skyrim.esm "Skyrim - Meshes0.bsa" || return 77
    launch skyrim --game skyrim_se --cell "${BYRO_SMOKE_SKYRIM_CELL:-HaltedStreamCamp01}"

    grep -Eq 'perk entry points: dialect Skyrim, 375 perks' "$LOG_DIR/skyrim.stderr" \
        || fail "skyrim: no install summary line: $(grep -F 'perk entry points' "$LOG_DIR/skyrim.stderr" | head -1)"

    # The player entity: the only one carrying a Perks component with entries.
    debug_command 'entities Perks' "$LOG_DIR/sk-perks.log" || fail 'skyrim: entities unavailable'
    local player
    # The NPCs carry names; the player's row has none.
    player="$(grep -E '^ *(byro> +)?Entity [0-9]+ *$' "$LOG_DIR/sk-perks.log" | head -1 | grep -oE '[0-9]+' | head -1)"
    [[ -n "$player" ]] || fail "skyrim: no unnamed Perks entity (the player): $(sed -n '1,5p' "$LOG_DIR/sk-perks.log")"

    local spell=837530 # 0x000CC79A, PerkMara — the Ability entry of T02MaraReward (0x000CC799)
    spell_count() { # <output file>
        debug_command "inspect $player" "$1" || fail 'skyrim: inspect unavailable'
        awk '/^  SpellList:/{p=1;next} p&&/^  [A-Z][A-Za-z]+:/{exit} p{print}' "$1" | tr -d ' \n' \
            | grep -o "\\b$spell\\b" | wc -l
    }
    [[ "$(spell_count "$LOG_DIR/sk-before.log")" == 0 ]] || fail "skyrim: the player already carries PerkMara"

    debug_command 'levelup xp 2000' "$LOG_DIR/sk-xp.log" || fail 'skyrim: levelup unavailable'
    debug_command 'levelup perk CC799' "$LOG_DIR/sk-take.log" || fail 'skyrim: levelup unavailable'
    grep -Fq 'perk 000CC799 now rank 1' "$LOG_DIR/sk-take.log" \
        || fail "skyrim: T02MaraReward was not taken: $(sed -n '1,3p' "$LOG_DIR/sk-take.log")"
    sleep 2 # perk_body_system runs on the next tick
    [[ "$(spell_count "$LOG_DIR/sk-after.log")" == 1 ]] \
        || fail "skyrim: the Ability spell is not in SpellList exactly once after taking the perk"

    # PERK-BODY-002 — F5 / F9: the reloaded SpellList still holds it once.
    debug_command 'input.press quicksave' "$LOG_DIR/sk-f5.log" || fail 'skyrim: quicksave unavailable'
    sleep 4
    debug_command 'input.press quickload' "$LOG_DIR/sk-f9.log" || fail 'skyrim: quickload unavailable'
    wait_log "$LOG_DIR/skyrim.stderr" 'save load: restored player pose'
    sleep 3
    [[ "$(spell_count "$LOG_DIR/sk-reload.log")" == 1 ]] \
        || fail "skyrim: the Ability spell is not in SpellList exactly once after F5 / F9"

    finish skyrim
    echo "smoke[perk-entry-points]: PASS (skyrim) -- PerkMara is in the player's SpellList once after taking T02MaraReward and after F5 / F9"
}

legs=("${@:-fnv skyrim}")
[[ "${legs[0]}" == "fnv skyrim" ]] && legs=(fnv skyrim)
status=0
for leg in "${legs[@]}"; do
    case "$leg" in
        fnv) run_fnv || status=$? ;;
        skyrim) run_skyrim || status=$? ;;
        *) echo "usage: $0 [fnv|skyrim]" >&2; exit 2 ;;
    esac
done
echo "artifacts: $LOG_DIR"
exit "$status"
