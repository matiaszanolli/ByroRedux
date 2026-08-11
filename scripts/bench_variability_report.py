#!/usr/bin/env python3
"""Summarize system-live benchmark variability without treating it as a gate."""

import csv
import statistics
import sys
from collections import defaultdict


NUMERIC_COLUMNS = (
    "wall_ms",
    "fence_ms",
    "brd_ms",
    "gpu_main_ms",
    "sim_time_s",
)


def median_absolute_deviation(values):
    center = statistics.median(values)
    return statistics.median(abs(value - center) for value in values)


def envelope(values):
    center = statistics.median(values)
    low = min(values)
    high = max(values)
    peak_to_peak = ((high - low) / center * 100.0) if center else 0.0
    return center, median_absolute_deviation(values), low, high, peak_to_peak


def main(path):
    with open(path, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    if not rows:
        print("bench-envelope: no completed rows")
        return 1

    groups = defaultdict(list)
    invalid = False
    for row in rows:
        if row["mode"] != "system-live" or row["camera"] != "free":
            print(
                f"bench-envelope: INVALID {row['scene']} run {row['run']} "
                f"uses {row['mode']}/{row['camera']}"
            )
            invalid = True
        try:
            for column in NUMERIC_COLUMNS:
                row[column] = float(row[column])
        except ValueError as error:
            print(
                f"bench-envelope: INVALID {row['scene']} run {row['run']}: {error}"
            )
            invalid = True
            continue
        groups[row["scene"]].append(row)

    print(
        "scene         runs  wall_ms median  MAD    min    max    p2p%  "
        "sim_s range       fingerprints"
    )
    for scene in sorted(groups):
        scene_rows = groups[scene]
        wall = [row["wall_ms"] for row in scene_rows]
        center, mad, low, high, peak_to_peak = envelope(wall)
        sim = [row["sim_time_s"] for row in scene_rows]
        fingerprints = {
            (
                row["camera_pos"],
                row["camera_forward"],
                row["entities"],
                row["draws"],
                row["lights"],
                row["tlas"],
                row["state_hash"],
            )
            for row in scene_rows
        }
        print(
            f"{scene:<13}{len(scene_rows):>4}  {center:>14.2f}  {mad:>5.2f}  "
            f"{low:>5.2f}  {high:>5.2f}  {peak_to_peak:>6.1f}  "
            f"{min(sim):>6.3f}–{max(sim):<6.3f}  {len(fingerprints):>4}"
        )

        # These are the agreed forensic columns, kept visible per row rather
        # than collapsed into the performance aggregate.
        for row in scene_rows:
            print(
                f"  run {row['run']}: camera={row['camera_pos']} "
                f"forward={row['camera_forward']} "
                f"sim={row['sim_time_s']:.6f}s entities={row['entities']} "
                f"draws={row['draws']} lights={row['lights']} tlas={row['tlas']} "
                f"hash={row['state_hash']}"
            )

    print(
        "\nSystem-live is observational only: hash drift is forensic evidence, "
        "not a failed gate."
    )
    return 1 if invalid else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "target/bench-variability-envelope/raw.tsv"))
