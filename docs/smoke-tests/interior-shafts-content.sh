#!/usr/bin/env bash
# A9 — interior day / night pair on real cells (VLM-INT-001, -002, -003; #6727).
#
# For each listed cell the script boots it held open on the free camera (`--fly`: no physics body, so the view
# does not move between captures), waits until the world is rendering, inventories every fog volume and sky
# aperture the engine built for it (VLM-INT-001), then runs the day / night procedure of VLM-INT-002 from a few
# poses, with the game clock paused and the exposure fixed so only the lighting changes. Per pose: `time.set 12`,
# a screenshot, `time.set 0`, a screenshot, `time.set 12` and a third. The two
# noon frames bracket the midnight one, so their difference is the noise of everything that changes over that
# interval except the lighting (flicker, animated props, auto-exposure, temporal accumulation).
#
# A `shaft` cell passes when, at its best pose, the pixels where the mean noon frame is brighter than midnight by
# more than max(3 levels, 2 x p99 |noon - noon repeat|) cover at least MIN_REGION_FRAC of the frame; a `sealed`
# control passes when no pose does (VLM-INT-003). A `shaft` cell's poses are `cam.tp` onto its volumes, sky
# apertures first; a `sealed` control's are `cam.tp` onto a few of the entities nearest its spawn. A pose whose
# three frames are pixel-identical is a frozen frame (a loading cover, a held menu), not a measurement: the cell
# is reported `invalid`, never as a pass.
#
# MIN_REGION_FRAC keeps a handful of hot pixels from passing: 0.1 % of a 1280x720 frame is 921 pixels.
#
# VLM-INT-004 (the same pass reached through an exterior-to-interior door) is NOT exercised here: it needs a
# door route per cell, and only the P0 fixtures have one. The script says so and counts it as not run.
#
# Usage: docs/smoke-tests/interior-shafts-content.sh [fnv|fo3|obl|sky|fo4|sf|all] [--inventory-only]
#   BYRO_INTERIOR_BIN   engine binary (default target/release/byroredux); BYRO_INTERIOR_OUT keeps captures;
#   BYRO_INTERIOR_PORT  first debug port (default 19900; each cell takes the next one);
#   BYRO_INTERIOR_ONLY  comma-separated cell ids to run instead of the whole list.
# Needs xvfb-run or a display, a Vulkan device, python3 with numpy and Pillow, and the games' data (a game whose
# data is absent is skipped). Exit 0 when every run cell passes, 1 when any fails, 77 when nothing could run.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export BYRO_ROOT="$ROOT"
export BYRO_BIN="${BYRO_INTERIOR_BIN:-$ROOT/target/release/byroredux}"
export BYRO_DBG="${BYRO_INTERIOR_DBG:-$ROOT/target/release/byro-dbg}"
export BYRO_OUT="${BYRO_INTERIOR_OUT:-$(mktemp -d /tmp/byro-interior-shafts.XXXXXX)}"
export BYRO_PORT0="${BYRO_INTERIOR_PORT:-19900}"

WHICH="${1:-all}"
INVENTORY_ONLY=0
[[ "${2:-}" == "--inventory-only" || "$WHICH" == "--inventory-only" ]] && INVENTORY_ONLY=1
[[ "$WHICH" == "--inventory-only" ]] && WHICH=all
export BYRO_WHICH="$WHICH" BYRO_INVENTORY_ONLY="$INVENTORY_ONLY"

[[ -x "$BYRO_BIN" ]] || { echo "SKIP: engine binary not found at $BYRO_BIN" >&2; exit 77; }
[[ -x "$BYRO_DBG" ]] || { echo "SKIP: byro-dbg not found at $BYRO_DBG" >&2; exit 77; }
mkdir -p "$BYRO_OUT"
if [[ -z "${DISPLAY:-}" ]]; then
    command -v xvfb-run >/dev/null || { echo "SKIP: no X display or xvfb-run" >&2; exit 77; }
    export BYRO_RUNNER=xvfb
else
    export BYRO_RUNNER=none
fi
if ss -ltn "sport = :$BYRO_PORT0" 2>/dev/null | grep -q LISTEN; then
    echo "FAIL: port $BYRO_PORT0 already occupied; refusing to attach to another session" >&2
    exit 1
fi

exec python3 -I - <<'PY'
import json, os, re, shutil, subprocess, sys, time
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(os.environ["BYRO_ROOT"])
BIN, DBG, OUT = os.environ["BYRO_BIN"], os.environ["BYRO_DBG"], Path(os.environ["BYRO_OUT"])
PORT0 = int(os.environ["BYRO_PORT0"])
WHICH = os.environ["BYRO_WHICH"]
INVENTORY_ONLY = os.environ["BYRO_INVENTORY_ONLY"] == "1"

