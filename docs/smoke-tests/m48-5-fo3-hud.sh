#!/usr/bin/env bash
# M48.5 Fallout 3 HUD smoke — verify the game-agnostic MenuXml HUD loads
# the FO3 corpus (Fallout - Misc.bsa + Fallout - Textures.bsa), assembles
# the meters + compass the source engine assembled in C++, and responds
# to console control.
#
# Workflow (same `--bench-hold` + byro-dbg pattern as m48-4-oblivion-hud.sh):
#   1. Spawn the engine on the FO3 fixture cell (Moriarty's Saloon — an
#      interior, so the scene behind the HUD is dark) with `--hud`.
#   2. Gate on the `hud: loaded` boot line.
#   3. byro-dbg: `hud.status`, pin the two FO3 bars (`hud.values <hp> <ap>`
#      — FO3 drives 2 bars, not Oblivion's 3), capture screenshots.
#   4. Verify the HP tick-meter band carries near-white tick ink at full
#      and measurably less after a 30% pin, and that the compass strip
#      band is inked.
#
# Pre-conditions: Fallout 3 installed (BYROREDUX_FO3_DATA or the default
# Steam path), Vulkan device, Xvfb.
#
# Usage: docs/smoke-tests/m48-5-fo3-hud.sh
#
# Exit: 0 on success, non-zero on any gate failure.

set -euo pipefail

DATA="${BYROREDUX_FO3_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data}"
PORT="${BYRO_DEBUG_PORT:-9912}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
OUT_DIR="$(mktemp -d)"
# BYRO_SMOKE_KEEP=1 keeps $OUT_DIR after the run for offline analysis.
[ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || trap 'rm -rf "$OUT_DIR"' EXIT
# Resolve repo-relative binaries before `cd "$DATA"` below.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../../target/debug"

for f in "Fallout3.esm" "Fallout - Meshes.bsa" "Fallout - Textures.bsa" \
    "Fallout - Misc.bsa"; do
    [ -f "$DATA/$f" ] || { echo "FAIL: missing $DATA/$f"; exit 1; }
done
[ -x "$BIN_DIR/byroredux" ] || { echo "FAIL: $BIN_DIR/byroredux not built"; exit 1; }

export BYRO_DEBUG_PORT=$PORT
cd "$DATA"
xvfb-run -a "$BIN_DIR/byroredux" \
    --esm "Fallout3.esm" --cell MegatonMoriartysSaloon \
    --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" \
    --hud --bench-frames "$BENCH_FRAMES" --bench-hold \
    > "$OUT_DIR/engine.log" 2>&1 &
ENGINE=$!

for _ in $(seq 1 60); do
    grep -q "bench-hold:" "$OUT_DIR/engine.log" && break
    kill -0 $ENGINE 2>/dev/null || { echo "FAIL: engine died"; tail -5 "$OUT_DIR/engine.log"; exit 1; }
    sleep 2
done

# Gate 1 — HUD booted from the FO3 vanilla archives.
grep -q "hud: loaded menus" "$OUT_DIR/engine.log" || { echo "FAIL: no 'hud: loaded' line"; exit 1; }
echo "PASS: hud loaded"

sleep 10
# Each pin gets its own byro-dbg session with a settle delay before the
# capture (the HUD render is change-driven and 33 ms rate-limited — a
# pin+capture pair piped back-to-back can race the throttle and
# photograph the PREVIOUS state; same note as m48-4). Sessions retry
# once: the FO3 fixture cell keeps streaming content well after
# bench-hold, and a session landing on a busy frame can outrun the
# server's 5 s drain timeout.
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
pin_and_shoot() {
    dbg_session "$1" || { echo "FAIL: byro-dbg session timed out twice"; exit 1; }
    sleep 1
    dbg_session "screenshot $2" || { echo "FAIL: screenshot session timed out twice"; exit 1; }
}
pin_and_shoot "hud.values 1.0 1.0" "$OUT_DIR/full.png"
pin_and_shoot "hud.values 0.3 0.9
hud.heading 90" "$OUT_DIR/pinned.png"
"$BIN_DIR/byro-dbg" <<CMDS | tee "$OUT_DIR/dbg.out"
hud.status
CMDS

sleep 2
# `kill $ENGINE` can hit the xvfb-run wrapper rather than the engine
# (smoke README's caveat) — make sure the listener on $PORT is gone too.
kill $ENGINE 2>/dev/null
wait $ENGINE 2>/dev/null || true
ORPHAN=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1)
[ -z "$ORPHAN" ] || kill "$ORPHAN" 2>/dev/null || true

# Gate 2 — the console reports a launched HUD with the FO3 bar labels.
grep -q 'hud: launched visible=true hp=' "$OUT_DIR/dbg.out" || { echo "FAIL: hud.status did not report FO3 bars"; exit 1; }
echo "PASS: hud.status"

# Gate 3 — pixel gates over the composited captures.
#
# The FO3 meters are tick rows (repeated 8x20 `hud_tick_mark.dds`, drawn
# near-white — the corpus art ships untinted), anchored bottom-left/bottom-
# right by the driver at 1280x720: HP band x 30..330, y 620..646; compass
# strip x 20..365, y 65..129. The classifier counts NEAR-WHITE columns
# (min(r,g,b) > 180 — safe over the dark interior; any constant bright
# background appears in both captures and cancels in the ratio).
#
# The PNG decode implements all five scanline filters (see m48-4's note:
# reading filtered bytes as pixels once passed gates on artifacts).
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
    return out, w, h

def white_cols(buf, w, x0, x1, y0, y1):
    """Columns in [x0,x1) with a near-white pixel in the row band."""
    n = 0
    for x in range(x0, x1):
        for y in range(y0, y1):
            i = (y*w + x) * 4
            if min(buf[i], buf[i+1], buf[i+2]) > 180:
                n += 1
                break
    return n

full, W, H = rgba(f"{sys.argv[1]}/full.png")
pinned, _, _ = rgba(f"{sys.argv[1]}/pinned.png")

# HP tick meter band (driver layout: x=30, y=h-100, width 300, height 24).
hp_full = white_cols(full, W, 30, 330, 620, 646)
hp_pinned = white_cols(pinned, W, 30, 330, 620, 646)
assert hp_full > 150, f"FAIL: full HP tick band missing ({hp_full} white cols)"
assert hp_pinned < hp_full * 0.6, \
    f"FAIL: 30% pin did not shrink the HP meter ({hp_pinned} vs {hp_full})"

# Compass strip band (template-authored x20 y65, 345x64 window).
compass = white_cols(full, W, 20, 365, 65, 129)
assert compass > 50, f"FAIL: compass strip band empty ({compass} white cols)"

print(f"PASS: HP tick cols {hp_full} -> {hp_pinned} after 30% pin; "
      f"compass {compass} cols")
PY

echo "M48.5 Fallout 3 HUD smoke: PASS"
