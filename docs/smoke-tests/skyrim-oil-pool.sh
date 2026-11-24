#!/usr/bin/env bash
# Hand Equipment PRD §7.2. Run with xvfb-run -a on a display-less host.
# Compare three fresh sessions on the same Skyrim SE reference. Its stock
# equipment check is disabled. The torch and sword comparison enables it via
# the live Papyrus property and selects the script's reset branch the same way.
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
data="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
port="${BYRO_DEBUG_PORT:-19968}"
wait_seconds="${BYROREDUX_SMOKE_TIMEOUT:-120}"
engine_bin="${BYRO_BIN:-$root/target/release/byroredux}"
debug_bin="${BYRO_DBG_BIN:-$root/target/release/byro-dbg}"
engine_pid=""
ref="0009B0DE"

for archive in 'Skyrim.esm' 'Skyrim - Meshes0.bsa' 'Skyrim - Meshes1.bsa' \
               'Skyrim - Textures0.bsa' 'Skyrim - Misc.bsa'; do
    if [[ ! -f "$data/$archive" ]]; then
        echo "smoke[skyrim-oil-pool]: SKIP -- missing $data/$archive"
        exit 77
    fi
done

log_dir="$(mktemp -d /tmp/byro-skyrim-oil.XXXXXX)"
fail() {
    echo "smoke[skyrim-oil-pool]: FAIL -- $*" >&2
    echo "artifacts: $log_dir" >&2
    exit 1
}

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

if ss -ltn "sport = :$port" | grep -q LISTEN; then
    fail "port $port is already occupied"
fi
if [[ -z "${BYRO_BIN:-}" || -z "${BYRO_DBG_BIN:-}" ]]; then
    cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail 'release build failed'
fi
[[ -x "$engine_bin" && -x "$debug_bin" ]] || fail 'missing engine or debug client binary'

debug() {
    printf '%s\n' "$@" '.quit' | BYRO_DEBUG_PORT="$port" timeout "$wait_seconds" "$debug_bin" 2>>"$console_log"
}

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + wait_seconds))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}