MIN_REGION_FRAC = 0.001      # 0.1 % of the frame
MIN_THRESHOLD = 3.0          # luma levels
NOISE_FACTOR = 2.0           # VLM-INT-002: "more than twice the noise of a repeated noon capture"
TIME_SETTLE_S = 12           # the time-of-day change blends in over seconds
POSE_SETTLE_S = 2            # the free camera does not fall; this lets streaming and temporal passes settle
READY_TIMEOUT_S = 240        # the cell must be rendering (not a loading cover) before anything is measured
MAX_POSES = 4

# (game key, `--game` profile, cell, role, short name)
CELLS = [
    ("fnv", "fnv", "NellisHangar1", "shaft", "Nellis hangar (the sun-driven FNV shaft)"),
    ("fnv", "fnv", "HELIOSOneDeck", "shaft", "HELIOS One observation level"),
    ("fnv", "fnv", "RepconHQ01", "shaft", "REPCONN office main floor"),
    ("fo3", "fo3", "Tenpenny01", "shaft", "Tenpenny Tower lobby"),
    ("sky", "skyrim_se", "AbandonedShackInterior", "shaft", "Abandoned Shack (Show Sky plus sunlight lights)"),
    ("fo4", "fo4", "ConcordMuseum01", "shaft", "Museum of Freedom"),
    ("obl", "oblivion", "WarehouseCaveGeneric", "shaft", "Warehouse cave (placed light beams; AbandonedMine builds no medium)"),
    ("sf", "starfield", "CityNewAtlantisLodgeInt", "shaft", "The Lodge (New Atlantis)"),
]
SEALED = [
    # Controls: interiors the inventory shows with no fog volume and no Show Sky.
    ("fnv", "fnv", "HiddenValleyBunker1", "sealed", "Hidden Valley bunker"),
    ("fo3", "fo3", "Vault101a", "sealed", "Vault 101 entrance"),
    ("sky", "skyrim_se", "HillgrundsTomb01", "sealed", "Hillgrund's Tomb"),
    ("obl", "oblivion", "ImperialSewers03", "sealed", "Imperial sewers"),
    ("fo4", "fo4", "Vault111Cryo", "sealed", "Vault 111"),
]
CELLS += SEALED

def games():
    return sorted({c[0] for c in CELLS})

if WHICH != "all":
    CELLS = [c for c in CELLS if c[0] == WHICH]
if os.environ.get("BYRO_INTERIOR_ONLY"):  # comma-separated cell ids, for iterating on one cell
    only = set(os.environ["BYRO_INTERIOR_ONLY"].split(","))
    CELLS = [c for c in CELLS if c[2] in only]

def runner():
    return ["xvfb-run", "-a", "--server-args=-screen 0 1280x720x24"] if os.environ["BYRO_RUNNER"] == "xvfb" else []

def dbg(port, *cmds, timeout=180):
    env = dict(os.environ, BYRO_DEBUG_PORT=str(port))
    r = subprocess.run([DBG], input="\n".join(cmds) + "\nquit\n", capture_output=True, text=True,
                       cwd=ROOT, timeout=timeout, env=env)
    return r.stdout

def luma(path):
    a = np.asarray(Image.open(path).convert("RGB")).astype(float)
    return a @ np.array([0.2126, 0.7152, 0.0722])

def shot(port, name):
    dbg(port, f"screenshot {name}")
    src = ROOT / "screenshots" / name
    for _ in range(80):
        if src.exists():
            break
        time.sleep(0.25)
    else:
        raise RuntimeError(f"screenshot {name} never appeared")
    time.sleep(0.3)
    dst = OUT / name
    shutil.move(str(src), str(dst))
    return dst

def inventory(port):
    """VLM-INT-001's per-cell facts, from the running engine."""
    listing = dbg(port, "entities FogVolume")
    ids = [int(m) for m in re.findall(r"Entity (\d+) ", listing)]
    rows = []
    for chunk in range(0, len(ids), 40):
        out = dbg(port, *[f"inspect {i}" for i in ids[chunk:chunk + 40]])
        for block in re.split(r'(?=Entity \d+ ")', out):
            m = re.match(r'Entity (\d+) "([^"]*)"', block)
            if not m:
                continue
            prof = re.search(r'"profile":\s*"([^"]+)"', block)
            src = re.search(r'"source":\s*"([^"]+)"', block)
            ext = re.search(r'"extinction_per_meter":\s*([-\d.eE]+)', block)
            ctr = re.search(r'"center":\s*\[\s*([-\d.eE]+),\s*([-\d.eE]+),\s*([-\d.eE]+)', block)
            rows.append({"id": int(m.group(1)), "name": m.group(2),
                         "profile": prof.group(1) if prof else "?", "source": src.group(1) if src else "?",
                         "extinction": float(ext.group(1)) if ext else 0.0,
                         "center": [float(v) for v in ctr.groups()] if ctr else None})
    sky = dbg(port, "sky.dump")
    portal = re.search(r"portal sun dir=\[[^\]]*\] radiance=\[([-\d., ]+)\] show_sky=(\w+)", sky)
    return {"volumes": rows, "show_sky": portal.group(2) == "true" if portal else None,
            "portal_radiance": [float(v) for v in portal.group(1).split(",")] if portal else None}

