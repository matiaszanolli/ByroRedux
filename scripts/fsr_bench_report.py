#!/usr/bin/env python3
"""Summarize the FSR benchmark matrix produced by fsr-bench-matrix.sh.

Reports two quantities the plan is explicit about not conflating:

  render-work recovered = native(render-resolution passes)
                        - preset(render-resolution passes)
  net frame recovery    = native end-to-end frame time
                        - preset end-to-end frame time

The first is the gross saving from shading fewer pixels. The second is what a
player actually gets, after paying for the upscale dispatch and for the
output-resolution work no preset shrinks. Quoting only the first would
overstate the win, which is precisely the substitution
docs/engine/fsr3-upscaler-integration-plan.md §1.7 rules out.

Per-run values are summarized as median with full range, matching the
bench-of-record convention. Median rather than mean because a single
scheduler hiccup on a shared desktop skews a 3-run mean badly, and the
existing ROADMAP numbers were gathered the same way.
"""

import sys
from collections import defaultdict

# Passes that run at render resolution and therefore shrink with the preset.
# `presentation` and `upscale` are deliberately excluded: the first runs at
# output resolution, the second is the cost of the preset itself.
RENDER_RES_PASSES = [
    "gpu_main",
    "gpu_svgf",
    "gpu_composite",
    "gpu_ssao",
    "gpu_volumetrics",
    "gpu_bloom",
]

REFERENCE = "taa"

# Preset display order — ascending upscale ratio, so a reader scans the table
# top-to-bottom as "more upscaling".
ORDER = ["taa", "fsr-native-aa", "fsr-quality", "fsr-balanced", "fsr-performance"]


def median(values):
    values = sorted(values)
    n = len(values)
    if n == 0:
        return 0.0
    mid = n // 2
    return values[mid] if n % 2 else (values[mid - 1] + values[mid]) / 2


def main(path):
    with open(path) as handle:
        header = handle.readline().rstrip("\n").split("\t")
        rows = [dict(zip(header, line.rstrip("\n").split("\t"))) for line in handle if line.strip()]

    if not rows:
        print("no rows — every run failed to produce a bench line")
        return 1

    # scene -> config -> list of per-run dicts of floats
    grouped = defaultdict(lambda: defaultdict(list))
    for row in rows:
        parsed = {}
        for key, value in row.items():
            if key in ("scene", "config", "run", "draws"):
                parsed[key] = value
            else:
                try:
                    parsed[key] = float(value)
                except ValueError:
                    parsed[key] = 0.0
        grouped[row["scene"]][row["config"]].append(parsed)

    for scene in grouped:
        configs = grouped[scene]
        if REFERENCE not in configs:
            print(f"\n=== {scene}: no {REFERENCE} reference captured, skipping\n")
            continue

        def stat(config, key):
            return median([r[key] for r in configs[config]])

        def spread(config, key):
            values = [r[key] for r in configs[config]]
            return min(values), max(values)

        ref_frame = stat(REFERENCE, "wall_ms")
        ref_render = sum(stat(REFERENCE, p) for p in RENDER_RES_PASSES)
        entities = int(stat(REFERENCE, "entities"))
        runs = len(configs[REFERENCE])

        print(f"\n=== {scene} — {entities} entities, {runs} runs, median (min–max)")
        print(
            f"{'config':<17}{'fps':>8}{'frame ms':>14}{'render ms':>11}"
            f"{'upscale':>9}{'present':>9}{'render rec.':>16}{'net rec.':>16}"
        )
        for config in ORDER:
            if config not in configs:
                continue
            fps = stat(config, "wall_fps")
            frame = stat(config, "wall_ms")
            render = sum(stat(config, p) for p in RENDER_RES_PASSES)
            upscale = stat(config, "gpu_upscale")
            present = stat(config, "gpu_presentation")
            lo, hi = spread(config, "wall_ms")

            if config == REFERENCE:
                render_rec = net_rec = ""
            else:
                render_delta = ref_render - render
                net_delta = ref_frame - frame
                render_rec = (
                    f"{render_delta:+.2f} ({render_delta / ref_render * 100:+.0f}%)"
                    if ref_render > 0
                    else f"{render_delta:+.2f}"
                )
                net_rec = (
                    f"{net_delta:+.2f} ({net_delta / ref_frame * 100:+.0f}%)"
                    if ref_frame > 0
                    else f"{net_delta:+.2f}"
                )

            frame_cell = f"{frame:.2f} ±{(hi - lo) / 2:.2f}"
            print(
                f"{config:<17}{fps:>8.1f}{frame_cell:>14}"
                f"{render:>11.2f}{upscale:>9.3f}{present:>9.3f}"
                f"{render_rec:>16}{net_rec:>16}"
            )

        print(
            "  render ms = "
            + " + ".join(p.replace("gpu_", "") for p in RENDER_RES_PASSES)
            + " (render-resolution passes only; upscale and present excluded)"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "target/fsr-bench/raw.tsv"))
