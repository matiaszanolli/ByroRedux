#!/usr/bin/env python3
"""Validate and summarize paired main-pass RT decomposition variants."""

import csv
import statistics
import sys
from collections import defaultdict


def median(rows, key):
    return statistics.median(float(row[key]) for row in rows)


def main(path):
    with open(path, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    if not rows:
        print("rt-decomposition: no rows")
        return 1

    fingerprints = {
        (
            row["mode"], row["camera"], row["sim_time_s"],
            row["entities"], row["draws"], row["lights"],
            row["tlas"], row["state_hash"],
        )
        for row in rows
    }
    if len(fingerprints) != 1:
        print(f"rt-decomposition: INVALID — {len(fingerprints)} scene-state fingerprints")
        for fingerprint in sorted(fingerprints):
            print("  " + " | ".join(fingerprint))
        return 1

    grouped = defaultdict(list)
    for row in rows:
        grouped[row["variant"]].append(row)
    if "baseline" not in grouped:
        print("rt-decomposition: missing baseline")
        return 1

    baseline_gpu = median(grouped["baseline"], "gpu_main_ms")
    baseline_wall = median(grouped["baseline"], "wall_ms")
    print(
        f"scene={rows[0]['scene']} mode={rows[0]['mode']}/{rows[0]['camera']} "
        f"runs={len(grouped['baseline'])} gpu_main baseline={baseline_gpu:.3f} ms"
    )
    print(
        f"{'feature':<20}{'runtime gpu':>13}{'compile gpu':>13}"
        f"{'avoided exec':>15}{'specialization':>17}{'runtime wall':>14}"
    )
    for feature in ("direct-shadow", "gi", "reflection-glass", "all-main-rays"):
        runtime_key = f"runtime-{feature}"
        compile_key = f"compile-{feature}"
        if runtime_key not in grouped or compile_key not in grouped:
            continue
        runtime_gpu = median(grouped[runtime_key], "gpu_main_ms")
        compile_gpu = median(grouped[compile_key], "gpu_main_ms")
        runtime_wall = median(grouped[runtime_key], "wall_ms")
        avoided = baseline_gpu - runtime_gpu
        specialization = runtime_gpu - compile_gpu
        print(
            f"{feature:<20}{runtime_gpu:>13.3f}{compile_gpu:>13.3f}"
            f"{avoided:>15.3f}{specialization:>17.3f}{runtime_wall:>14.3f}"
        )

    print(
        f"\nBaseline wall={baseline_wall:.3f} ms. Avoided execution is "
        "baseline→runtime-off; specialization is runtime-off→compile-off. "
        "Do not call a residual shared cost without occupancy/register or "
        "instruction-cache evidence."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "target/rt-decomposition/raw.tsv"))
