#!/usr/bin/env bash
# A9 — interior day / night pair on real cells (VLM-INT-001, -002, -003; #6727).
#
# For each listed cell the script boots it held open on the free camera (`--fly`: no physics body, so the view
# does not move between captures), waits until the cell's entities are in, inventories every fog volume and sky
# aperture the engine built for it (VLM-INT-001), then runs the day / night procedure of VLM-INT-002 from a few
# poses, with the game clock paused and the exposure fixed so only the lighting changes. Per pose: `time.set 12`,
# a screenshot, `time.set 0`, a screenshot, `time.set 12` and a third. The two noon frames bracket the midnight
# one, so their difference is the noise of everything that changes over that interval except the lighting
# (light flicker, animated props, temporal accumulation). It is not small: fixture lights flicker, so a pose with
# a flickering light in view needs a correspondingly larger day / night difference to count.
#
# A `shaft` cell passes when, at its best pose, the pixels where the mean noon frame is brighter than midnight by
# more than max(3 levels, 2 x p99 |noon - noon repeat|) cover at least MIN_REGION_FRAC of the frame; a `sealed`
# control passes when no pose does (VLM-INT-003). A `gap` cell is a known open gap (an authored sun with no medium
# to carry it): it is measured and reported as GAP-OPEN or GAP-CLOSED and never fails the run. The poses are
# `cam.tp` onto the cell's volumes: marked sky apertures first, then light-shaft volumes, then any (a `sealed`
# control with no volume at all uses the entities nearest its spawn). A pose whose
# three frames are identical and not empty is a frozen frame (a held menu), not a measurement: the cell is
# reported `invalid`, never as a pass. The cell must also show a loaded world (a stable, large entity count).
#
# The frames are the froxel in-scatter radiance view (`render.debug volumetric_radiance`), not the finished
# frame: a window or roof hole shows the sky, which differs between noon and midnight whether or not a shaft
# exists, and a lit surface differs too. The in-scatter view holds only the medium, so only a shaft can count.
#
# MIN_REGION_FRAC keeps a handful of hot pixels from passing: 0.1 % of a 1280x720 frame is 921 pixels.
#
# Reading a FAIL: it means no sampled pose (at most MAX_POSES per cell) showed a sun-driven change in the medium,
# not that none exists. For a cell whose volumes are all fixture-beam cones beside local lights (the inventory
# line shows only `Homogeneous` volumes, Show Sky off) that is the authored content, not an engine fault: those
# beams have no sun path to follow (docs/engine/interior-shaft-openings.md). A cell with no volume at all fails
# its shaft run by construction.
#
# VLM-INT-004 (the same pass reached through an exterior-to-interior door) is `BYRO_INTERIOR_ROUTE=door`: after the
# direct boot the run takes the cell's own exit door out (`door.teleport`, the console frontend of the E-key
# interaction path) and then, from the exterior arrival, the partner door back in, so the cell the day / night pass
# measures was reached by a real exterior-to-interior transition (the exterior's streaming torn down and the
# interior loaded over it), not by `--cell`. The free camera spawns on the door it came through and arrives on the
# partner door; `door.list` names the loaded doors with the cell each leads to, so the way back in is the nearest door
# whose destination is the cell under test (not whichever one the camera ray hits: two hangars' doors can stand
# side by side), with no walking. A cell whose exit door does not lead to
# an exterior (the Institute, entered by a teleporter) is reported `no-route`, not failed.
#
# The cell list is the sun-path census's (see CELLS); Starfield has no sun-capable or passive volume in any of its
# 11,985 interiors, so it has no entry.
#
# Usage: docs/smoke-tests/interior-shafts-content.sh [fnv|fo3|obl|sky|fo4|all] [--inventory-only]
#   BYRO_INTERIOR_ROUTE direct (default: `--cell` boot) or door (out through the exit door and back, VLM-INT-004);
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
ROUTE = os.environ.get("BYRO_INTERIOR_ROUTE", "direct")
if ROUTE not in ("direct", "door"):
    sys.exit(f"BYRO_INTERIOR_ROUTE must be direct or door, not {ROUTE!r}")

