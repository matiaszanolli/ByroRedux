#!/usr/bin/env bash
# M48.7 Fallout 4 HUD smoke — verify the `--hud` Scaleform route serves
# the AVM2 game: vanilla `hudmenu.swf` out of `Fallout4 - Interface.ba2`
# loads over a live cell with the injected BGSCodeObj adapter, keeps
# world input alive, and reports through the shared `hud.*` console with
# FO4's two-bar labels and `backend=scaleform`.
#
# Scope note (same honesty as m48-6): FO4's hudmenu renders its chrome
# (health/rads bar, AP bar, compass, crosshair, icons) statically at boot
# and makes zero host calls while idle — the meters are fed by GFx
# object-path invocation in the vanilla engine, which Ruffle's surface
# cannot reach, so this gate asserts chrome + protocol, NOT bar movement.
#
# Workflow (same `--bench-hold` + byro-dbg pattern as m48-4/5/6):
#   1. Spawn the engine on the MedTekResearch01 fixture cell with `--hud`.
#   2. Gate on the `hud: loaded ... state=Some(AdapterInjected)` boot line.
#   3. byro-dbg: `hud.status` (2-bar pins), `hud.debug` (the adapter's
#      `__byro*` lifecycle callbacks = runtime AdapterInjected proof),
#      `hud.values` pins, screenshot, `hud.off`, screenshot.
#   4. Pixel gate: chrome on/off RGB diff over the health-bar and compass
#      bands (world pixels cancel — the scene is static between captures).
#
# Pre-conditions: Fallout 4 installed (BYROREDUX_FO4_DATA or the default
# path), Vulkan device, Xvfb.
#
# Usage: docs/smoke-tests/m48-7-fo4-hud.sh
#
# Exit: 0 on success, non-zero on any gate failure.

set -euo pipefail

DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
PORT="${BYRO_DEBUG_PORT:-9916}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
OUT_DIR="$(mktemp -d)"
[ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || trap 'rm -rf "$OUT_DIR"' EXIT
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../../target/debug"

for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - MeshesExtra.ba2" \
    "Fallout4 - Textures1.ba2" "Fallout4 - Materials.ba2" \
    "Fallout4 - Interface.ba2"; do
    [ -f "$DATA/$f" ] || { echo "FAIL: missing $DATA/$f"; exit 1; }
done
[ -x "$BIN_DIR/byroredux" ] || { echo "FAIL: $BIN_DIR/byroredux not built"; exit 1; }

export BYRO_DEBUG_PORT=$PORT
cd "$DATA"
xvfb-run -a "$BIN_DIR/byroredux" \
    --esm "Fallout4.esm" --cell MedTekResearch01 \
    --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2" \
    --textures-bsa "Fallout4 - Textures1.ba2" --materials-ba2 "Fallout4 - Materials.ba2" \
    --hud --bench-frames "$BENCH_FRAMES" --bench-hold \
    > "$OUT_DIR/engine.log" 2>&1 &
ENGINE=$!

for _ in $(seq 1 60); do
    grep -q "bench-hold:" "$OUT_DIR/engine.log" && break
    kill -0 $ENGINE 2>/dev/null || { echo "FAIL: engine died"; tail -5 "$OUT_DIR/engine.log"; exit 1; }
    sleep 2
done

# Gate 1 — the Scaleform HUD booted from the FO4 interface archive with
# the BGSCodeObj adapter injected.
grep -q "hud: loaded interface.hudmenu.swf .* backend=scaleform .* state=Some(AdapterInjected)" \
    "$OUT_DIR/engine.log" \
    || { echo "FAIL: no 'hud: loaded ... state=Some(AdapterInjected)' line"; exit 1; }
echo "PASS: hud loaded"

sleep 12
dbg_session() {
    local cmds="$1"
    for _ in 1 2; do
        if "$BIN_DIR/byro-dbg" <<CMDS
$cmds
CMDS
        then
            return 0
        fi
        sleep 3
    done
    return 1
}

# Gate 2 — console reports the Scaleform backend with FO4's two bars.
dbg_session "hud.values 0.3 0.9
hud.heading 90" || { echo "FAIL: byro-dbg session timed out twice"; exit 1; }
sleep 1
dbg_session "hud.status" | tee "$OUT_DIR/dbg.out" >/dev/null
grep -q 'backend=scaleform' "$OUT_DIR/dbg.out" \
    || { echo "FAIL: hud.status did not report backend=scaleform"; exit 1; }
grep -qE 'visible=true health=0\.30 \(pinned\) ap=0\.90 \(pinned\)' "$OUT_DIR/dbg.out" \
    || { echo "FAIL: hud.status did not apply the FO4 bar pins"; exit 1; }
echo "PASS: hud.status"

# Gate 3 — bridge diagnostics: the injected adapter's lifecycle hooks are
# registered (runtime AdapterInjected proof), and the driver tick loop is
# alive with FO4's labels.
sleep 2
dbg_session "hud.debug" | tee "$OUT_DIR/dbg2.out" >/dev/null
grep -q '__byroBGSCodeObjReady' "$OUT_DIR/dbg2.out" \
    || { echo "FAIL: the BGSCodeObj adapter lifecycle hook is not registered"; exit 1; }
grep -q 'last_push: health=0.30 ap=0.90 heading=90.0' "$OUT_DIR/dbg2.out" \
    || { echo "FAIL: the Scaleform HUD driver is not ticking"; exit 1; }
echo "PASS: hud.debug"

# Gate 4 — chrome on/off diff: capture with the HUD live, then `hud.off`
# (the driver mirrors HudControl.visible into the player; the UI quad
# stops) and capture again. World pixels cancel — the fixture cell is
# static between captures.
dbg_session "screenshot $OUT_DIR/hud_on.png" \
    || { echo "FAIL: screenshot session timed out twice"; exit 1; }
dbg_session "hud.off" || { echo "FAIL: hud.off session timed out twice"; exit 1; }
sleep 1
dbg_session "screenshot $OUT_DIR/hud_off.png" \
    || { echo "FAIL: second screenshot session timed out twice"; exit 1; }

sleep 2
# `kill $ENGINE` can hit the xvfb-run wrapper rather than the engine —
# make sure the listener on $PORT is gone too.
kill $ENGINE 2>/dev/null
wait $ENGINE 2>/dev/null || true
ORPHAN=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1)
[ -z "$ORPHAN" ] || kill "$ORPHAN" 2>/dev/null || true

