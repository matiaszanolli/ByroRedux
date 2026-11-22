# RCG Phase 3 — GI cost flatness before the cache (2026-11-20)

Evidence for [radiance-cache-gi-coverage](../prd/radiance-cache-gi-coverage.md) Phase 3 (#7074), RCG-PERF-004 and the §8 cost-flatness metric, (dearest − cheapest) ÷ cheapest over distance buckets. This is the **control**: the engine with no radiance cache, the footprint gate and the distance fade in place. Raw rows: [RCG_phase3_flatness_baseline_2026-11-20.tsv](RCG_phase3_flatness_baseline_2026-11-20.tsv).

## Result

The main-pass time the GI rays add, per distance bucket of the receding-plane lab (tier 3, median of 5 runs per side; the noise floor is half the run-to-run range of the two sides combined):

| Camera height over the plane | GI cost, ms | Noise floor, ms | Eligible pixels traced |
|---|---|---|---|
| 2 m | +1.050 | 0.279 | 100 % |
| 8 m | +0.002 | 0.164 | 0 % |
| 32 m | −0.113 | 0.226 | 0 % |
| 128 m | −0.038 | 0.337 | 0 % |
| 512 m | +0.007 | 0.005 | 0 % |
| 2048 m | +0.010 | 0.009 | 0 % |

**The cost is not flat.** The spread is 1.163 ms, all of it in the 2 m bucket; every other bucket is within its noise floor of zero. The §8 ratio reads 3.46 with the cheapest bucket taken at the largest noise floor (0.337 ms). The cheapest buckets measure zero, so the true ratio is unbounded and 3.46 is a lower bound on it. This is the footprint gate: it traces the pixels at 2 m and culls every pixel from 8 m out (`traced_fraction`, the share of RT-LOD-eligible fragments that took the traced share, is 1 and then 0). A flat cost needs every bucket traced, which is the cache's job.

The budget this gate judges against is the figure Phase 1 publishes (RCG-PERF-004), and it is unset: `FLATNESS_BUDGET` is `None` and every row reads `report`.

## How it was measured

`scripts/m-light-gates.sh --gates RCG-FLAT --profile release` at revision `d24fafc6634c-dirty` (the tree held this gate's own uncommitted code), RTX 4070 Ti, 1280×720, tier 3 pinned.

- **Buckets.** `--recede-lab` (a flat 0.5-albedo plane to the far plane, one fixed sun), the camera at 2, 8, 32, 128, 512 and 2048 m looking straight down, so every pixel of a frame is at about its height and the plane fills the frame. The gate checks that every bucket holds the same fragment count (about 409,440, within 1 % of the median), so a bucket's pixels are the same number and the same material.
- **Cost.** `gpu_main_render` of the final bench frame, 300 frames per run, with and without `DBG_DISABLE_GI_RAYS`, 5 runs per side interleaved across buckets. The GI cost is the median with GI on minus the median with it off, which removes the raster and direct-light cost that is the same in every bucket.
- **Angle.** The buckets vary distance only; RCG-PERF-004 also names angle, and the §8 metric names distance buckets alone. The lab's grazing pose is the horizon view, which is not a bucket here.
- **Noise.** The GPU was shared with other work in this session, and `gpu_main_render` is one retired frame's value, so single runs move by 0.2 to 0.4 ms at the near buckets. The 2 m result (1.05 ms against a 0.28 ms floor) is outside that noise.
