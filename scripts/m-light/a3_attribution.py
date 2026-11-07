#!/usr/bin/env python3
"""A3 attribution (docs/prd/reference-lighting.md rev 29, #6603): where does the
A3 pan's post-stop "excess over the hold control" come from?

A3 judges TAA's resolved `final` against the engine parked and converged at the
stop pose. This tool re-measures the same pan, post-stop, three ways so the
excess can be attributed to an accumulator instead of to "motion":

  final-moving   the A3 pan on `final` (post-TAA tap), sampled to +512 frames
  final-held     a run held at the stop pose from frame 0 (`--bench-camera-hold-at`),
                 sampled at early frame numbers: the error of a purely parked run
                 that has only N frames of history
  direct-moving  the same pan on `direct_only` (the in-shader shadow-ratio EMA,
                 no SVGF / TAA / GI)
  direct-held    the held run on `direct_only`

Every row compares one single-frame capture against the same converged parked
capture of the same view at the stop pose (`--bench-camera-freeze-at`, 256 frames
of settle). During-motion frames are NOT sampled: a parked capture at the stop
pose is the wrong reference for an earlier pose. One engine seed, one capture per
row: the seed-pair noise of these rows is not measured.

    <mitsuba-venv>/bin/python -I -B scripts/m-light/a3_attribution.py [--out DIR] [--tsv FILE]
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "m-light"))
import gates as g  # noqa: E402  (a sibling script, not a package)

STOP = g.TEMPORAL_MOTION - 1
POST_STOP = (1, 4, 16, 64, 256, 512)
HELD_FRAMES = (8, 16, 24, 32, 48, 64, 96, 135, 200)
DIRECT_POST_STOP = (1, 4, 16, 64)
DIRECT_HELD_FRAMES = (16, 32, 64, 96, 135, 183)


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=REPO / "target" / "m-light-a3-attribution")
    parser.add_argument("--tsv", type=Path, default=None, help="write the rows here")
    parser.add_argument("--profile", default="release", choices=("dev", "release"))
    args = parser.parse_args()

    cargo = ["cargo", "build", "-p", "byroredux", "--bin", "byroredux"]
    if args.profile == "release":
        cargo.append("--release")
    subprocess.run(cargo, cwd=REPO, check=True)
    binary = REPO / "target" / ("release" if args.profile == "release" else "debug") / "byroredux"
    args.out.mkdir(parents=True, exist_ok=True)
    engine = g.Engine(binary, args.out, dict(os.environ), 20, 64)

    pan = ["--bench-camera-length", str(g.TEMPORAL_MOTION), "--upscaler", "taa", "--bench-camera", "pan"]
    # `direct_only` bypasses TAA, so only `final` taps the post-TAA output.
    taps = {"final": ["--linear-capture-tap", "post-taa"], "direct_only": []}
    rows: list[tuple[str, str, int, float]] = []

    for view, label, post_stop, held_frames in (
        ("final", "final", POST_STOP, HELD_FRAMES),
        ("direct_only", "direct", DIRECT_POST_STOP, DIRECT_HELD_FRAMES),
    ):
        extra = pan + taps[view]
        parked = g.load_exr(engine.capture(
            f"attr_{label}_parked", "penumbra", view, 0, 3,
            extra=extra + ["--bench-camera-freeze-at", str(STOP)],
            bench_frames=STOP + g.PARKED_SETTLE, bench_mode="renderer-stepped"))
        mask = g.erode(g.luminance(parked) > 1e-4, g.ERODE_PX)

        frames = [STOP + k for k in post_stop]
        moving = engine.sequence(f"attr_{label}_moving", "penumbra", 3, frames, frames[-1] + 4,
                                 extra, view_mode=view)
        for k, f in zip(post_stop, frames):
            rows.append((f"{label}-moving", f"post_stop_{k}", f,
                         g.temporal_error(g.load_exr(moving[f][0]), parked, mask)))

        held = engine.sequence(f"attr_{label}_held", "penumbra", 3, list(held_frames),
                               held_frames[-1] + 4, extra + ["--bench-camera-hold-at", str(STOP)],
                               view_mode=view)
        for f in held_frames:
            rows.append((f"{label}-held", f"frame_{f}", f,
                         g.temporal_error(g.load_exr(held[f][0]), parked, mask)))

    revision = g.git("rev-parse", "--short=12", "HEAD") + ("-dirty" if g.git("status", "--porcelain") else "")
    lines = ["case\tregion\tframe\ttemporal_error\ttier\tseed\trevision"]
    lines += [f"{c}\t{r}\t{f}\t{v:.5f}\t3\t0\t{revision}" for c, r, f, v in rows]
    text = "\n".join(lines) + "\n"
    if args.tsv:
        args.tsv.write_text(text)
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
