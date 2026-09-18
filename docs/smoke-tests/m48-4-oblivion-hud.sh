#!/usr/bin/env bash
# M48.4 Oblivion HUD smoke — verify the MenuXml HUD loads from the vanilla
# archives, renders over the live frame, and responds to console control.
#
# Workflow (same `--bench-hold` + byro-dbg pattern as m41-equip.sh):
#   1. Spawn the engine on the Oblivion playable-slice fixture with `--hud`
#      under `--bench-frames N --bench-hold`.
#   2. Gate on the `hud: loaded` boot line (archive + texture handle).
#   3. Pipe a byro-dbg command sequence:
#        - `hud.status`      — HUD is live and reporting
#        - `hud.values ...`  — pin bar fractions
#        - `hud.heading 90`  — pin the compass
#        - `screenshot`      — capture the composited frame
#   4. Verify the screenshot carries HUD ink in the bottom strip (the bar
#      rows) and that the pinned values changed the pixel content versus a
#      full-bars capture.
#
# Pre-conditions: Oblivion installed (BYROREDUX_OBLIVION_DATA or the
# default Steam path), Vulkan device, Xvfb.
#
# Usage: docs/smoke-tests/m48-4-oblivion-hud.sh
#
# Exit: 0 on success, non-zero on any gate failure.

set -euo pipefail

DATA="${BYROREDUX_OBLIVION_DATA:-/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data}"
PORT="${BYRO_DEBUG_PORT:-9911}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

for f in "Oblivion.esm" "Oblivion - Meshes.bsa" \
    "Oblivion - Textures - Compressed.bsa" "Oblivion - Misc.bsa"; do
    [ -f "$DATA/$f" ] || { echo "FAIL: missing $DATA/$f"; exit 1; }
done

export BYRO_DEBUG_PORT=$PORT
cd "$DATA"
xvfb-run -a "$(dirname "$0")/../../target/debug/byroredux" \
    --esm "Oblivion.esm" --cell AbandonedMine \
    --bsa "Oblivion - Meshes.bsa" --textures-bsa "Oblivion - Textures - Compressed.bsa" \
    --hud --bench-frames "$BENCH_FRAMES" --bench-hold \
    > "$OUT_DIR/engine.log" 2>&1 &
ENGINE=$!

for _ in $(seq 1 60); do
    grep -q "bench-hold:" "$OUT_DIR/engine.log" && break
    kill -0 $ENGINE 2>/dev/null || { echo "FAIL: engine died"; tail -5 "$OUT_DIR/engine.log"; exit 1; }
    sleep 2
done

# Gate 1 — HUD booted from the vanilla archives.
grep -q "hud: loaded menus" "$OUT_DIR/engine.log" || { echo "FAIL: no 'hud: loaded' line"; exit 1; }
echo "PASS: hud loaded"

sleep 3
"$(dirname "$0")/../../target/debug/byro-dbg" <<CMDS | tee "$OUT_DIR/dbg.out"
hud.status
screenshot $OUT_DIR/full.png
hud.values 0.35 0.7 1.0
hud.heading 90
screenshot $OUT_DIR/pinned.png
CMDS

sleep 2
kill $ENGINE 2>/dev/null
wait $ENGINE 2>/dev/null || true

# Gate 2 — the console reports a launched HUD.
grep -q 'hud: launched visible=true' "$OUT_DIR/dbg.out" || { echo "FAIL: hud.status did not report launched"; exit 1; }
echo "PASS: hud.status"

# Gate 3 — both captures exist and the pinned values changed the HUD strip.
python3 - "$OUT_DIR" <<'PY'
import sys, zlib

def rgba(png):
    d = open(png, "rb").read()
    pos, idat = 8, b""
    while pos < len(d):
        ln = int.from_bytes(d[pos:pos+4], "big"); tag = d[pos+4:pos+8]
        if tag == b"IDAT":
            idat += d[pos+8:pos+8+ln]
        pos += 12 + ln
    return zlib.decompress(idat)

full, pinned = rgba(f"{sys.argv[1]}/full.png"), rgba(f"{sys.argv[1]}/pinned.png")
W = 1280
def ink(buf):
    return sum(1 for y in range(640, 720) for x in range(0, W)
               if buf[y*(W*4+1)+1+x*4+3] > 0)
n_full, n_pinned = ink(full), ink(pinned)
assert n_full > 1000, f"FAIL: HUD strip empty in full capture ({n_full} px)"
assert full != pinned, "FAIL: pinned values did not change the frame"
print(f"PASS: HUD ink {n_full} px; pinned frame differs")
PY

echo "M48.4 Oblivion HUD smoke: PASS"