def pick_targets(inv):
    vols = inv["volumes"]
    apertures = [v for v in vols if v["profile"] == "SkyAperture"]
    pool = apertures or vols
    if not pool:
        return []
    n = min(MAX_POSES, len(pool))
    return [pool[round(i * (len(pool) - 1) / max(n - 1, 1))]["id"] for i in range(n)]

def aim(port, target):
    dbg(port, f"cam.tp {target}")
    time.sleep(POSE_SETTLE_S)

def cam_where(port):
    m = re.search(r"position: \(([-\d.]+), ([-\d.]+), ([-\d.]+)\)", dbg(port, "cam.where"))
    return tuple(float(v) for v in m.groups()) if m else None

def measure(port, tag, targets):
    """Per pose, camera unmoved: noon, midnight, noon again. One metrics dict per pose."""
    res = []
    dbg(port, "time.set 12"); time.sleep(TIME_SETTLE_S)
    for k, target in enumerate(targets):
        aim(port, target)
        here = cam_where(port)
        a = luma(shot(port, f"{tag}_p{k}_noon.png"))
        dbg(port, "time.set 0"); time.sleep(TIME_SETTLE_S)
        mid = luma(shot(port, f"{tag}_p{k}_mid.png"))
        moved = cam_where(port) != here
        dbg(port, "time.set 12"); time.sleep(TIME_SETTLE_S)
        c = luma(shot(port, f"{tag}_p{k}_noon2.png"))
        noise = float(np.percentile(np.abs(a - c), 99))
        thr = max(MIN_THRESHOLD, NOISE_FACTOR * noise)
        d = (a + c) / 2 - mid
        region = d > thr
        frozen = bool(np.array_equal(a, mid) and np.array_equal(a, c))
        res.append({"pose": f"cam.tp {target}", "noise_p99": round(noise, 2), "threshold": round(thr, 2),
                    "region_frac": float(region.mean()),
                    "region_mean_delta": float(d[region].mean()) if region.any() else 0.0,
                    "max_delta": float(d.max()), "noon_mean": float(a.mean()), "mid_mean": float(mid.mean()),
                    "camera_moved": moved, "frozen": frozen})
    return res

def wait_ready(tag):
    """Block until the engine's GPU stats show the world rendering, not a loading cover."""
    path = OUT / f"{tag}.engine.log"
    t0 = time.time()
    while time.time() - t0 < READY_TIMEOUT_S:
        vals = [float(v) for v in re.findall(r"gpu\[[^\]]*\]: main_render=([\d.]+)", path.read_text(errors="replace"))[-4:]]
        if len(vals) == 4 and min(vals) > 0.5:
            return True
        time.sleep(2)
    return False

def nearby_targets(port, exclude):
    ids = [int(m) for m in re.findall(r"^\s*[\d.]+\s+(\d+)\s", dbg(port, "near 1500"), re.M)]
    ids = [i for i in ids if i not in exclude]
    if len(ids) < 3:
        return ids
    n = min(MAX_POSES, len(ids))
    return [ids[round(i * (len(ids) - 1) / (n - 1))] for i in range(n)]