python3 - "$OUT_DIR" <<'PY'
import sys, zlib, struct

def rgba(png):
    d = open(png, "rb").read()
    pos, idat = 8, b""
    w = h = 0
    while pos < len(d):
        ln = int.from_bytes(d[pos:pos+4], "big"); tag = d[pos+4:pos+8]
        if tag == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", d[pos+8:pos+18])
            assert (depth, ctype) == (8, 6), f"expected 8-bit RGBA, got {depth}/{ctype}"
        if tag == b"IDAT":
            idat += d[pos+8:pos+8+ln]
        pos += 12 + ln
    raw = zlib.decompress(idat)
    stride = w * 4
    out = bytearray(h * stride)
    prev = bytearray(stride)
    def paeth(a, b, c):
        p = a + b - c
        pa, pb, pc = abs(p-a), abs(p-b), abs(p-c)
        return a if pa <= pb and pa <= pc else (b if pb <= pc else c)
    for y in range(h):
        f = raw[y*(stride+1)]
        row = bytearray(raw[y*(stride+1)+1:(y+1)*(stride+1)])
        if f == 1:
            for i in range(4, stride): row[i] = (row[i] + row[i-4]) & 0xff
        elif f == 2:
            for i in range(stride): row[i] = (row[i] + prev[i]) & 0xff
        elif f == 3:
            for i in range(stride):
                a = row[i-4] if i >= 4 else 0
                row[i] = (row[i] + ((a + prev[i]) >> 1)) & 0xff
        elif f == 4:
            for i in range(stride):
                a = row[i-4] if i >= 4 else 0
                c = prev[i-4] if i >= 4 else 0
                row[i] = (row[i] + paeth(a, prev[i], c)) & 0xff
        out[y*stride:(y+1)*stride] = row
        prev = row
    # Presentation pass does not preserve alpha — judge RGB only.
    return out, w, h

on, W, H = rgba(f"{sys.argv[1]}/hud_on.png")
off, _, _ = rgba(f"{sys.argv[1]}/hud_off.png")

def diff_count(x0, x1, y0, y1, eps):
    """Pixels whose RGB changed beyond eps between the two captures."""
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            a = (y*W + x) * 4
            d = abs(on[a]-off[a]) + abs(on[a+1]-off[a+1]) + abs(on[a+2]-off[a+2])
            if d > eps:
                n += 1
    return n

# FO4 HUD chrome bands (calibrated against the first M48.7 run):
# health/rads bar bottom-left, compass + markers top-center.
health = diff_count(85, 360, 645, 665, 60)
compass = diff_count(500, 780, 28, 75, 60)
control = diff_count(400, 480, 250, 300, 60)
assert health > 200, f"FAIL: health bar chrome did not draw ({health} changed px)"
assert compass > 100, f"FAIL: compass chrome did not draw ({compass} changed px)"
assert control < (health + compass) // 8, \
    f"FAIL: world too animated between captures ({control} vs {health}+{compass})"
print(f"PASS: FO4 chrome diff health={health} compass={compass} control={control}")
PY

echo "M48.7 Fallout 4 HUD smoke: PASS (chrome + protocol gates)"
