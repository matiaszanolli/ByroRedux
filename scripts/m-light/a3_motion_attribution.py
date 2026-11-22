#!/usr/bin/env python3
"""Compare A3 direct-light motion with and without temporal reuse.

Both conditions use their own parked reference and same-age held control at
one pan frame. This is diagnostic evidence, not an acceptance gate.

    python -I -B scripts/m-light/a3_motion_attribution.py --profile release
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
import gates as g  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=REPO / "target" / "m-light-a3-motion")
    parser.add_argument("--profile", choices=("dev", "release"), default="release")
    parser.add_argument("--frame", type=int, default=112)
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()
    if not 0 <= args.frame < g.TEMPORAL_MOTION:
        parser.error("--frame must lie inside the pan")
    g.pick_variant()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    cargo = ["cargo", "build", "-p", "byroredux", "--bin", "byroredux"]
    if args.profile == "release":
        cargo.append("--release")
    subprocess.run(cargo, cwd=REPO, check=True)
    binary = REPO / "target" / ("release" if args.profile == "release" else "debug") / "byroredux"
    engine = g.Engine(binary, args.out, dict(os.environ), 20, 64)
    extra = ["--bench-camera-length", str(g.TEMPORAL_MOTION), "--upscaler", "taa", "--bench-camera", "pan"]
    rows = []
    for label, flags in (("normal", "0"), ("no_temporal", "0x40000")):
        common = dict(view_mode="direct_only", seed=args.seed, debug_flags=flags)
        parked = g.load_exr(engine.capture(
            f"{label}_parked_f{args.frame}", "penumbra", "direct_only", args.seed, 3,
            extra=extra + ["--bench-camera-freeze-at", str(args.frame)],
            bench_frames=args.frame + g.PARKED_SETTLE, bench_mode="renderer-stepped", debug_flags=flags))
        moving = engine.sequence(f"{label}_moving", "penumbra", 3, [args.frame], args.frame + 4,
                                 extra, **common)
        held = engine.sequence(f"{label}_held", "penumbra", 3, [args.frame], args.frame + 4,
                               extra + ["--bench-camera-hold-at", str(args.frame)], **common)
        mask = g.erode(g.luminance(parked) > 1e-4, g.ERODE_PX)
        moving_error = g.temporal_error(g.load_exr(moving[args.frame][0]), parked, mask)
        held_error = g.temporal_error(g.load_exr(held[args.frame][0]), parked, mask)
        rows.append((label, flags, moving_error, held_error, moving_error - held_error))

    revision = g.git("rev-parse", "--short=12", "HEAD") + ("-dirty" if g.git("status", "--porcelain") else "")
    lines = ["condition\tdebug_flags\tframe\tseed\tmoving_error\theld_error\texcess\trevision"]
    lines += [f"{label}\t{flags}\t{args.frame}\t{args.seed}\t{moving:.6g}\t{held:.6g}\t{excess:.6g}\t{revision}"
              for label, flags, moving, held, excess in rows]
    result = "\n".join(lines) + "\n"
    (args.out / "attribution.tsv").write_text(result)
    print(result, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