run_trial() {
    local trial="$1" item="$2" held="$3" override="$4" expected="$5"
    local engine_log="$log_dir/$trial-engine.log" console_log="$log_dir/$trial-console.log"
    local status pool player crossing deadline
    env BYRO_DEBUG_PORT="$port" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SETTINGS_PATH="$log_dir/$trial-settings.toml" \
        RUST_LOG='warn,byroredux::cell_loader::references::attach=debug,byroredux_scripting::trigger=debug,byroredux_scripting::destruction=debug,byroredux_scripting::fragment::reference=debug,byroredux_scripting::fragment::effects::deferred=debug,byroredux_scripting::fragment::vm::natives::destruction=debug' \
        "$engine_bin" --esm "$data/Skyrim.esm" --cell BleakFallsBarrow01 \
        --bsa "$data/Skyrim - Meshes0.bsa" --bsa "$data/Skyrim - Meshes1.bsa" \
        --textures-bsa "$data/Skyrim - Textures0.bsa" --scripts-bsa "$data/Skyrim - Misc.bsa" \
        --player --bench-frames 5 --bench-hold >"$engine_log" 2>&1 &
    engine_pid=$!
    wait_log "$engine_log" 'bench-hold:'

    pool="$(sed -n "s/^.*model trigger \([0-9]*\) on ref $ref .*/\1/p" "$engine_log")"
    [[ "$pool" =~ ^[0-9]+$ ]] || fail "$trial did not load one trigger for $ref"
    status="$(debug 'player.status')"
    printf '%s\n' "$status" >>"$console_log"
    player="$(sed -n 's/^.*mode=Character player=\([0-9]*\).*$/\1/p' <<<"$status")"
    [[ "$player" =~ ^[0-9]+$ ]] || fail "$trial has no character controller"
    crossing="trigger $pool emitted OnTriggerEnter for actor $player"

    if [[ "$override" == yes ]]; then
        debug "script.exec Game.GetForm(0x$ref).checkActorEquippedItems = True" \
              "script.exec Game.GetForm(0x$ref).deleteSelfAfterIgnition = False" \
              "script.var 0x$ref checkActorEquippedItems" \
              "script.var 0x$ref deleteSelfAfterIgnition" >>"$console_log"
        grep -Fq "$ref.checkActorEquippedItems = Some(1.0)" "$console_log" || fail "$trial equipment override was not applied"
        grep -Fq "$ref.deleteSelfAfterIgnition = Some(0.0)" "$console_log" || fail "$trial reset override was not applied"
    fi

    # Face the pool and place the player just outside its northern edge. Use
    # gameplay input for the crossing so the usual character controller runs.
    debug "inv.add $item" "inv.equip $item" "cam.tp $pool" 'cam.pos 7307.38 -2450 8745' >>"$console_log"
    deadline=$((SECONDS + wait_seconds))
    until status="$(debug 'inventory.status')" && grep -Fq "$held" <<<"$status"; do
        (( SECONDS < deadline )) || fail "$trial did not equip $item"
        sleep 0.25
    done
    printf '%s\n' "$status" >>"$console_log"
    status="$(debug 'magic.effects')"
    printf '%s\n' "$status" >>"$console_log"
    grep -Fq "entity $player: no magic effects" <<<"$status" || fail "$trial started with a magic effect"
    if grep -Fq "$crossing" "$engine_log"; then
        fail "$trial entered during setup"
    fi

    debug 'input.hold forward 25' >>"$console_log"
    wait_log "$engine_log" "$crossing"
    wait_log "$engine_log" "reference $ref: TrapOilPool.OnTriggerEnter"
    if [[ "$expected" == burn ]]; then
        wait_log "$engine_log" "destruction: entity $pool changed from stage -1 to 1"
        wait_log "$engine_log" "destruction: entity $pool changed from stage 1 to 2"
        wait_log "$engine_log" "from base 000889AE at $ref"
        wait_log "$engine_log" "destruction: entity $pool changed from stage 2 to 3"
        wait_log "$engine_log" "Reset: restored destructible reference $ref to its editor location"
    else
        deadline=$((SECONDS + wait_seconds))
        until status="$(debug 'player.status')" && grep -Fq 'input_hold_frames_remaining=0' <<<"$status"; do
            (( SECONDS < deadline )) || fail "$trial walk did not finish"
            sleep 0.25
        done
        if grep -Fq "destruction: entity $pool changed from stage" "$engine_log"; then
            fail "$trial ignited the pool"
        fi
    fi
    debug "script.exec Game.GetForm(0x$ref).SmokeStage = (Game.GetForm(0x$ref) as ObjectReference).GetCurrentDestructionStage()" \
          "script.var 0x$ref SmokeStage" >>"$console_log"
    grep -Fq "$ref.SmokeStage = Some(-1.0)" "$console_log" || fail "$trial ended with a damaged pool"
    if grep -Fq "on $ref stopped:" "$engine_log"; then
        fail "$trial stopped inside the pool script"
    fi

    debug 'engine.quit' >>"$console_log"
    deadline=$((SECONDS + wait_seconds))
    while kill -0 "$engine_pid" 2>/dev/null; do
        (( SECONDS < deadline )) || fail "$trial did not shut down"
        sleep 0.25
    done
    wait "$engine_pid" || fail "$trial exited unsuccessfully"
    engine_pid=""
    echo "smoke[skyrim-oil-pool]: $trial PASS"
}

run_trial stock-torch 1D4EC 'hands left=11 0001D4EC' no unlit
run_trial enabled-torch 1D4EC 'hands left=11 0001D4EC' yes burn
run_trial enabled-sword 12EB7 'hands left=0 right=1 00012EB7' yes unlit
echo "artifacts: $log_dir"
