#!/usr/bin/env bash
# M42 gate — two AI-package procedure fixes, live on FNV.
#
#   1. #5495 — creatures never take furniture. Vault11a spawns giant
#      mantises whose Sandbox packages put furniture markers in reach. The
#      `CreatureActor` marker must be stamped on them at spawn (the first
#      fix gated the seat systems on it but nothing stamped it), they keep
#      their SandboxBehavior, and nothing in the cell is `Seated`.
#   2. UseWeapon (PACK procedure 16). ULKitchen's two White Glove torchers
#      run `VMS18UseFlamer1/2` (unconditional, Repeat Fire, Do No Damage,
#      aimed at in-cell references): each must install
#      `UseWeaponPackageBehavior`, be marked `UseWeaponPackageStarted` once
#      the request is raised, and the volley must keep running rather than
#      give up (the runtime logs "cannot reach ... gives up" after 10 s of
#      shots it cannot take).
#
# FleeNotCombat has no unconditional vanilla placement to boot into, so it
# is unit-gated only (`systems::flee_package`, `npc_spawn::ai_package`).
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19884}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-m42-pkg.XXXXXX)"
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
    echo "smoke[m42-package-procedures]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# The SKIP≠PASS contract (#5427): no game data measures nothing.
DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
for f in "FalloutNV.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m42-package-procedures]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

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

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}

boot_cell() {
    local cell="$1" tag="$2"
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings-$tag.toml" \
        RUST_LOG="warn,byroredux::systems::use_weapon_package=info,byroredux::systems::script_requests=info" \
        target/release/byroredux \
        --game fnv --cell "$cell" \
        --player --bench-frames 30 --bench-hold \
        >"$LOG_DIR/$tag.stdout" 2>"$LOG_DIR/$tag.stderr" &
    engine_pid=$!
    wait_log "$LOG_DIR/$tag.stderr" 'bench-hold:'
    sleep 4
}

# Prints the number of `Entity N` rows `entities(<component>)` lists.
count_entities() {
    debug_command "entities($1)" "$LOG_DIR/$2.log" >/dev/null 2>&1 || true
    grep -c 'Entity [0-9]' "$LOG_DIR/$2.log" || true
}

# ── 1. #5495: creatures stay out of the furniture ───────────────────
boot_cell Vault11a vault11
creatures="$(count_entities CreatureActor vault11-creatures)"
(( creatures > 0 )) || fail "no spawned actor carries CreatureActor: the spawn path does not stamp it (#5495)"
sandboxing="$(count_entities SandboxBehavior vault11-sandbox)"
(( sandboxing > 0 )) || fail "no creature kept its SandboxBehavior; the cell no longer exercises the seat gate"
seated="$(count_entities Seated vault11-seated)"
(( seated == 0 )) || fail "$seated actor(s) are Seated in Vault11a, where only creatures sandbox: $(sed -n '1,4p' "$LOG_DIR/vault11-seated.log")"
echo "smoke[m42-package-procedures]: #5495 PASS — $creatures creatures marked, $sandboxing sandboxing, none seated"
stop_engine

# ── 2. UseWeapon: the torchers' packages install and keep firing ────
boot_cell ULKitchen ulkitchen
installed="$(count_entities UseWeaponPackageBehavior ulkitchen-installed)"
(( installed >= 2 )) || fail "expected both White Glove torchers to install UseWeaponPackageBehavior, found $installed"
started="$(count_entities UseWeaponPackageStarted ulkitchen-started)"
(( started >= 2 )) || fail "the package raised its volley for $started of $installed actors"
grep -Eq 'UseWeapon: actor [0-9]+ uses 0000432D on 00161F3[23] x4294967295' "$LOG_DIR/ulkitchen.stderr" \
    || fail "the runtime never took the package's repeat-fire request: $(grep UseWeapon "$LOG_DIR/ulkitchen.stderr" | head -3)"
sleep 14   # past the runtime's 10 s patience window for shots it cannot take
if grep -Fq 'gives up' "$LOG_DIR/ulkitchen.stderr"; then
    fail "a volley gave up: $(grep 'gives up' "$LOG_DIR/ulkitchen.stderr" | head -2)"
fi
echo "smoke[m42-package-procedures]: UseWeapon PASS — $installed packages installed, $started started, no volley gave up"
echo "smoke[m42-package-procedures]: PASS; artifacts: $LOG_DIR"
