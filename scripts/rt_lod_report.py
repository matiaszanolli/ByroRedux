#!/usr/bin/env python3
"""Report an rt-lod-sweep matrix and select the largest visual-safe scale."""

from __future__ import annotations

import csv
import statistics
import sys
from collections import defaultdict
from pathlib import Path

try:
    import numpy as np
    from PIL import Image
except ImportError as error:
    raise SystemExit(f"rt_lod_report.py requires numpy and Pillow: {error}")


SSIM_THRESHOLD = 0.995
BLOCK = 8


def linear_rgb(path: str) -> np.ndarray:
    encoded = np.asarray(Image.open(path).convert("RGB"), dtype=np.float64) / 255.0
    return np.where(
        encoded <= 0.04045,
        encoded / 12.92,
        ((encoded + 0.055) / 1.055) ** 2.4,
    )


def block_ssim(reference: np.ndarray, candidate: np.ndarray) -> float:
    if reference.shape != candidate.shape:
        raise ValueError(f"capture dimensions differ: {reference.shape} != {candidate.shape}")
    height, width, channels = reference.shape
    height -= height % BLOCK
    width -= width % BLOCK

    def blocks(image: np.ndarray) -> np.ndarray:
        return (
            image[:height, :width]
            .reshape(height // BLOCK, BLOCK, width // BLOCK, BLOCK, channels)
            .transpose(0, 2, 1, 3, 4)
            .reshape(-1, BLOCK * BLOCK, channels)
        )

    left = blocks(reference)
    right = blocks(candidate)
    left_mean = left.mean(axis=1)
    right_mean = right.mean(axis=1)
    left_variance = left.var(axis=1, ddof=1)
    right_variance = right.var(axis=1, ddof=1)
    covariance = (
        (left - left_mean[:, None]) * (right - right_mean[:, None])
    ).sum(axis=1) / (BLOCK * BLOCK - 1)
    c1 = 0.01**2
    c2 = 0.03**2
    score = (
        (2 * left_mean * right_mean + c1) * (2 * covariance + c2)
    ) / (
        (left_mean**2 + right_mean**2 + c1)
        * (left_variance + right_variance + c2)
    )
    return float(score.mean())


def main(path: str) -> int:
    with Path(path).open(newline="") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    if not rows:
        print("no RT LOD rows")
        return 1

    grouped: dict[str, dict[float, list[dict[str, str]]]] = defaultdict(
        lambda: defaultdict(list)
    )
    for row in rows:
        grouped[row["scene"]][float(row["scale"])].append(row)

    scene_scores: dict[str, dict[float, float]] = defaultdict(dict)
    invalid_fingerprint = False
    for scene, scales in grouped.items():
        reference_scale = min(scales)
        reference_row = next(row for row in scales[reference_scale] if row["kind"] == "telemetry")
        reference = linear_rgb(reference_row["capture"])
        fingerprints = {
            (row["mode"], row["camera"], row["state_hash"])
            for scale_rows in scales.values()
            for row in scale_rows
        }
        if len(fingerprints) != 1:
            invalid_fingerprint = True
            print(f"\n=== {scene}: INVALID — scene fingerprints differ: {sorted(fingerprints)}")
            continue

        print(f"\n=== {scene} — reference scale {reference_scale:g}")
        print(
            f"{'scale':>9} {'SSIM':>9} {'gpu main ms':>13} {'LOD bins':>31} "
            f"{'refl traced/culled':>21} {'GI traced/culled':>21}"
        )
        for scale in sorted(scales):
            scale_rows = scales[scale]
            telemetry = next(row for row in scale_rows if row["kind"] == "telemetry")
            timing = [
                float(row["gpu_main_ms"])
                for row in scale_rows
                if row["kind"] == "timing"
            ]
            score = block_ssim(reference, linear_rgb(telemetry["capture"]))
            scene_scores[scene][scale] = score
            gpu = statistics.median(timing)
            bins = "/".join(telemetry[f"lod{index}"] for index in range(4))
            reflection = (
                f"{telemetry['reflection_traced']}/{telemetry['reflection_lod_culled']}"
            )
            gi = f"{telemetry['gi_traced']}/{telemetry['gi_lod_culled']}"
            print(f"{scale:9g} {score:9.6f} {gpu:13.3f} {bins:>31} {reflection:>21} {gi:>21}")

    if invalid_fingerprint:
        return 1
    common_scales = set.intersection(
        *(set(scores) for scores in scene_scores.values())
    )
    safe = [
        scale
        for scale in common_scales
        if min(scores[scale] for scores in scene_scores.values()) >= SSIM_THRESHOLD
    ]
    if not safe:
        print(f"\nNo scale met SSIM >= {SSIM_THRESHOLD:.3f} in every scene.")
        return 1
    selected = max(safe)
    worst = min(scores[selected] for scores in scene_scores.values())
    print(
        f"\nSelected scale {selected:g}: largest declared scale with per-scene "
        f"linear block-SSIM >= {SSIM_THRESHOLD:.3f} (worst {worst:.6f})."
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: rt_lod_report.py RAW_TSV")
    raise SystemExit(main(sys.argv[1]))
