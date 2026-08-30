# RT-13: first-frame hitch of 29 s (fnv) / 10 s (fo3) blocks the render thread — cell load runs on it

**Issue**: #3559
**Labels**: bug, low, performance
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-13.

## Description

Measured per-frame tail latency shows a single blocking first frame of tens of seconds on two games:

| Game | `frame_p50_ms` | `frame_p95_ms` | `frame_max_ms` |
|---|---|---|---|
| fnv | 15.88 | 16.77 | **29164.91** |
| fo3 | 8.35 | 19.85 | **9960.93** |
| skyrim_se | 8.58 | 11.55 | 41.11 |
| oblivion | 3.55 | 15.85 | 19.95 |
| fo4 | 23.61 | 24.71 | 120.14 |

The p95s (16.77 / 19.85) are unremarkable, so this is **one blocking frame** — cell load running on the render thread — not a distribution problem.

## Scope note (why this is LOW, and what it is not)

`bench_frame_*_ms` is advisory under `xvfb` per RT-2 / #1701, and is **not** raised here as a baseline-gating regression. The magnitude is nonetheless real and the mechanism is structural: skyrim (41 ms) and oblivion (20 ms) do not show it, so it scales with cell content rather than being a universal harness artifact.

## Impact

A 29-second unresponsive window at cell entry on FNV. Under a windowed (non-`xvfb`) run this is a hung window and an OS not-responding prompt, not merely a slow frame.

## Suggested Fix

Move cell-load work off the render thread, or chunk it against the existing `STREAMING_APPLY_BUDGET` the way exterior streaming already does. Instrumenting *which* phase of cell load owns the 29 s (mesh import, texture upload, collider build, BLAS build) is the necessary first step — the current telemetry only bounds it.

## Completeness Checks
- [ ] **SIBLING**: The interior cell-load path checked against the exterior streaming path, which already has a per-frame budget
- [ ] **TESTS**: A telemetry assertion that bounds `frame_max_ms` relative to `frame_p95_ms`, so a regression is visible without reading raw numbers
