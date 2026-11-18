# M-LIGHT SHD-012 darkened tiles — 2026-11-16

This investigates the Phase 3 A11 declaration; it does not accept the change
or make the declaration advance-declared. Release engines at `c3a804d89`
(`55ba81e20^`) and `55ba81e20` were built from clean archived sources using
the same toolchain. On an NVIDIA RTX 4070 Ti, driver 580.178.04, each captured
the Phase 1b Cornell and Dugout poses at the recorded TAA tier, with jitter
off, 64 linear-capture frames and seeds 0 and 1. Captures and engine logs:
`/tmp/byro-shd012-pre/` and `/tmp/byro-shd012-post/`. The snapshots use the
historical capture code; these are diagnostic repeats, not new phase-entry
anchors.

| Scene and tile | Seed 0 change | Seed 1 change | Control seed shift | Changed seed shift |
|---|---:|---:|---:|---:|
| Cornell (15,11) | -3.12% | -3.31% | +1.21% | +1.02% |
| Cornell (15,12) | -5.37% | -5.37% | +0.61% | +0.60% |
| Dugout (16,15) | -1.73% | -1.80% | +0.27% | +0.19% |
| Dugout (17,15) | -5.85% | -6.04% | +3.43% | +3.24% |

Values are tile mean linear-luminance changes relative to the seed-0 control
tile mean. The similar paired-seed changes make a single seed unlikely to
explain the darkening, although two seeds do not establish a full noise
distribution. The normal undeclared comparison fails 161 of 754 Cornell
tiles (the previous measurement was 163), and exactly 2 of 833 Dugout tiles,
as before. Dugout (17,15)'s control seed difference is large and the 95th
percentile pixel change is 27.5% of its mean luminance, below that tile's
82.6% two-seed floor plus margin. Tile (16,15)'s corresponding figures are
13.6% and 58.0%. These noisy pixel tails limit what the tile averages alone
can establish.

The Cornell tiles are on the red wall beside a shadow edge; both darkened at
every pixel in this repeat. The Dugout tiles cover a dark doorway edge and
the bottom of the jukebox; 26% and 84% of their pixels respectively darkened.
Annotated diagnostic images are `/tmp/cornell-shadow-tiles.png` and
`/tmp/dugout-shadow-tiles.png`. SHD-012 changes shadow-ray transmission by
weighting emitter samples according to receiver cosine. A penumbra can change
in either direction depending on which samples are blocked, so the locations
are consistent with the change, but these final-image captures do not isolate
direct visibility or prove closer agreement with a reference. A controlled
direct-visibility comparison or reference for these regions remains needed
before review. The requirement's declaration was written after the change,
so the advance-declaration exception remains open independently of the visual
result.

An additional same-pose, same-tier, 64-frame seed-0 `direct_only` capture
bypasses the final-image path and TAA. Cornell (15,11) falls from 0.11829 to
0.11239 (-4.99%), and (15,12) from 0.08210 to 0.07493 (-8.73%), in tile
mean direct-view luminance. Thus the darkening is already present in the
direct-light output, consistent with the visibility-weight change; this
does not independently validate it against a reference. Captures:
`/tmp/byro-shd012-direct-pre/` and `/tmp/byro-shd012-direct-post/`.
In Dugout, (16,15) falls from 0.01520 to 0.01473 (-3.08%) and (17,15)
from 0.01158 to 0.00988 (-14.69%) in that same view. This is one seed;
the direct-view noise floor and reference error at these tiles remain
unmeasured.

The 2026-11-18 A11 rerun used the same historical captures with partial
edge tiles included and applied the declaration to both mean and p95
change. Cornell has five failures out of 790 tiles. Tile (27,20) exceeds
the declared p95 allowance. Four dark tiles on the bottom edge,
(18,22), (19,22), (20,22), and (33,22), are outside the declaration.
Dugout has zero failures out of 851 tiles.

A separate 2026-11-18 same-machine comparison covered the moving-blocker
update and radius debug view at the Phase 1b poses and pinned TAA tiers.
It used 64-frame captures with jitter disabled, control seeds 0 and 1,
and candidate seed 0. With the 1% margin and no declaration, all five
scenes had zero failures: Cornell 789 tiles, Prospector 545, Whiterun
920, MedTek 920, and Dugout 851. This comparison covers the new patch
against its immediate control. The full Phase 3 comparison stays open.
