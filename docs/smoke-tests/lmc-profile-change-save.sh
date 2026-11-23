#!/usr/bin/env bash
# Live gate for the profile-change saves (PRD legacy-mod-compatibility LMC-SAV-002, -003, -004; Phase 7, #6676):
# `lmc-profile-change-save.sh`. Fallout: New Vegas, the Prospector Saloon (fixtures/fnv.env P1_*), headless.
#
# Four small plugins are generated from FalloutNV.esm's own saloon CELL (copied whole, as a mod's override would):
#   LMCGone.esp    a MISC token (01000800) and a reference of it placed in the saloon (01000801);
#   LMCAdded.esp   moves one vanilla static of the saloon and adds a Start Game Enabled quest (LMCAddedQuest);
#   LMCOrderA.esp  and LMCOrderB.esp each move one other vanilla static, B after A.
#
#   1. Save: FalloutNV.esm + LMCGone + LMCOrderA + LMCOrderB. The placed token is resident, the player is given two
#      tokens, slot 1 is saved.
#   2. Load into a changed profile: FalloutNV.esm + LMCOrderB + LMCOrderA + LMCAdded (LMCGone removed, A and B
#      swapped, LMCAdded added); `load 1` live. The load completes (a removed plugin no longer refuses it), and its
#      report names: LMCGone not loaded with its one reference left out and the token stack removed (LMC-SAV-002);
#      the reference LMCAdded edits, whose saved state masks the edit (LMC-SAV-003a); the reference the swap gave
#      another winner, LMCOrderA now (LMC-SAV-004); the Start Game Enabled quest the save had not seen (LMC-SAV-003b),
#      which is running after the load. The tokens are gone from the inventory, and the placed token is not resident.
#   3. The changed profile saves (slot 2) and loads that save back with no removed-plugin line.
#
# Requires the New Vegas data (BYROREDUX_FNV_DATA), python3, xvfb-run and a Vulkan device; exits 77 without the data.
# Binaries: BYRO_BIN / BYRO_DBG_BIN, else target/release (built when missing).
set -euo pipefail

GATE=lmc-profile-change-save
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture "$GATE" fnv
smoke_require_fixture_fields P1_CELL P1_CAMERA_POS P1_CAMERA_FORWARD
smoke_require_data
command -v python3 >/dev/null || { echo "smoke[$GATE]: SKIP -- missing python3"; exit 77; }
command -v xvfb-run >/dev/null || { echo "smoke[$GATE]: SKIP -- missing xvfb-run"; exit 77; }

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="${BYRO_BIN:-$ROOT_DIR/target/release/byroredux}"
DEBUG_BIN="${BYRO_DBG_BIN:-$ROOT_DIR/target/release/byro-dbg}"
PORT="${BYRO_DEBUG_PORT:-19887}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-240}"
WORK="$(mktemp -d "/tmp/byro-$GATE.XXXXXX")"
engine_pid=""
# Stopping xvfb-run does not wait for the engine under it: the engine holding the debug port is stopped too, and the
# next leg starts once the port is free.
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
    local holder deadline=$((SECONDS + 30))
    while holder="$(ss -ltnp "sport = :$PORT" 2>/dev/null | grep -oP 'pid=\K[0-9]+' | head -1)" && [[ -n "$holder" ]]; do
        kill -TERM "$holder" 2>/dev/null || true
        (( SECONDS < deadline )) || { kill -KILL "$holder" 2>/dev/null || true; }
        sleep 0.5
    done
}
trap stop_engine EXIT
fail() { echo "smoke[$GATE]: FAIL -- $*; artifacts: $WORK" >&2; exit 1; }
pass() { echo "smoke[$GATE]: ok -- $*"; }
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi
if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg) || fail 'build failed'
fi

