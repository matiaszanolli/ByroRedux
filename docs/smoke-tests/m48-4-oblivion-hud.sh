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
# BYRO_SMOKE_KEEP=1 keeps $OUT_DIR after the run for offline analysis.
[ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || trap 'rm -rf "$OUT_DIR"' EXIT
# Resolve repo-relative binaries before `cd "$DATA"` below — the engine and
# byro-dbg live in the repo's target dir, and a literal `$(dirname "$0")`
# expansion would resolve against the Oblivion Data directory instead.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../../target/debug"

for f in "Oblivion.esm" "Oblivion - Meshes.bsa" \
    "Oblivion - Textures - Compressed.bsa" "Oblivion - Misc.bsa"; do
    [ -f "$DATA/$f" ] || { echo "FAIL: missing $DATA/$f"; exit 1; }
done
[ -x "$BIN_DIR/byroredux" ] || { echo "FAIL: $BIN_DIR/byroredux not built"; exit 1; }

export BYRO_DEBUG_PORT=$PORT
cd "$DATA"
xvfb-run -a "$BIN_DIR/byroredux" \
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
# Both captures are PINNED (1.0 then 0.35). The un-pinned boot state is not
# deterministic bar geometry — the mine's actors carry live ActorValues that
# the HUD already reads (health ~= a third at spawn), so "default full bars"
# is a false assumption here.
#
# Each pin gets its own byro-dbg session with a settle delay before the
# capture: the HUD render is change-driven and rate-limited to 33 ms
# (HUD_REFRESH_INTERVAL), so a pin+capture pair piped back-to-back can
# race the throttle and photograph the PREVIOUS state.
pin_and_shoot() {
    "$BIN_DIR/byro-dbg" <<CMDS
$1
CMDS
    sleep 1
    "$BIN_DIR/byro-dbg" <<CMDS
screenshot $2
CMDS
}
pin_and_shoot "hud.values 1.0 1.0 1.0" "$OUT_DIR/full.png"
pin_and_shoot "hud.values 0.35 0.7 1.0
hud.heading 90" "$OUT_DIR/pinned.png"
"$BIN_DIR/byro-dbg" <<CMDS | tee "$OUT_DIR/dbg.out"
hud.status
CMDS

sleep 2
kill $ENGINE 2>/dev/null
wait $ENGINE 2>/dev/null || true

# Gate 2 — the console reports a launched HUD.
grep -q 'hud: launched visible=true' "$OUT_DIR/dbg.out" || { echo "FAIL: hud.status did not report launched"; exit 1; }
echo "PASS: hud.status"

# Gate 3 — both captures exist and the pinned values changed the HUD strip.
#
# The PNG decode below implements ALL five scanline filters. An earlier
# revision consumed the zlib stream raw — i.e. read *filtered* bytes as
# pixels — which made any per-pixel gate meaningless (it passed on filter
# artifacts). The gates are RGB-domain: the screenshot's alpha channel is
# not a reliable signal (the presentation pass does not preserve it), but
# the bar fill colors are: at `hud.values 0.35 …` the health trough's left
# section carries a red-dominant run that must shrink vs. the full-bars
# capture, and both must be present.
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

def red_run(buf, w):
    """Longest contiguous red-dominant column run in the health-fill rows.

    The trough's permanent red-brown ornamentation defeats raw counts (973
    of 1455 red pixels survive a 100%->35% pin), but the solid fill is the
    only long contiguous run: ~106 px at 35%, ~3x that at full bars
    (measured against the MenuXml CPU dump at 1280x720).
    """
    cols = []
    for x in range(250, 560):
        hit = False
        for y in range(666, 673):
            i = (y*w + x) * 4
            r, g, b = buf[i], buf[i+1], buf[i+2]
            if r > 120 and r > g + 30 and r > b + 20:
                hit = True
                break
        cols.append(hit)
    best = cur = gap = 0
    for v in cols:
        if v:
            cur += 1; gap = 0
            best = max(best, cur)
        else:
            gap += 1
            if gap > 2:
                cur = 0
    return best

# Health trough: bottom-left cluster (verified against the MenuXml CPU dump —
# fill sits around x 266..560, rows 650..690 at 1280x720).
full, W, H = rgba(f"{sys.argv[1]}/full.png")
pinned, _, _ = rgba(f"{sys.argv[1]}/pinned.png")
run_full = red_run(full, W)
run_pinned = red_run(pinned, W)
assert run_full > 200, f"FAIL: full-bars health fill missing ({run_full} px run)"
assert 30 < run_pinned < run_full * 0.75, \
    f"FAIL: pinned 35% fill not a shrunken run ({run_pinned} vs {run_full})"
print(f"PASS: health fill run {run_full} -> {run_pinned} px after 35% pin")
PY

echo "M48.4 Oblivion HUD smoke: PASS"