MIN_REGION_FRAC = 0.001      # 0.1 % of the frame
MIN_THRESHOLD = 3.0          # luma levels
NOISE_FACTOR = 2.0           # VLM-INT-002: "more than twice the noise of a repeated noon capture"
TIME_SETTLE_S = 12           # the time-of-day change blends in over seconds
POSE_SETTLE_S = 2            # the free camera does not fall; this lets streaming and temporal passes settle
READY_TIMEOUT_S = 300        # the cell must be loaded (not a loading cover) before anything is measured
MIN_ENTITIES = 150           # a cell with fewer entities than this is not a loaded interior
MAX_POSES = 4
TRANSITION_TIMEOUT_S = 300   # a door's destination (an exterior grid, then the interior again) must load in this time
DOOR_WAIT_S = 300            # how long the exterior streaming has to load the door that leads back in (about 1 fps for FO4 under xvfb)
DOOR_SETTLE_S = 30           # let the exterior's loading cover lift and its streaming settle before the second door

# (game key, `--game` profile, cell, role, short name). The list comes from the sun-path census
# (`interior_sun_path_census`, docs/engine/interior-shaft-openings.md). A `shaft` cell is one the census predicts
# can show a sun-driven shaft: it has a marked SkyAperture plane, or LightShaft volumes in a Show Sky cell (an
# opening the sun can cross). A `sealed` control has no such path: passive fixture beams, no medium, or LightShaft
# volumes with Show Sky off and no marked aperture, which measured as sun-independent. A `gap` cell has an
# authored sun and no volume to carry it. FO3, Skyrim and Starfield have no `shaft` cell in the census.
CELLS = [
    ("fnv", "fnv", "NellisHangar1", "shaft", "Nellis hangar: 32 SkyAperture planes"),
    ("fnv", "fnv", "NellisHangar1b", "shaft", "Nellis hangar (b): 32 SkyAperture planes"),
    ("fnv", "fnv", "NellisHangar2", "shaft", "Nellis mess hall and munitions: 32 SkyAperture planes"),
    ("fo4", "fo4", "ConcordMuseum01", "shaft", "Museum of Freedom: 10 LightShaft, Show Sky"),
    ("fo4", "fo4", "DBTechHighSchool01", "shaft", "DB Technical High School: 76 LightShaft, Show Sky"),
    ("fo4", "fo4", "TrinityTower01", "shaft", "Trinity Tower: 12 LightShaft, Show Sky"),
    ("fo4", "fo4", "zInstituteRotunda", "shaft", "Institute rotunda: 6 LightShaft, Show Sky"),
    ("fo4", "fo4", "CambridgePD01", "shaft", "Cambridge police station: 6 LightShaft, Show Sky"),
    ("fo4", "fo4", "UniversityPoint01", "shaft", "University Point: 4 LightShaft, Show Sky"),
    ("sky", "skyrim_se", "AbandonedShackInterior", "gap", "Abandoned Shack: Show Sky, 4 sunlight LIGH, no medium"),
]
SEALED = [
    # Controls: no sun path in the census.
    ("fnv", "fnv", "RocketLabTop", "sealed", "REPCONN research labs: 60 LightShaft, Show Sky off, no aperture"),
    ("fnv", "fnv", "NCRPrisonBlockB", "sealed", "NCR prison block B: 30 LightShaft, Show Sky off, no aperture"),
    ("fnv", "fnv", "2EOVault21GiftShop", "sealed", "Vault 21 gift shop: 25 LightShaft, Show Sky off, no aperture"),
    ("obl", "oblivion", "AnvilCastleSmugglersCave", "sealed", "Smuggler's cave: 2 LightShaft, Show Sky off"),
    ("obl", "oblivion", "TestCave01", "sealed", "Caverns (test cell): 7 LightShaft, Show Sky off"),
    ("fnv", "fnv", "HELIOSOneDeck", "sealed", "HELIOS One observation level: passive cones only"),
    ("fnv", "fnv", "HiddenValleyBunker1", "sealed", "Hidden Valley bunker: no medium"),
    ("fo3", "fo3", "Tenpenny01", "sealed", "Tenpenny Tower lobby: passive cones only"),
    ("sky", "skyrim_se", "HillgrundsTomb01", "sealed", "Hillgrund's Tomb"),
    ("obl", "oblivion", "WarehouseCaveGeneric", "sealed", "Warehouse cave: passive cones only"),
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
    return r.stdout.replace("\\n", "\n")  # some replies arrive as one quoted line with literal \n

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
    # Aim at what can follow the sun first: marked apertures, then light-shaft volumes, then anything.
    apertures = [v for v in vols if v["profile"] == "SkyAperture"]
    shafts = [v for v in vols if v["profile"] == "LightShaft"]
    pool = apertures or shafts or vols
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
        frozen = bool(np.array_equal(a, mid) and np.array_equal(a, c) and a.mean() > 1.0)
        res.append({"pose": f"cam.tp {target}", "noise_p99": round(noise, 2), "threshold": round(thr, 2),
                    "region_frac": float(region.mean()),
                    "region_mean_delta": float(d[region].mean()) if region.any() else 0.0,
                    "max_delta": float(d.max()), "noon_mean": float(a.mean()), "mid_mean": float(mid.mean()),
                    "camera_moved": moved, "frozen": frozen})
    return res

def wait_ready(port):
    """Block until the cell's entities are in: a large, stable `entities` total (a loading cover has none)."""
    last, same = -1, 0
    t0 = time.time()
    while time.time() - t0 < READY_TIMEOUT_S:
        n = len(re.findall(r"Entity \d+ ", dbg(port, "entities")))
        same = same + 1 if (n == last and n >= MIN_ENTITIES) else 0
        if same >= 2:
            return True
        last = n
        time.sleep(3)
    return False

def wait_stable(port):
    """Streaming can outlast the first rendered frames: wait for the fog-volume count to stop changing."""
    last, same = -1, 0
    for _ in range(40):
        n = len(re.findall(r"Entity \d+ ", dbg(port, "entities FogVolume")))
        same = same + 1 if n == last else 0
        if same >= 2:
            return
        last = n
        time.sleep(3)

def nearby_targets(port, exclude):
    ids = [int(m) for m in re.findall(r"^\s*[\d.]+\s+(\d+)\s", dbg(port, "near 1500"), re.M)]
    ids = [i for i in ids if i not in exclude]
    if len(ids) < 3:
        return ids
    n = min(MAX_POSES, len(ids))
    return [ids[round(i * (len(ids) - 1) / (n - 1))] for i in range(n)]

def applied(logfile):
    return logfile.read_text(errors="replace").count("Cell transition applied")

DOOR_RE = re.compile(r"^\s*([\d.]+)\s+door\s+(\d+)\s+->\s+[0-9A-Fa-f]+\s+(.*)$", re.M)

def doors(port):
    """Every loaded teleport door near the camera as (distance, entity, destination label), nearest first
    (`door.list`, read-only). Chosen by destination rather than by the camera ray: two doors can stand side by
    side (the Nellis hangars' exterior doors are 112 units from each other) and the ray takes whichever it hits."""
    # byro-dbg prints a reply inside quotes, so the closing one lands on the last row's label.
    return [(float(d), int(e), lab.strip().rstrip('"')) for d, e, lab in DOOR_RE.findall(dbg(port, "door.list 4000"))]

def through_door(port, logfile, want=None):
    """`door.teleport` a door and wait for the transition to be applied: the nearest one, or with `want` the nearest
    whose destination is the interior `want`. Returns (destination, why): the destination label the console
    resolved, or None and the reason."""
    # An exterior streams its doors in after the transition lands, so the one asked for is polled for.
    deadline = time.time() + (DOOR_WAIT_S if want else 0)
    while True:
        found = doors(port)
        if want:
            found = [d for d in found if d[2].lower() == f"interior '{want.lower()}'"]
        if found or time.time() >= deadline:
            break
        time.sleep(5)
    if not found:
        if not want:
            return None, "no door in reach"
        seen = ", ".join(sorted({d[2] for d in doors(port)})) or "none"
        return None, f"no door in reach leads to {want} (doors loaded: {seen})"
    door = found[0][1]
    before = applied(logfile)
    out = dbg(port, f"door.teleport {door}")
    dest = re.search(r"destination cell: ([^\n]*)", out)
    if "queued" not in out or not dest:
        return None, f"door {door} did not queue a transition: {out.strip()[:160]}"
    t0 = time.time()
    while applied(logfile) <= before:
        if time.time() - t0 > TRANSITION_TIMEOUT_S:
            return None, f"transition through door {door} to {dest.group(1)} never applied"
        time.sleep(1)
    return dest.group(1).strip(), None

def door_round_trip(port, logfile, cell):
    """VLM-INT-004: out through the interior's exit door, then back in through the partner door on the exterior
    side. Returns (record fields, None) when the interior was reached by an exterior-to-interior transition."""
    out_dest, why = through_door(port, logfile)
    if out_dest is None:
        # No door to take is a property of the cell; a transition that never landed is a failed run.
        return None, ("no-route" if why == "no door in reach" else "invalid", why)
    if not out_dest.startswith("exterior"):
        return None, ("no-route", f"the exit door leads to {out_dest}, not an exterior")
    # The transition is already confirmed applied; an exterior is not asked for a stable entity count (streaming,
    # ground cover and actors keep it moving, so `wait_ready` never settles there), only given time to load.
    time.sleep(DOOR_SETTLE_S)
    back_dest, why = through_door(port, logfile, want=cell)
    if back_dest is None:
        # The exterior's loaded doors were searched for one that leads to this cell: none does within reach.
        return None, ("no-route", f"no way back in: {why}")
    if cell.lower() not in back_dest.lower():
        return None, ("invalid", f"the way back led to {back_dest}, not {cell}")
    time.sleep(DOOR_SETTLE_S)
    return {"out_via": out_dest, "back_via": back_dest}, None

def run_cell(i, game, profile, cell, role, note):
    port = PORT0 + i
    tag = f"{game}_{cell}" + ("_door" if ROUTE == "door" else "")
    log = open(OUT / f"{tag}.engine.log", "w")
    env = dict(os.environ, BYRO_DEBUG_SERVER="1", BYRO_DEBUG_PORT=str(port),
               RUST_LOG="warn,byroredux::cell_loader::transition=info,byroredux::app_step=info")  # app_step logs "Cell transition applied"
    for v in ("WAYLAND_DISPLAY", "XDG_SESSION_TYPE"):
        env.pop(v, None)
    proc = subprocess.Popen(runner() + [BIN, "--game", profile, "--cell", cell, "--bench-frames", "60",
                                        "--bench-hold", "--fly", "--window-size", "1280x720"],
                            cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    rec = {"game": game, "cell": cell, "role": role, "note": note, "route": ROUTE}
    try:
        t0 = time.time()
        while "bench-hold" not in (OUT / f"{tag}.engine.log").read_text(errors="replace"):
            if proc.poll() is not None:
                rec["status"] = "boot-failed"; return rec
            if time.time() - t0 > 420:
                rec["status"] = "boot-timeout"; return rec
            time.sleep(1)
        if not wait_ready(port):
            rec["status"] = "invalid"; rec["why"] = "the world never rendered (loading cover or stalled boot)"; return rec
        if ROUTE == "door":
            fields, bad = door_round_trip(port, OUT / f"{tag}.engine.log", cell)
            if bad:
                rec["status"], rec["why"] = bad
                return rec
            rec.update(fields)
            if not wait_ready(port):
                rec["status"] = "invalid"; rec["why"] = "the interior never rendered after the door transition"; return rec
        # Hold everything but the lighting still: a running game clock moves the sun and sky during a capture
        # pair, and auto-exposure re-adapts after every change; both would read as noise or as a false shaft.
        dbg(port, "time.pause", "exposure fixed 1.0")
        wait_stable(port)
        inv = inventory(port)
        # Measure the in-scatter view, not the finished frame: a window or roof hole shows the sky, and the sky
        # changes between noon and midnight whether or not any shaft exists. The radiance view holds only the
        # froxel in-scatter (no opacity floor), so a lit surface or a visible sky can never count as a shaft.
        dbg(port, "render.debug volumetric_radiance")
        by_profile = {}
        for v in inv["volumes"]:
            by_profile[v["profile"]] = by_profile.get(v["profile"], 0) + 1
        rec["inventory"] = {"volumes": len(inv["volumes"]), "by_profile": by_profile,
                            "show_sky": inv["show_sky"], "portal_radiance": inv["portal_radiance"],
                            "sources": sorted({v["source"] for v in inv["volumes"]}),
                            "extinction_max": max([v["extinction"] for v in inv["volumes"]] or [0.0])}
        if INVENTORY_ONLY:
            rec["status"] = "inventory"; return rec
        if role in ("shaft", "gap"):
            targets = pick_targets(inv)
            if not targets:
                rec["status"] = "GAP-OPEN" if role == "gap" else "FAIL"
                rec["why"] = "no fog volume or sky aperture in the cell"; return rec
        else:
            # A control with media is aimed at its own media (a claim that they do not follow the sun has to
            # look at them); one with none is aimed at the entities nearest its spawn.
            targets = pick_targets(inv) or nearby_targets(port, set())
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
        if role in ("shaft", "gap"):
            ok = best["region_frac"] >= MIN_REGION_FRAC
        else:
            ok = all(r["region_frac"] < MIN_REGION_FRAC for r in res)
        if role == "gap":
            # A known gap: reported, never failing the run. It closes when the engine gives the sun a medium.
            rec["status"] = "GAP-CLOSED" if ok else "GAP-OPEN"
        else:
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
    if ROUTE == "door":
        print("VLM-INT-004 (door transition): every result above was reached out through the cell's exit door and back "
              "in through the exterior door; `no-route` is a cell whose exit door does not lead to an exterior.")
    else:
        print("VLM-INT-004 (door transition): not exercised on this route; run with BYRO_INTERIOR_ROUTE=door.")
    print("Pass count per game:")
    for g in games():
        run = [r for r in records if r["game"] == g]
        if run:
            counted = [r for r in run if r["role"] != "gap" and r.get("status") != "no-route"]
            gaps = [r for r in run if r["role"] == "gap"]
            extra = f"  (+{len(gaps)} known gap: {', '.join(r['cell'] + ' ' + r['status'] for r in gaps)})" if gaps else ""
            if counted or gaps:
                print(f"  {g:4s} {sum(r.get('status')=='PASS' for r in counted)} / {len(counted)}{extra}")
print(f"Captures, logs and results.json: {OUT}")
if INVENTORY_ONLY:
    sys.exit(0)
ran = [r for r in records if r.get("status") not in ("boot-failed", "boot-timeout")]
if not ran:
    sys.exit(77)
counted = [r for r in ran if r["role"] != "gap" and r.get("status") != "no-route"]
sys.exit(0 if all(r.get("status") == "PASS" for r in counted) else 1)
PY
