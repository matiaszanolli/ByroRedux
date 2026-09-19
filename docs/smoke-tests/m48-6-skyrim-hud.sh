#!/usr/bin/env bash
# M48.6 Skyrim HUD smoke — verify the `--hud` Scaleform route loads the
# vanilla `hudmenu.swf` out of `Skyrim - Interface.bsa` over a live cell,
# keeps world input alive, answers the movie's host protocol, and reports
# through the shared `hud.*` console with `backend=scaleform`.
#
# Scope note (what this gate honestly claims): vanilla hudmenu is passive
# chrome — its bytecode calls exactly four host methods (pinned by
# `crates/ui/tests/hudmenu_protocol.rs`) and its ExternalInterface surface
# is the GameDelegate pair `call`/`respond`. The meters are fed by GFx
# object-path invocation in the vanilla engine, which Ruffle's surface
# does not expose, so this gate asserts the compass chrome renders over
# the live frame and the driver/protocol/console work — NOT bar movement.
#
# Workflow (same `--bench-hold` + byro-dbg pattern as m48-4/m48-5):
#   1. Spawn the engine on the BleakFallsBarrow01 fixture cell with
#      `--hud` (no --menu) under `--bench-frames N --bench-hold`.
#   2. Gate on the `hud: loaded ... backend=scaleform` boot line.
#   3. byro-dbg: `hud.status` (backend + bar pins), `hud.debug`
#      (registered callbacks must be the GameDelegate pair), pins,
#      screenshot.
#   4. Pixel gate: the compass chrome band (top-center) carries bright
#      ink; a control band at the same height does not.
#
# Pre-conditions: Skyrim SE installed (BYROREDUX_SKYRIM_DATA /
# BYROREDUX_SKYRIMSE_DATA or the default path), Vulkan device, Xvfb.
#
# Usage: docs/smoke-tests/m48-6-skyrim-hud.sh
#
# Exit: 0 on success, non-zero on any gate failure.

set -euo pipefail

DATA="${BYROREDUX_SKYRIM_DATA:-${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}}"
PORT="${BYRO_DEBUG_PORT:-9913}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
OUT_DIR="$(mktemp -d)"
[ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || trap 'rm -rf "$OUT_DIR"' EXIT
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../../target/debug"

for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa" \
    "Skyrim - Interface.bsa"; do
    [ -f "$DATA/$f" ] || { echo "FAIL: missing $DATA/$f"; exit 1; }
done
[ -x "$BIN_DIR/byroredux" ] || { echo "FAIL: $BIN_DIR/byroredux not built"; exit 1; }

export BYRO_DEBUG_PORT=$PORT
cd "$DATA"
xvfb-run -a "$BIN_DIR/byroredux" \
    --esm "Skyrim.esm" --cell BleakFallsBarrow01 \
    --bsa "Skyrim - Meshes0.bsa" --textures-bsa "Skyrim - Textures0.bsa" \
    --hud --bench-frames "$BENCH_FRAMES" --bench-hold \
    > "$OUT_DIR/engine.log" 2>&1 &
ENGINE=$!

for _ in $(seq 1 60); do
    grep -q "bench-hold:" "$OUT_DIR/engine.log" && break
    kill -0 $ENGINE 2>/dev/null || { echo "FAIL: engine died"; tail -5 "$OUT_DIR/engine.log"; exit 1; }
    sleep 2
done

# Gate 1 — the Scaleform HUD booted from the Interface BSA.
grep -q "hud: loaded interface.hudmenu.swf .* backend=scaleform" "$OUT_DIR/engine.log" \
    || { echo "FAIL: no 'hud: loaded ... backend=scaleform' line"; exit 1; }
echo "PASS: hud loaded"

sleep 10
# Sessions retry once: the cell keeps streaming past bench-hold and a
# session landing on a busy frame can outrun the server's 5s drain
# timeout (same note as m48-5).
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

# Gate 2 — console reports the Scaleform backend and accepts the game's
# three bar pins (health/magicka/stamina — Skyrim's labels).
dbg_session "hud.values 0.3 0.5 0.9
hud.heading 90" || { echo "FAIL: byro-dbg session timed out twice"; exit 1; }
sleep 1
dbg_session "hud.status" | tee "$OUT_DIR/dbg.out" >/dev/null
grep -q 'backend=scaleform' "$OUT_DIR/dbg.out" \
    || { echo "FAIL: hud.status did not report backend=scaleform"; exit 1; }
grep -q 'visible=true health=0.30 (pinned) magicka=0.50 (pinned) stamina=0.90 (pinned)' \
    "$OUT_DIR/dbg.out" \
    || { echo "FAIL: hud.status did not apply the Skyrim bar pins"; exit 1; }
echo "PASS: hud.status"

# Gate 3 — the bridge diagnostics: the movie registered the GameDelegate
# pair, and the driver tick loop is alive (last_push carries the pins).
sleep 2
dbg_session "hud.debug" | tee "$OUT_DIR/dbg2.out" >/dev/null
grep -q 'callbacks: call, respond' "$OUT_DIR/dbg2.out" \
    || { echo "FAIL: hudmenu did not register the GameDelegate callbacks"; exit 1; }
grep -q 'last_push: health=0.30 magicka=0.50 stamina=0.90 heading=90.0' "$OUT_DIR/dbg2.out" \
    || { echo "FAIL: the Scaleform HUD driver is not ticking"; exit 1; }
echo "PASS: hud.debug"

sleep 1
# Gate 4 — chrome on/off diff: capture with the HUD live, then `hud.off`
# (the driver mirrors HudControl.visible into the player, whose render
# answers `UiFrame::Hidden` — the UI quad stops) and capture again. The
# compass band must lose its chrome between the two captures. Differencing
# the same run makes the gate immune to world brightness: the stage
# clears transparent (M48.6 fix), so the world legitimately shows through
# and varies between runs.
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

def bright(buf, w, x0, x1, y0, y1, thr):
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            i = (y*w + x) * 4
            if min(buf[i], buf[i+1], buf[i+2]) > thr:
                n += 1
    return n

on, W, H = rgba(f"{sys.argv[1]}/hud_on.png")
off, _, _ = rgba(f"{sys.argv[1]}/hud_off.png")

def diff_count(x0, x1, y0, y1, eps):
    """Pixels whose RGB changed beyond eps between the two captures.

    The world renders in both captures (static camera, frozen fixture
    cell), so world pixels cancel; the HUD's contribution is what
    remains. This is immune to world brightness, which a fixed
    threshold is not — the barrow's upper region legitimately carries
    thousands of bright world pixels.
    """
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            a = (y*W + x) * 4
            d = abs(on[a]-off[a]) + abs(on[a+1]-off[a+1]) + abs(on[a+2]-off[a+2])
            if d > eps:
                n += 1
    return n

chrome = diff_count(455, 825, 55, 90, 60)
control = diff_count(200, 570, 300, 335, 60)
assert chrome > 300, f"FAIL: compass chrome did not draw ({chrome} changed px)"
assert control < chrome // 3, \
    f"FAIL: world too animated between captures ({control} vs {chrome} changed px)"
print(f"PASS: compass chrome {chrome} changed px (control {control})")
PY

echo "M48.6 Skyrim HUD smoke: PASS (chrome + protocol gates)"