# The plugins, from the saloon's CELL record and two of its plain placed statics; prints the two statics' FormIDs.
mkdir -p "$WORK/mods/lmc"
read -r ADDED_REF ORDER_REF < <(python3 -I - "$SMOKE_DATA/$FIXTURE_ESM" "$WORK/mods/lmc" 00106185 <<'PY'
import os, struct, sys

esm_path, out, cell_id = sys.argv[1], sys.argv[2], int(sys.argv[3], 16)


def sub(tag, data):
    return tag + struct.pack('<H', len(data)) + data


def record(tag, form, body, flags=0):
    return tag + struct.pack('<IIIIHH', len(body), flags, form, 0, 15, 0) + body


def grup(label, kind, body):
    return b'GRUP' + struct.pack('<I4sIHHHH', 24 + len(body), label, kind, 0, 0, 15, 0) + body


def subrecords(body):
    at, out = 0, []
    while at + 6 <= len(body):
        tag, size = body[at:at + 4], struct.unpack_from('<H', body, at + 4)[0]
        out.append((tag, body[at + 6:at + 6 + size]))
        at += 6 + size
    return out


data = open(esm_path, 'rb').read()


def walk(start, end):
    """Yield (offset, header tag, size field, label/flags, form/type) for each entry in [start, end)."""
    at = start
    while at < end:
        tag = data[at:at + 4]
        size, a, b = struct.unpack_from('<III', data, at + 4)
        yield at, tag, size, a, b
        at += size if tag == b'GRUP' else 24 + size


tops = {}
for at, tag, size, label, kind in walk(24 + struct.unpack_from('<I', data, 4)[0], len(data)):
    if tag == b'GRUP' and kind == 0:
        tops[struct.pack('<I', label)] = (at, size)

stat_ids = set()
at, size = tops[b'STAT']
for _, tag, rsize, flags, form in walk(at + 24, at + size):
    if tag == b'STAT':
        stat_ids.add(form)

cell_record, temporary = None, []
at, size = tops[b'CELL']


def scan(start, end):
    global cell_record
    for pos, tag, rsize, a, b in walk(start, end):
        if tag == b'GRUP':
            if b == 6 and a != cell_id:
                continue
            if b == 9 and a == cell_id:
                for rpos, rtag, rs, flags, form in walk(pos + 24, pos + rsize):
                    if rtag == b'REFR' and not flags & 0x40000:
                        temporary.append((form, flags, data[rpos + 24:rpos + 24 + rs]))
                continue
            scan(pos + 24, pos + rsize)
        elif tag == b'CELL' and b == cell_id:
            cell_record = data[pos:pos + 24 + rsize]


scan(at + 24, at + size)
assert cell_record, f'cell {cell_id:08X} not found'
# Placed statics with no enable parent, not initially disabled, no teleport: plain set dressing.
statics = []
for form, flags, body in temporary:
    subs = dict(subrecords(body))
    if flags & 0x800 or b'XESP' in subs or b'XTEL' in subs or b'NAME' not in subs or b'DATA' not in subs:
        continue
    if struct.unpack('<I', subs[b'NAME'])[0] in stat_ids:
        statics.append((form, flags, body))
assert len(statics) >= 2, 'fewer than two plain statics in the cell'
(added_ref, added_flags, added_body), (order_ref, order_flags, order_body) = statics[0], statics[1]
cell_local = cell_id & 0xFFFFFF
block, sub_block = cell_local % 10, (cell_local // 10) % 10


def moved(body, dz):
    parts = []
    for tag, value in subrecords(body):
        if tag == b'DATA':
            x, y, z, rx, ry, rz = struct.unpack('<6f', value)
            value = struct.pack('<6f', x, y, z + dz, rx, ry, rz)
        parts.append(sub(tag, value))
    return b''.join(parts)


def cell_group(refs):
    children = grup(struct.pack('<I', cell_id), 9, b''.join(refs))
    body = cell_record + grup(struct.pack('<I', cell_id), 6, children)
    return grup(b'CELL', 0, grup(struct.pack('<i', block), 2, grup(struct.pack('<i', sub_block), 3, body)))


def plugin(name, groups, records):
    head = sub(b'HEDR', struct.pack('<fII', 1.34, records, 0x900)) + sub(b'CNAM', b'ByroRedux\0')
    head += sub(b'MAST', b'FalloutNV.esm\0') + sub(b'DATA', b'\0' * 8)
    with open(os.path.join(out, name), 'wb') as f:
        f.write(record(b'TES4', 0, head) + b''.join(groups))


x, y, z = struct.unpack('<3f', dict(subrecords(added_body))[b'DATA'][:12])
token = record(b'MISC', 0x01000800, sub(b'EDID', b'LMCGoneToken\0') + sub(b'OBND', b'\0' * 12) +
               sub(b'FULL', b'LMC Gone Token\0') + sub(b'MODL', b'Clutter\\Junk\\TinCan01.nif\0') +
               sub(b'DATA', struct.pack('<if', 1, 0.1)))
placed = record(b'REFR', 0x01000801, sub(b'NAME', struct.pack('<I', 0x01000800)) +
                sub(b'DATA', struct.pack('<6f', x, y, z + 40.0, 0, 0, 0)))
plugin('LMCGone.esp', [grup(b'MISC', 0, token), cell_group([placed])], 3)
quest = record(b'QUST', 0x01000810, sub(b'EDID', b'LMCAddedQuest\0') + sub(b'FULL', b'LMC Added Quest\0') +
               sub(b'DATA', struct.pack('<BBHf', 0x01, 10, 0, 0.0)))
plugin('LMCAdded.esp', [grup(b'QUST', 0, quest),
                        cell_group([record(b'REFR', added_ref, moved(added_body, 8.0), added_flags)])], 3)
plugin('LMCOrderA.esp', [cell_group([record(b'REFR', order_ref, moved(order_body, 4.0), order_flags)])], 2)
plugin('LMCOrderB.esp', [cell_group([record(b'REFR', order_ref, moved(order_body, 12.0), order_flags)])], 2)
print(f'{added_ref:08X} {order_ref:08X}')
PY
)
[[ -n "${ORDER_REF:-}" ]] || fail 'the plugin generator did not name the saloon statics'
echo "smoke[$GATE]: plugins in $WORK/mods/lmc; LMCAdded moves $ADDED_REF, LMCOrderA/B move $ORDER_REF"

# The fixture's archives, without its --esm: each leg names its own plugins.
ARCHIVES=()
for ((i = 2; i < ${#SMOKE_ENGINE_ARGS[@]}; i++)); do ARCHIVES+=("${SMOKE_ENGINE_ARGS[i]}"); done

# `debug_command <commands> <logfile>` (newline-separated commands).
debug_command() {
    { printf '%s\n' "$1"; echo .quit; } | timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" 2>&1 \
        | sed 's/\\n/\n/g' >"$2"
}
wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq -- "$pattern" "$file" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for '$pattern'"
        grep -Fq 'save load ABORTED' "$file" && fail "the load aborted: $(grep -F 'save load ABORTED' "$file" | tail -1)"
        (( SECONDS < deadline )) || fail "timed out waiting for '$pattern'"
        sleep 0.25
    done
}
require() { grep -Fq -- "$2" "$1" || fail "$3 (no '$2' in $(basename "$1"))"; pass "$3"; }
# launch <phase> <plugin>...: FalloutNV.esm then the generated plugins, the last as --esm.
launch() {
    local phase="$1" plugins=() i
    shift
    plugins=(--master "$SMOKE_DATA/$FIXTURE_ESM")
    for ((i = 1; i < $#; i++)); do plugins+=(--master "${!i}"); done
    plugins+=(--esm "${!#}")
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 BYROREDUX_SETTINGS_PATH="$WORK/settings.toml" \
        BYROREDUX_SAVE_DIR="$WORK/saves" RUST_LOG=warn,byroredux::save_io=info \
        "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${plugins[@]}" --mod-root "$WORK/mods/lmc" "${ARCHIVES[@]}" \
        --cell "$P1_CELL" --player --radius 1 --camera-pos "$P1_CAMERA_POS" --camera-forward "$P1_CAMERA_FORWARD" \
        --bench-frames 30 --bench-hold >"$WORK/$phase.stdout" 2>"$WORK/$phase.stderr" &
    engine_pid=$!
    wait_log "$WORK/$phase.stderr" 'bench-hold:'
    smoke_end_boot_conversation "$WORK/$phase.dialogue"
}
player_entity() {
    debug_command 'player.status' "$WORK/player.status"
    grep -o 'player=[0-9]*' "$WORK/player.status" | head -1 | cut -d= -f2
}
item_count() {
    debug_command 'inventory.status' "$WORK/inventory.status"
    grep -o 'item_count=[0-9]*' "$WORK/inventory.status" | head -1 | cut -d= -f2
}
# live_load <phase> <slot>: queue the load and wait until it has restored the player.
live_load() {
    local log="$WORK/$1.stderr" before
    before="$(grep -c 'save load: restored player pose' "$log" || true)"
    debug_command "load $2" "$WORK/$1.load$2"
    require "$WORK/$1.load$2" "queued load of slot $2" "$1: load $2 queued"
    local deadline=$((SECONDS + TIMEOUT))
    until (( $(grep -c 'save load: restored player pose' "$log" || true) > before )); do
        grep -Fq 'save load ABORTED' "$log" && fail "$1: load $2 aborted: $(grep -F 'save load ABORTED' "$log" | tail -1)"
        kill -0 "$engine_pid" 2>/dev/null || fail "$1: the engine exited loading slot $2"
        (( SECONDS < deadline )) || fail "$1: timed out loading slot $2"
        sleep 0.5
    done
    sleep 2
}
saved() { debug_command "save $2" "$WORK/$1.save$2"; require "$WORK/$1.save$2" "saved slot $2" "$1: slot $2 saved"; }

# 1. Save under the first profile.
launch save LMCGone.esp LMCOrderA.esp LMCOrderB.esp
PLAYER="$(player_entity)"
[[ -n "$PLAYER" ]] || fail 'player.status named no player entity'
debug_command 'cond 0x01000801 GetDisabled' "$WORK/save.placed"
require "$WORK/save.placed" 'GetDisabled(' 'save: LMCGone.esp'"'"'s placed token 01000801 is resident'
debug_command 'inv.add 0x01000800 2' "$WORK/save.inv"
require "$WORK/save.inv" 'inv.add: 2 x 01000800' 'save: two LMCGone.esp tokens given to the player'
debug_command "cond $PLAYER GetItemCount 0x01000800" "$WORK/save.count"
require "$WORK/save.count" '= 2' 'save: the player holds the two tokens'
SAVED_ITEMS="$(item_count)"
saved save 1
stop_engine

# 2. Load it into the changed profile.
launch load LMCOrderB.esp LMCOrderA.esp LMCAdded.esp
PLAYER="$(player_entity)"
live_load load 1
LOG="$WORK/load.stderr"
require "$LOG" 'save load: lmcgone.esp is not loaded: ' 'LMC-SAV-002: the removed plugin is reported'
require "$LOG" ', 1 reference(s) of it left out' 'LMC-SAV-002: its placed reference is left out'
require "$LOG" 'save load: 1 inventory stack(s) of plugins that are not loaded removed' \
    'LMC-SAV-002: its inventory stack is removed'
require "$LOG" "reference(s) lmcadded.esp edits keep their saved state, which masks the edit" \
    'LMC-SAV-003: the added plugin'"'"'s masked edit is reported'
require "$LOG" "(e.g. $ORDER_REF: lmcorderb.esp, now lmcordera.esp)" \
    'LMC-SAV-004: the reordered reference'"'"'s new winner is reported'
require "$LOG" 'Start Game Enabled quest(s) the save had not seen start now' \
    'LMC-SAV-003: the added Start Game Enabled quest is reported'
debug_command 'quest.show 0x03000810' "$WORK/load.quest"
require "$WORK/load.quest" 'state: running' 'LMC-SAV-003: LMCAddedQuest runs after the load'
debug_command 'cond 0x01000801 GetDisabled' "$WORK/load.placed"
require "$WORK/load.placed" 'no resident reference has FormID 01000801' 'LMC-SAV-002: the placed token is not resident'
LOADED_ITEMS="$(item_count)"
(( LOADED_ITEMS == SAVED_ITEMS - 2 )) || fail "the player holds $LOADED_ITEMS items, expected $((SAVED_ITEMS - 2)) (saved $SAVED_ITEMS less the two tokens)"
pass "LMC-SAV-002: the player holds $LOADED_ITEMS items, the saved $SAVED_ITEMS less the two tokens"

# 3. A save made under the changed profile loads back with nothing removed.
saved load 2
before="$(grep -c 'is not loaded: ' "$LOG" || true)"
live_load load 2
after="$(grep -c 'is not loaded: ' "$LOG" || true)"
(( after == before )) || fail 'loading the save made under the changed profile reported a removed plugin'
pass 'the changed profile'"'"'s own save loads back with no removed plugin'
stop_engine
echo "smoke[$GATE]: PASS -- removed, added and reordered plugins across a live load; artifacts: $WORK"
