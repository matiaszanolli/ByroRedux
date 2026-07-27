# RT-1: #2165's fix does not restore indirect grouping — fnv gpu_calls still 23, oblivion 31, fo4 48 at HEAD

Severity: MEDIUM · Labels: medium, renderer, performance, bug
Source: docs/audits/AUDIT_RUNTIME_2026-07-27.md

Regression of #2165 (CLOSED) / #1804 (CLOSED). Filed from `docs/audits/AUDIT_RUNTIME_2026-07-27.md` (RT-1).

## Description

Commit `8e55a714` ("perf: restore particle indirect grouping", 2026-07-25) closed #2165 and states in its own message:

> Confirmed at runtime on three corpora (fnv gpu_calls 10->23, oblivion batches 27->31, fo4 gpu_calls 40->46).

All three corpora were re-measured at that exact commit and at HEAD (`db625997`). **None of the three was restored.** FO4 is now 48, worse than the 46 the commit claims to have repaired.

## Evidence

Three release builds in an isolated git worktree, each driven headless at `--bench-frames 240` under `xvfb-run`, `BYROREDUX_FIXED_DT` unset (the methodology the committed baselines were captured on — confirmed by FNV `wall_fps` reproducing its baseline exactly, 141.4 vs 141.4):

| Corpus / metric | Baseline | Pre-regression `883f57cd~1` | At the "fix" `8e55a714` | HEAD `db625997` |
|---|---|---|---|---|
| fnv `bench_draws_gpu_calls` | 10 | **8** | **23** | **23** |
| oblivion `bench_draws_batches` | 27 | — | **31** | **31** |
| fo4 `bench_draws_gpu_calls` | 40 | — | **48** | **48** |

Full FNV draw split: `883f57cd~1` → `2629/103b/8c`; `8e55a714` and HEAD → `2629/106b/23c` (identical). Oblivion `324/31b/4c` at both the fix and HEAD. FO4 `3824/279b/48c` at both.

The pre-regression build restores FNV to 8 GPU calls, which both validates the committed baseline (~10) and confirms the attribution of the regression to `883f57cd` (2026-07-20).

## Impact

The regression #2165 was opened for is still live on every particle-heavy interior across three game corpora, and is no longer tracked — #2165 and #1804 are both CLOSED. Wasted GPU submission overhead on every affected frame (FNV Freeside is ~2.9x its pre-regression indirect-call count).

Secondary, and arguably the more expensive problem: a runtime-telemetry regression was closed on the strength of a code change that never moved the number. The corrective code is defensible on its own terms; the causal claim attached to it is not.

## Root-cause status

`883f57cd` was blamed via `needs_two_sided_blend_split` losing its `&& b.z_write` limb. `8e55a714` rewrote that predicate to be material-driven (`DrawBatch::order_dependent_glass` resolved from `is_refractive_glass`) — the code is present and correct at HEAD (`crates/renderer/src/vulkan/context/draw.rs:702,739,753-764`) — and the metric did not move a single call on any corpus. This corroborates the standing observation that the two-sided-blend-split predicate is dormant on tested cells (`blended && two_sided` == 0), and that drift attributed to it is misattributed.

**The real mechanism inside `883f57cd` has not been found.** That commit also replaced the `GpuInstance` padding with a stable surface ID, added a thin-glass material flag, and changed alpha-blend blending state.

Verified **non**-causes:
- `is_refractive_glass` (`draw.rs:604-615`) correctly excludes `MATERIAL_KIND_FIRE_REFRACTION` (103) — it covers only `MATERIAL_KIND_GLASS` (100) and `MATERIAL_KIND_MULTI_LAYER_PARALLAX` (11) with a positive refraction scale.
- `surface_id` does not enter the batch merge key or `group_state` (single assignment at `draw.rs:2100`).

## Suggested Fix

1. Bisect *within* `883f57cd` by reverting its sub-changes one at a time against the FNV corpus — 8 → 23 GPU calls is a large, stable signal reachable in a ~12 s headless run.
2. Add a telemetry assertion to the runtime harness so a `gpu_calls` regression cannot be closed without the number moving. This is the gap that let #2165 close green.

Repro:
```
cargo build --release -p byroredux
BYROREDUX_FIXED_DT= RUST_LOG=warn xvfb-run -a --server-args="-screen 0 1280x720x24" \
  ./target/release/byroredux --game fnv --cell FreesideAtomicWrangler --bench-frames 240 \
  | grep -o 'draws=[0-9]*/[0-9]*b/[0-9]*c'
```

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other batch-merge / indirect-gather call sites)
- [ ] **TESTS**: A regression test pins this specific fix — and it must assert on the runtime `gpu_calls` number, not only on the predicate's boolean
- [ ] **TELEMETRY**: The runtime baseline (`.claude/audit-baselines/runtime/`) is re-verified as green on fnv / oblivion / fo4 after the fix, not assumed
