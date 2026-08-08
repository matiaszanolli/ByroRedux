# REN-D20-NEW-03: GpuTimerSnapshot::*_active flags have zero consumers -- #2278 landed only the producer half

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2513
**Finding ID**: REN-D20-NEW-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 20 — Debug/Telemetry
**Location**: `crates/renderer/src/vulkan/gpu_timers.rs:193-206` (fields) / `crates/renderer/src/vulkan/context/mod.rs:3162-3216` (`fill_skin_coverage_stats`, the only real consumer of the snapshot)
**Status**: NEW (incomplete fix of Existing PERF-D9-01 / #2278)

## Description
PERF-D9-01 was "`0.0` is ambiguous between 'inactive' and 'genuinely instantaneous'". #2278 added fourteen `*_active: bool` companions to `GpuTimerSnapshot` and `snapshot_from_bits` fills them correctly (three unit tests pin the behaviour). But nothing outside the module ever reads them. `fill_skin_coverage_stats` — the sole path from the snapshot to the world — copies the fourteen `_ms` fields and drops all fourteen `_active` flags; `SkinCoverageStats` (`crates/core/src/ecs/resources/mod.rs`) has no `_active` members; `fill_upscaler_telemetry` reads only `upscale_ms`. A repo-wide grep for the flag names outside `gpu_timers.rs` returns only the module's own tests.

## Evidence
`grep -rn "_active\b" --include="*.rs" crates byroredux tools | grep -v vulkan/gpu_timers.rs` matches nothing in the renderer/telemetry path. `mod.rs:3173-3204` copies `snap.skin_dispatch_ms` ... `snap.presentation_ms` and nothing else; the `else` branch zeroes the same fourteen `_ms` fields, which means "no timer at all" is *also* indistinguishable from "ran at 0 ms" at every surface.

## Impact
The original ambiguity is fully intact everywhere a human actually looks — the debug-UI `gpu_pass_ms` grid, `skin.coverage`, and the bench summary all still print `0.000 ms` for "the pass was skipped this frame", "the pass ran instantly", and "this GPU has no timestamp support". Interacts with the sibling REN-D20-NEW-02 finding (this report): skipped brackets contribute a clean `0.0` to the Σ, which makes the Σ look more trustworthy than it is. Diagnostic-quality only; no runtime effect.

## Related
PERF-D9-01 / #2278; #2040; REN-D20-NEW-02 (this report).

## Suggested Fix
Add the matching `bool` fields to `SkinCoverageStats`, copy them in `fill_skin_coverage_stats`, and have `metrics.rs` emit `None`/"n/a" rather than `0.0` into `gpu_pass_ms` for inactive brackets (widening the tuple to `(String, Option<f32>)`).

## Completeness Checks
- [ ] **TESTS**: A regression test confirms an inactive bracket renders as "n/a" not "0.000 ms" in the debug-UI grid