def run_cell(i, game, profile, cell, role, note):
    port = PORT0 + i
    tag = f"{game}_{cell}"
    log = open(OUT / f"{tag}.engine.log", "w")
    env = dict(os.environ, BYRO_DEBUG_SERVER="1", BYRO_DEBUG_PORT=str(port), RUST_LOG="warn")
    for v in ("WAYLAND_DISPLAY", "XDG_SESSION_TYPE"):
        env.pop(v, None)
    proc = subprocess.Popen(runner() + [BIN, "--game", profile, "--cell", cell, "--bench-frames", "60",
                                        "--bench-hold", "--fly", "--window-size", "1280x720"],
                            cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    rec = {"game": game, "cell": cell, "role": role, "note": note}
    try:
        t0 = time.time()
        while "bench-hold" not in (OUT / f"{tag}.engine.log").read_text(errors="replace"):
            if proc.poll() is not None:
                rec["status"] = "boot-failed"; return rec
            if time.time() - t0 > 420:
                rec["status"] = "boot-timeout"; return rec
            time.sleep(1)
        if not wait_ready(tag):
            rec["status"] = "invalid"; rec["why"] = "the world never rendered (loading cover or stalled boot)"; return rec
        # Hold everything but the lighting still: a running game clock moves the sun and sky during a capture
        # pair, and auto-exposure re-adapts after every change; both would read as noise or as a false shaft.
        dbg(port, "time.pause", "exposure fixed 1.0")
        time.sleep(3)
        inv = inventory(port)
        by_profile = {}
        for v in inv["volumes"]:
            by_profile[v["profile"]] = by_profile.get(v["profile"], 0) + 1
        rec["inventory"] = {"volumes": len(inv["volumes"]), "by_profile": by_profile,
                            "show_sky": inv["show_sky"], "portal_radiance": inv["portal_radiance"],
                            "sources": sorted({v["source"] for v in inv["volumes"]}),
                            "extinction_max": max([v["extinction"] for v in inv["volumes"]] or [0.0])}
        if INVENTORY_ONLY:
            rec["status"] = "inventory"; return rec
        if role == "shaft":
            targets = pick_targets(inv)
            if not targets:
                rec["status"] = "FAIL"; rec["why"] = "no fog volume or sky aperture in the cell"; return rec
        else:
            targets = nearby_targets(port, {v["id"] for v in inv["volumes"]})
            if not targets:
                rec["status"] = "invalid"; rec["why"] = "no entity near the spawn to aim at"; return rec
        poses = targets
        res = measure(port, tag, poses)
        rec["poses"] = res
        best = max(res, key=lambda r: r["region_frac"])
        rec["best"] = best
        if any(r["frozen"] for r in res) or any(r["camera_moved"] for r in res):
            rec["status"] = "invalid"
            rec["why"] = "frozen frame" if any(r["frozen"] for r in res) else "the camera moved between captures"
            return rec
        if role == "shaft":
            ok = best["region_frac"] >= MIN_REGION_FRAC
        else:
            ok = all(r["region_frac"] < MIN_REGION_FRAC for r in res)
        rec["status"] = "PASS" if ok else "FAIL"
        return rec
    finally:
        try:
            dbg(port, "engine.quit", timeout=30)
        except Exception:
            pass
        time.sleep(3)
        proc.terminate()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.kill()
        subprocess.run(["pkill", "-x", "Xvfb"], check=False)

records = []
for i, (game, profile, cell, role, note) in enumerate(CELLS):
    print(f"[{game}] {cell} ({role}) ...", flush=True)
    try:
        rec = run_cell(i, game, profile, cell, role, note)
    except Exception as e:  # a cell that cannot be driven is a result, not a crash
        rec = {"game": game, "cell": cell, "role": role, "note": note, "status": "error", "why": repr(e)}
    records.append(rec)
    (OUT / "results.json").write_text(json.dumps(records, indent=1))
    print("   ", rec.get("status"), json.dumps(rec.get("best", rec.get("inventory", rec.get("why", "")))), flush=True)

print()
print("VLM-INT-001 inventory (volumes by profile; Show Sky; portal sun radiance)")
for r in records:
    inv = r.get("inventory")
    if inv:
        print(f"  {r['game']:4s} {r['cell']:26s} volumes={inv['volumes']:3d} {inv['by_profile']} show_sky={inv['show_sky']} "
              f"portal={inv['portal_radiance']} ext_max={inv['extinction_max']:.4f}")
if not INVENTORY_ONLY:
    print()
    print("VLM-INT-002 / -003 day-night (noon minus midnight, pixels above max(3, 2 x repeat p99))")
    for r in records:
        b = r.get("best")
        if b:
            print(f"  {r['status']:4s} {r['game']:4s} {r['cell']:26s} {r['role']:6s} region {100*b['region_frac']:.2f}% "
                  f"mean dL {b['region_mean_delta']:.1f} max dL {b['max_delta']:.1f} noise p99 {b['noise_p99']:.2f}  [{b['pose']}]")
        else:
            print(f"  {r.get('status','?'):4s} {r['game']:4s} {r['cell']:26s} {r['role']:6s} {r.get('why','')}")
    print()
    print("VLM-INT-004 (door transition): not exercised by this script (no door route per cell).")
    print("Pass count per game:")
    for g in games():
        run = [r for r in records if r["game"] == g]
        if run:
            print(f"  {g:4s} {sum(r.get('status')=='PASS' for r in run)} / {len(run)}")
print(f"Captures, logs and results.json: {OUT}")
if INVENTORY_ONLY:
    sys.exit(0)
ran = [r for r in records if r.get("status") not in ("boot-failed", "boot-timeout")]
if not ran:
    sys.exit(77)
sys.exit(0 if all(r.get("status") == "PASS" for r in ran) else 1)
PY
