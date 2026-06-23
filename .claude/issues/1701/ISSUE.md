# RT-2: Oblivion Gilded Carafe FPS dropped 14.4% below the ×0.9 gate (411.8→352.3); gpu_calls 3→4

**Severity**: MEDIUM
**Dimension**: performance — surfaced via runtime telemetry
**Location**: `bench:` line of `/tmp/audit/runtime/oblivion-ICMarketDistrictTheGildedCarafe.engine.log`.
**Status**: NEW (CONFIRMED against live telemetry 2026-06-23)

## Description
On Oblivion's tiny clean cell (701 entities), bench fps fell 411.8→352.3, just under the `≥ baseline×0.9` gate (370.6). GPU calls ticked 3→4. All structural symptom metrics (entities, tex.missing=0, mesh_fail=0, skin) are unchanged.

## Evidence
```
bench: frames=240 wall_fps=352.3 wall_ms=2.84 ... systems_ms=0.14 entities=701 draws=324/30b/4c
```
(baseline `draws=324/30b/3c`)

## Impact
Low absolute — both fps and the +1 GPU call are small moves on a 4-call, 400-fps scene where Xvfb wall-clock jitter dominates. The prior audit (RT-2, 06-14) explicitly flagged headless `bench_fps_*` as unreliable and recommended demoting it from the hard gate. This finding is the second data point for that demotion; absent the fps gate it would be a clean PASS.

## Suggested Fix
Re-run 3× and average before treating as a true regression; if it holds, bisect the `draws` gpu-call split. Otherwise fold into the standing decision to make `bench_fps_*` advisory rather than gating (06-14 RT-2).

## Completeness Checks
- [ ] **SIBLING**: Re-run all five game benches to confirm the gpu-call split is cell-specific
- [ ] **TESTS**: If `bench_fps_*` is demoted to advisory, the baseline gate config records the change
