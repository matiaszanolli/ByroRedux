# PERF-D9-03: ScratchTelemetry doc says 5 today; producer now emits 9 rows

**Labels**: low, performance, documentation

**Severity**: LOW
**Dimension**: Telemetry & Origin Cost
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/core/src/ecs/resources/mod.rs:421-426` vs `crates/renderer/src/vulkan/context/mod.rs:2785-2864`

## Description
`ScratchTelemetry`'s doc comment says `rows` "stabilises at the count of registered scratches (5 today)"; the producer (`VulkanContext::fill_scratch_telemetry`) now emits 9 rows (`gpu_instances_scratch`, `batches_scratch`, `indirect_draws_scratch`, `terrain_tile_scratch`, `skin_dispatch_seen_scratch`, `skin_dispatches_scratch`, `skin_first_sight_builds_scratch`, `skin_built_this_frame_scratch`, and one additional row). Pure doc-rot; the "bounded, reused Vec" invariant the comment documents still holds at runtime.

Verified current: the doc comment at `crates/core/src/ecs/resources/mod.rs:421-426` still says "5 today"; the producer at `crates/renderer/src/vulkan/context/mod.rs` pushes at least 8 distinct scratch rows (gpu_instances, batches, indirect_draws, terrain_tile, skin_dispatch_seen, skin_dispatches, skin_first_sight_builds, skin_built_this_frame), confirming the count has grown well past 5.

## Impact
Pure doc-rot; the "bounded, reused Vec" invariant the comment documents still holds at runtime — no functional impact.

## Suggested Fix
Update the doc comment to reflect the current row count (9), or better, drop the specific number and just say "one row per registered scratch, currently N — see `fill_scratch_telemetry`."

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix)
