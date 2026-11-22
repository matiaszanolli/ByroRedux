"""A13 attribution: where a pinned-tier frame-time increase goes, pass by pass and build by build.

The phase-exit A13 (`baselines.py --exit-cost`) reports one wall-time delta per
scene and config. When a case exceeds its Q8 allowance, this runner repeats the
same pinned-tier, renderer-stepped orbit runs for two or more engines,
interleaved run by run, and reports the median of every GPU timer bracket the
bench line prints (`gpu_<pass>=<ms>`) beside the wall time. With the phase
entry first and intermediate builds after it, the increase is assigned to
passes and then to the commits between builds.

Usage (from the repo root, with the Mitsuba Python the baselines use):
    python3 -I -B scripts/m-light/a13_attribution.py \
        --binary entry=<entry worktree>/target/release/byroredux \
        --binary head=target/release/byroredux \
        --cases prospector:taa:0,dugout:taa:0 --runs 3 --out target/m-light-a13-attribution

Writes <out>/attribution.tsv: one row per (scene, config, pass) with each
build's median and its delta from the first build, the wall row first.
"""

from __future__ import annotations

import argparse
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import baselines  # noqa: E402  (Engine, CONFIGS, bench_fields, gpu_brackets)


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    parser.add_argument("--binary", action="append", required=True, metavar="LABEL=PATH",
                        help="an engine to time; repeat it, the first is the reference the deltas are taken from")
    parser.add_argument("--cases", required=True,
                        help="comma-separated scene:config:pinned_tier, e.g. prospector:taa:0")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--frames", type=int, default=300)
    parser.add_argument("--min-delta", type=float, default=0.02,
                        help="print a pass only when some build moves it by at least this many ms")
    parser.add_argument("--out", type=Path, default=baselines.REPO / "target" / "m-light-a13-attribution")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    engines = []
    for spec in args.binary:
        label, _, path = spec.partition("=")
        if not path:
            raise SystemExit(f"a13_attribution: --binary takes LABEL=PATH, got {spec!r}")
        engines.append((label, baselines.Engine(Path(path).resolve(), out / label)))
    if len(engines) < 2:
        raise SystemExit("a13_attribution: give at least two --binary engines")
    labels = [label for label, _ in engines]
    configs = dict(baselines.CONFIGS)
    stepped = ["--bench-frames", str(args.frames), "--bench-mode", "renderer-stepped", "--bench-camera", "orbit"]

    rows = []
    for case in [c.strip() for c in args.cases.split(",") if c.strip()]:
        scene, config, tier = case.split(":")
        pinned = configs[config] + stepped + ["--rt-test-ray-quality-tier", tier]
        samples = {label: [] for label in labels}
        for run in range(1, args.runs + 1):
            for label, engine in engines:
                line, _ = engine.run(scene, f"{config}_t{tier}_{run}", pinned)
                gpu = baselines.gpu_brackets(line)
                gpu["wall"] = float(baselines.bench_fields(line)["wall_ms"])
                samples[label].append(gpu)
                baselines.log(f"{scene}/{config} {label} run {run}: wall {gpu['wall']:.2f} ms")
        passes = sorted(set().union(*(s.keys() for runs in samples.values() for s in runs)) - {"wall"})
        for name in ["wall", *passes]:
            medians = [statistics.median(s.get(name, 0.0) for s in samples[label]) for label in labels]
            rows.append([scene, config, tier, name, *(f"{m:.3f}" for m in medians),
                         *(f"{m - medians[0]:+.3f}" for m in medians[1:])])

    with open(out / "attribution.tsv", "w") as f:
        f.write("\t".join(["scene", "config", "pinned_tier", "pass", *(f"{l}_ms" for l in labels),
                           *(f"{l}_delta_ms" for l in labels[1:])]) + "\n")
        for row in rows:
            f.write("\t".join(row) + "\n")
    print("\t".join(["scene", "config", "tier", "pass", *labels]))
    for row in rows:
        deltas = [abs(float(d)) for d in row[4 + len(labels):]]
        if row[3] == "wall" or max(deltas) >= args.min_delta:
            print("\t".join(row[:4 + len(labels)]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
