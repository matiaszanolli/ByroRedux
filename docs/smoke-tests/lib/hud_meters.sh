#!/usr/bin/env bash
# Shared body of the M48 Phase 2 HUD-meter gates (#6661): m48-skyrim-hud-meters.sh (A4) and
# m48-fo4-hud-meters.sh (A5). The caller sets GATE, DATA, PORT, ENGINE_ARGS (array), FULL / LOW
# (`hud.values` lines), FEED_TOKEN (a `hud.debug` path that must read `=found`) and BANDS (a
# Python literal: {"name": (x0, x1, y0, y1, "moves"|"still")}), then sources this file.
#
# Same-run differencing: two captures of one held frame, one at FULL and one at LOW. The world
# cancels; a "moves" band (a meter whose pinned value changed) must change, a "still" band (HUD
# chrome whose value did not) must not.

set -euo pipefail
LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$LIB_DIR/../../../target/debug"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
OUT_DIR="$(mktemp -d)"
[ "${BYRO_SMOKE_KEEP:-0}" = "1" ] || trap 'rm -rf "$OUT_DIR"' EXIT
[ -x "$BIN_DIR/byroredux" ] || { echo "FAIL: $BIN_DIR/byroredux not built"; exit 1; }
[ -x "$BIN_DIR/byro-dbg" ] || { echo "FAIL: $BIN_DIR/byro-dbg not built"; exit 1; }

export BYRO_DEBUG_PORT=$PORT
export BYRO_DEBUG_SERVER=1
cd "$DATA"
xvfb-run -a "$BIN_DIR/byroredux" "${ENGINE_ARGS[@]}" --hud --bench-frames "$BENCH_FRAMES" --bench-hold \
    > "$OUT_DIR/engine.log" 2>&1 &
ENGINE=$!
stop_engine() {
    # `kill $ENGINE` can hit the xvfb-run wrapper rather than the engine.
    kill $ENGINE 2>/dev/null || true
    wait $ENGINE 2>/dev/null || true
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [ -z "$orphan" ] || kill "$orphan" 2>/dev/null || true
}
fail() {
    echo "FAIL[$GATE]: $*"
    stop_engine
    exit 1
}

for _ in $(seq 1 90); do
    grep -q "bench-hold:" "$OUT_DIR/engine.log" && break
    kill -0 $ENGINE 2>/dev/null || { echo "FAIL[$GATE]: engine died"; tail -5 "$OUT_DIR/engine.log"; exit 1; }
    sleep 2
done
grep -q "hud: loaded interface.hudmenu.swf .* backend=scaleform" "$OUT_DIR/engine.log" \
    || fail "no 'hud: loaded ... backend=scaleform' line"
sleep 8

# Sessions retry once: the cell keeps streaming past bench-hold (m48-6's note).
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
# A start-up message box dims the HUD strip (#6609): answer whatever is waiting.
answer_message_boxes() {
    local out
    for _ in 1 2 3 4 5 6; do
        out="$(dbg_session "ui.answer" 2>&1 || true)"
        grep -Fq 'no message box is waiting' <<<"$out" && return 0
        sleep 0.5
    done
}
# The debug server's `screenshot` takes a bare filename and writes under the engine cwd's
# screenshots/ (63c0aee3b / #5142).
shoot() {
    answer_message_boxes
    local name="$1" shot="$DATA/screenshots/$1"
    rm -f "$shot"
    dbg_session "screenshot $name" > /dev/null || fail "screenshot session timed out twice"
    for _ in 1 2 3 4 5; do [[ -s "$shot" ]] && break; sleep 2; done
    [[ -s "$shot" ]] || fail "$name never landed in $DATA/screenshots/"
    mv "$shot" "$OUT_DIR/$name"
}

answer_message_boxes
dbg_session "$FULL" > /dev/null || fail "hud.values session timed out twice"
sleep 2
shoot full.png
dbg_session "$LOW" > /dev/null || fail "hud.values session timed out twice"
sleep 2
shoot low.png
dbg_session "hud.debug" > "$OUT_DIR/debug.out" 2>&1 || fail "hud.debug session timed out twice"
grep -Fq "${FEED_TOKEN}=found" "$OUT_DIR/debug.out" \
    || fail "the path service did not land the meter feed: $(grep -o 'paths (queued[^"]*' "$OUT_DIR/debug.out" | head -1)"
echo "PASS[$GATE]: the meter feed landed (${FEED_TOKEN}=found)"
stop_engine

python3 - "$OUT_DIR" "$LIB_DIR" "$GATE" "$BANDS" <<'PY'
import ast, sys
sys.path.insert(0, sys.argv[2])
from pngdiff import load, changed
out, gate, bands = sys.argv[1], sys.argv[3], ast.literal_eval(sys.argv[4])
full, width, _ = load(f"{out}/full.png")
low, _, _ = load(f"{out}/low.png")
counts = {name: changed(full, low, width, x0, x1, y0, y1) for name, (x0, x1, y0, y1, _) in bands.items()}
print(f"{gate}: changed pixels per band {counts}")
for name, (*_, expect) in bands.items():
    if expect == "moves":
        assert counts[name] > 0, f"FAIL[{gate}]: the {name} band did not change ({counts[name]} px)"
    else:
        assert counts[name] == 0, f"FAIL[{gate}]: the control {name} band changed ({counts[name]} px)"
print(f"PASS[{gate}]: the pinned meters moved and the control band held")
PY
echo "$GATE: PASS"
