#!/usr/bin/env python3
"""M-LIGHT content gate A11 (docs/prd/reference-lighting.md RLT-CNT-001..003).

Compares a scene's linear capture with the anchor captured at the current
phase's entry (scripts/m-light-baselines.sh writes both seeds of it), on a
fixed 32 x 32-pixel tile grid. Per tile it measures the mean change of linear
luminance relative to the anchor tile's mean, and the 95th percentile of the
per-pixel relative change inside the tile, so a brightening and a darkening
in one region cannot cancel.

A tile passes when both changes are within the scene's tolerance — that
tile's two-capture floor (anchor seed 0 against seed 1) plus the per-scene
margin from the anchor manifest — or when it lies inside a declared region
(RLT-CNT-002) and changed in the declared direction by no more than the
declared size plus its tolerance. A declared region that did not change is
reported, not failed.

Declaration file (TSV, one entry per line, `#` comments):
    scene  tiles  direction  expected  tolerance  note
where `tiles` is `x0-x1,y0-y1` in tile indices (inclusive), `direction` is
`brighter` or `darker`, and `expected` / `tolerance` are relative luminance
changes (0.05 = 5 %).

    content.py compare --anchors DIR --capture FILE --scene NAME \\
        [--declarations FILE] [--margin 0.01]
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import mitsuba as mi

LUMA = np.array([0.2126, 0.7152, 0.0722])
TILE = 32
DARK = 1e-4  # tiles darker than this mean luminance are skipped (sky, voids)


@dataclass
class Declaration:
    scene: str
    x0: int
    x1: int
    y0: int
    y1: int
    direction: str
    expected: float
    tolerance: float
    note: str

    def covers(self, tx: int, ty: int) -> bool:
        return self.x0 <= tx <= self.x1 and self.y0 <= ty <= self.y1


def load_declarations(path: Path | None, scene: str) -> list[Declaration]:
    if path is None:
        return []
    out = []
    for raw in path.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) < 5:
            raise SystemExit(f"content: malformed declaration line: {raw!r}")
        if fields[0] != scene:
            continue
        xs, ys = fields[1].split(",")
        x0, x1 = (int(v) for v in xs.split("-"))
        y0, y1 = (int(v) for v in ys.split("-"))
        if fields[2] not in ("brighter", "darker"):
            raise SystemExit(f"content: direction must be brighter or darker: {raw!r}")
        out.append(Declaration(fields[0], x0, x1, y0, y1, fields[2], float(fields[3]), float(fields[4]),
                               fields[5] if len(fields) > 5 else ""))
    return out


def luminance(path: Path) -> np.ndarray:
    return np.array(mi.Bitmap(str(path)), dtype=np.float64)[..., :3] @ LUMA


def tile_changes(capture: np.ndarray, anchor: np.ndarray):
    """Yield (tx, ty, anchor_mean, mean_change, p95_change) per tile."""
    h, w = anchor.shape
    for ty in range(h // TILE):
        for tx in range(w // TILE):
            a = anchor[ty * TILE:(ty + 1) * TILE, tx * TILE:(tx + 1) * TILE]
            c = capture[ty * TILE:(ty + 1) * TILE, tx * TILE:(tx + 1) * TILE]
            base = float(a.mean())
            if base < DARK:
                continue
            mean_change = (float(c.mean()) - base) / base
            per_pixel = (c - a) / base
            p95 = float(np.percentile(np.abs(per_pixel), 95))
            yield tx, ty, base, mean_change, p95


def compare(args: argparse.Namespace) -> int:
    anchor = luminance(args.anchors / f"{args.scene}_s0.exr")
    anchor_b = luminance(args.anchors / f"{args.scene}_s1.exr")
    capture = luminance(args.capture)
    if capture.shape != anchor.shape:
        raise SystemExit(f"content: capture {capture.shape} and anchor {anchor.shape} differ in size")
    floors = {(tx, ty): (abs(m), p) for tx, ty, _, m, p in tile_changes(anchor_b, anchor)}
    declarations = load_declarations(args.declarations, args.scene)
    seen_declared = {id(d): False for d in declarations}
    failures = []
    tiles = 0
    for tx, ty, _, mean_change, p95 in tile_changes(capture, anchor):
        tiles += 1
        floor_mean, floor_p95 = floors.get((tx, ty), (0.0, 0.0))
        tol_mean = floor_mean + args.margin
        tol_p95 = floor_p95 + args.margin
        if abs(mean_change) <= tol_mean and p95 <= tol_p95:
            continue
        covering = [d for d in declarations if d.covers(tx, ty)]
        ok = False
        for d in covering:
            sign = 1.0 if d.direction == "brighter" else -1.0
            if sign * mean_change > 0 and abs(mean_change) <= d.expected + d.tolerance + tol_mean:
                ok = True
                seen_declared[id(d)] = True
        if not ok:
            failures.append((tx, ty, mean_change, p95, tol_mean, tol_p95))
    print(f"content: {args.scene}: {tiles} tiles, {len(failures)} outside tolerance and declarations "
          f"(margin {args.margin}, tile {TILE})")
    for tx, ty, m, p, tm, tp in failures[:40]:
        print(f"  FAIL tile ({tx},{ty}): mean {m:+.4f} (tol {tm:.4f}), p95 {p:.4f} (tol {tp:.4f})")
    for d in declarations:
        if not seen_declared[id(d)]:
            print(f"  REPORT declared {d.direction} {d.expected:+.3f} on tiles "
                  f"{d.x0}-{d.x1},{d.y0}-{d.y1} did not appear ({d.note})")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    cmp = sub.add_parser("compare")
    cmp.add_argument("--anchors", type=Path, required=True)
    cmp.add_argument("--capture", type=Path, required=True)
    cmp.add_argument("--scene", required=True)
    cmp.add_argument("--declarations", type=Path)
    cmp.add_argument("--margin", type=float, default=0.01,
                     help="per-scene margin over the two-capture floor (RLT-CNT-001)")
    args = parser.parse_args()
    mi.set_variant("scalar_rgb")
    return compare(args)


if __name__ == "__main__":
    sys.exit(main())
