#!/usr/bin/env bash
# M48 Phase 1 A3 (#6653) — the menu registry and stack on Skyrim SE: the vanilla HUD movie stays
# open under a native modal and a second movie, input goes to the topmost menu whose flags claim
# it and back to the next owner when that menu closes, and every open movie renders through one
# wgpu device (GUI-REG-003/005/007/009, GUI-OPS-001/005).
#
# Workflow (the `--bench-hold` + byro-dbg pattern of m48-6):
#   1. Spawn the engine on BleakFallsBarrow01 with `--hud` (vanilla `hudmenu.swf` from
#      `Skyrim - Interface.bsa`).
#   2. `ui.menus` after each step:
#        boot                         -> HUD Menu open (movie), input_owner=world,         devices=1
#        input.press inventory        -> + InventoryMenu (native), input_owner=InventoryMenu
#        ui.open MessageBoxMenu       -> + MessageBoxMenu (movie messagebox.swf), owner MessageBoxMenu
#        ui.close MessageBoxMenu cancel -> MessageBoxMenu gone, input back to InventoryMenu
#      The HUD is open on every line and the device count stays 1.
#
# Pre-conditions: Skyrim SE installed (BYROREDUX_SKYRIMSE_DATA or the default path), a Vulkan
# device, Xvfb, a debug build (`cargo build`, which also builds byro-dbg).
#
# Usage: docs/smoke-tests/m48-menu-stack.sh
#
# Exit: 0 PASS, 77 SKIP (game data absent), non-zero FAIL.

set -euo pipefail

DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9914}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
OUT_DIR="$(mktemp -d)"
[ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || trap 'rm -rf "$OUT_DIR"' EXIT
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../../target/debug"

for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa" "Skyrim - Interface.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m48-menu-stack]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done
[ -x "$BIN_DIR/byroredux" ] || { echo "FAIL: $BIN_DIR/byroredux not built"; exit 1; }
[ -x "$BIN_DIR/byro-dbg" ] || { echo "FAIL: $BIN_DIR/byro-dbg not built"; exit 1; }

export BYRO_DEBUG_PORT=$PORT
export BYRO_DEBUG_SERVER=1
cd "$DATA"
xvfb-run -a "$BIN_DIR/byroredux" \
    --esm "Skyrim.esm" --cell BleakFallsBarrow01 \
    --bsa "Skyrim - Meshes0.bsa" --textures-bsa "Skyrim - Textures0.bsa" \
    --hud --bench-frames "$BENCH_FRAMES" --bench-hold \
    > "$OUT_DIR/engine.log" 2>&1 &
ENGINE=$!
cleanup_engine() {
    # `kill $ENGINE` can hit the xvfb-run wrapper rather than the engine — make sure the listener
    # on $PORT is gone too.
    kill $ENGINE 2>/dev/null || true
    wait $ENGINE 2>/dev/null || true
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [ -z "$orphan" ] || kill "$orphan" 2>/dev/null || true
}
fail() {
    echo "FAIL: $*"
    cleanup_engine
    exit 1
}

for _ in $(seq 1 60); do
    grep -q "bench-hold:" "$OUT_DIR/engine.log" && break
    kill -0 $ENGINE 2>/dev/null || { echo "FAIL: engine died"; tail -5 "$OUT_DIR/engine.log"; exit 1; }
    sleep 2
done
grep -q "hud: loaded interface.hudmenu.swf .* backend=scaleform" "$OUT_DIR/engine.log" \
    || fail "no 'hud: loaded ... backend=scaleform' line"
sleep 5

# Sessions retry once: the cell keeps streaming past bench-hold (same note as m48-6).
dbg_session() {
    for _ in 1 2; do
        if "$BIN_DIR/byro-dbg" <<CMDS
$1
CMDS
        then
            return 0
        fi
        sleep 3
    done
    return 1
}

# A start-up message box would sit on the stack and own input; answer whatever is waiting
# (`ui.answer`, the m48-6 / lib/fixture.sh pattern) so the stack starts from the HUD alone.
for _ in 1 2 3 4 5 6; do
    out="$(dbg_session "ui.answer" 2>&1 || true)"
    grep -Fq 'no message box is waiting' <<<"$out" && break
    sleep 0.5
done

# `ui.menus` after a step: the summary line and one line per open menu, bottom to top.
menus() {
    local step="$1"
    sleep 1
    dbg_session "ui.menus" > "$OUT_DIR/$step.out" 2>&1 || fail "$step: ui.menus session timed out twice"
    cat "$OUT_DIR/$step.out"
}
expect() {
    local step="$1" pattern="$2" what="$3"
    grep -Eq "$pattern" "$OUT_DIR/$step.out" || fail "$step: $what (pattern: $pattern)"
}
reject() {
    local step="$1" pattern="$2" what="$3"
    if grep -Eq "$pattern" "$OUT_DIR/$step.out"; then fail "$step: $what"; fi
}
common() {
    local step="$1" owner="$2"
    expect "$step" "ui\.menus: game=skyrim .*input_owner=$owner .*devices=1" "input owner $owner and one device"
    expect "$step" "HUD Menu code=- surface=movie interface.+hudmenu\.swf" "the HUD movie is open"
}

menus boot
common boot world
reject boot "InventoryMenu|MessageBoxMenu" "a modal is open at boot"
echo "PASS: boot — HUD open, input with the world, devices=1"

dbg_session "input.press inventory" > "$OUT_DIR/press.out" 2>&1 || fail "input.press session timed out twice"
menus inventory
common inventory InventoryMenu
expect inventory "InventoryMenu code=- surface=native Inventory" "the native inventory page is open"
expect inventory "paused=yes" "the inventory pauses the game"
echo "PASS: input.press inventory — HUD under the native inventory, which owns input"

dbg_session "ui.open MessageBoxMenu" > "$OUT_DIR/open.out" 2>&1 || fail "ui.open session timed out twice"
menus messagebox
common messagebox MessageBoxMenu
expect messagebox "MessageBoxMenu code=- surface=movie interface.+messagebox\.swf" "the message box movie is open"
expect messagebox "InventoryMenu" "the inventory stays open under the message box"
echo "PASS: ui.open MessageBoxMenu — a second movie on top owns input, still one device"

dbg_session "ui.close MessageBoxMenu cancel" > "$OUT_DIR/close.out" 2>&1 || fail "ui.close session timed out twice"
menus closed
common closed InventoryMenu
reject closed "MessageBoxMenu" "the message box is still open"
grep -q "ui.close: MessageBoxMenu closed result=cancel" "$OUT_DIR/engine.log" \
    || fail "closed: the engine did not log the cancel result"
echo "PASS: ui.close MessageBoxMenu cancel — input back to the inventory in the same frame"

cleanup_engine
echo "M48 Phase 1 menu-stack smoke: PASS"
