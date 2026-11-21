#!/usr/bin/env python3
"""A12 (M-LIGHT RLT-IBL-006): does a GI path that leaves a Show Sky interior
read the portal sky?

A Show Sky interior (default FO4 `DBTechHighSchool01`, a Wave 1 title) is booted held
open on the free camera at its spawn pose with two engines: a control without
RLT-IBL-006 and the build under test. Through the debug server the exposure is
pinned (`exposure fixed 1.0`; the auto meter would pull every frame to the
same mean), and each of `indirect_only` and `traced_share` is shot at noon,
midnight and noon again (`time.set`), several frames averaged per shot. The
two noon shots bound the frame-to-frame noise.

The control's escaped GI reads the authored ambient, which does not follow the
hour; with RLT-IBL-006 an escaping path reads the portal sky cube, which does.
The portal sun lights path hits in both engines, so noon minus midnight moves
in the control too. The column that isolates RLT-IBL-006 is head minus
control at the same hour. Values are the mean display level (0–255) of the
presented frame under the pinned exposure. A report: it passes when
published (PRD §7.2).

Measured 2026-11-21, two cells cannot discriminate: FNV `NellisHangar1`
(bounded apertures) has window meshes in its openings, so no GI ray misses the
TLAS and both engines render the same frame, and Skyrim SE
`AbandonedShackInterior` (Show Sky) has closed walls, so head and control read
the same within the noise. FNV has no Show Sky interior
(docs/engine/interior-shaft-openings.md).

    portal_sky.py --control BIN --head BIN [--dbg BIN] [--out DIR]
                  [--game fo4] [--cell DBTechHighSchool01] [--shots 4]
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import uuid
from pathlib import Path

import numpy as np
import mitsuba as mi

ROOT = Path(__file__).resolve().parents[2]
LUMA = np.array([0.2126, 0.7152, 0.0722])
PORT = int(os.environ.get("BYRO_PORTAL_SKY_PORT", "19983"))
SETTLE_S = 5.0
VIEWS = ("indirect_only", "traced_share")
HOURS = (("noon", 12), ("midnight", 0), ("noon_repeat", 12))


def dbg(dbg_bin: Path, *cmds: str, timeout: int = 180) -> str:
    env = dict(os.environ, BYRO_DEBUG_PORT=str(PORT))
    out = subprocess.run([str(dbg_bin)], input="\n".join(cmds) + "\nquit\n", capture_output=True, text=True,
                         cwd=ROOT, timeout=timeout, env=env)
    return out.stdout.replace("\\n", "\n")


def wait_ready(dbg_bin: Path, timeout_s: float = 300.0) -> int:
    last, same, t0 = -1, 0, time.time()
    while time.time() - t0 < timeout_s:
        n = len(re.findall(r"Entity \d+ ", dbg(dbg_bin, "entities")))
        same = same + 1 if (n == last and n > 100) else 0
        if same >= 2:
            return n
        last = n
        time.sleep(3)
    raise SystemExit("portal-sky: the cell never finished loading")


def shot(dbg_bin: Path, out: Path, name: str) -> float:
    source = f"portal_{uuid.uuid4().hex}_{name}.png"
    dbg(dbg_bin, f"screenshot {source}")
    src = ROOT / "screenshots" / source
    for _ in range(80):
        if src.exists():
            break
        time.sleep(0.25)
    else:
        raise SystemExit(f"portal-sky: screenshot {name} never appeared")
    time.sleep(0.3)
    dst = out / f"{name}.png"
    shutil.move(str(src), str(dst))
    image = np.array(mi.Bitmap(str(dst)), dtype=np.float64)[..., :3]
    return float((image @ LUMA).mean())


def measure(binary: Path, dbg_bin: Path, game: str, cell: str, shots: int, out: Path,
            label: str) -> dict[tuple[str, str], float]:
    out.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, BYRO_DEBUG_SERVER="1", BYRO_DEBUG_PORT=str(PORT), RUST_LOG="warn",
               BYROREDUX_SETTINGS_PATH=str(out / f"{label}.settings.toml"))
    log_path = out / f"{label}.engine.log"
    with open(log_path, "w") as log:
        proc = subprocess.Popen([str(binary), "--game", game, "--cell", cell, "--bench-frames", "60",
                                 "--bench-hold", "--fly", "--window-size", "1280x720",
                                 # Tier 3: the adaptive budget can settle on tier 0, which traces no
                                 # GI at all, and then no path escapes to read either sky.
                                 "--rt-test-ray-quality-tier", "3"],
                                cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        t0 = time.time()
        while "bench-hold" not in log_path.read_text(errors="replace"):
            if proc.poll() is not None or time.time() - t0 > 420:
                raise SystemExit(f"portal-sky: {label} engine did not reach bench-hold, see {log_path}")
            time.sleep(1)
        wait_ready(dbg_bin)
        sky = dbg(dbg_bin, "sky.dump")
        if "show_sky=true" not in sky:
            raise SystemExit(f"portal-sky: {cell} is not a Show Sky interior (sky.dump: show_sky=false)")
        dbg(dbg_bin, "exposure fixed 1.0")
        levels: dict[tuple[str, str], float] = {}
        for view in VIEWS:
            dbg(dbg_bin, f"render.debug {view}")
            for hour_name, hour in HOURS:
                dbg(dbg_bin, f"time.set {hour}")
                time.sleep(SETTLE_S)
                levels[(view, hour_name)] = float(np.mean(
                    [shot(dbg_bin, out, f"{label}_{view}_{hour_name}_{k}") for k in range(shots)]))
        return levels
    finally:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=60)


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    parser.add_argument("--control", type=Path, required=True)
    parser.add_argument("--head", type=Path, required=True)
    parser.add_argument("--dbg", type=Path, default=ROOT / "target" / "release" / "byro-dbg")
    parser.add_argument("--game", default="fo4")
    parser.add_argument("--cell", default="DBTechHighSchool01")
    parser.add_argument("--shots", type=int, default=4)
    parser.add_argument("--out", type=Path, default=ROOT / "target" / "m-light-portal-sky")
    args = parser.parse_args()
    mi.set_variant("scalar_rgb")
    out = args.out.resolve()
    runs = {label: measure(binary.resolve(), args.dbg.resolve(), args.game, args.cell, args.shots, out, label)
            for label, binary in (("control", args.control), ("head", args.head))}
    report = out / "portal-sky.tsv"
    with open(report, "w") as f:
        f.write("engine\tgame\tcell\tview\tnoon\tmidnight\tnoon_repeat\tnoon_minus_midnight\t"
                "noon_repeat_noise\thead_minus_control_noon\thead_minus_control_midnight\n")
        for label, levels in runs.items():
            for view in VIEWS:
                noon, night, again = (levels[(view, h)] for h, _ in HOURS)
                d_noon = levels[(view, "noon")] - runs["control"][(view, "noon")]
                d_night = levels[(view, "midnight")] - runs["control"][(view, "midnight")]
                f.write(f"{label}\t{args.game}\t{args.cell}\t{view}\t{noon:.3f}\t{night:.3f}\t{again:.3f}\t"
                        f"{noon - night:+.3f}\t{abs(noon - again):.3f}\t{d_noon:+.3f}\t{d_night:+.3f}\n")
    print(report.read_text(), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
