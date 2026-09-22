# PERF-D8-2026-09-21-02: Six persistent per-frame scratches have no `ScratchTelemetry` row, and the coverage guard pins a fixed name list

**Labels**: bug, renderer, low, performance, test-gap

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW · **Dimension**: 8 — Telemetry & Origin
**Location**:
- `crates/renderer/src/vulkan/context/telemetry.rs:44-58`: `fill_scratch_telemetry`'s maintenance rule ("every persistent `Vec` scratch declared in this crate must show up here")
- `crates/renderer/src/vulkan/context/mod.rs:269-289`: `ScratchBuffers`, with `instance_map_scratch` at `:289`
- `byroredux/src/app_events.rs:700-770`: the App-side `ScratchRow`s
- `byroredux/src/main.rs:532-551`: the App-owned ground-cover scratches
- `crates/renderer/src/vulkan/context/mod.rs:2044`: `fill_scratch_telemetry_covers_all_four_previously_missing_scratches`

**Status**: NEW
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- The renderer's `instance_map_scratch` (#4193, `fd9d000b3`) has no row. `fill_scratch_telemetry` emits `gpu_instances_scratch`, `batches_scratch` and the others, but never this one.
- The App-owned `groundcover_cells`, `groundcover_chunks`, `groundcover_species`, `groundcover_species_table` and `groundcover_disturbers` have no rows either. `app_events.rs` emits `draw_commands`, `water_commands`, `gpu_lights`, `gpu_fog_volumes`, `light_sort_scratch`, `bone_world` and `skin_offsets` only.
- The coverage guard `fill_scratch_telemetry_covers_all_four_previously_missing_scratches` names only the four scratches from #3693 (`blend_seen_scratch`, `tlas_addresses_scratch`, `tlas_missing_samples_scratch`, `water_param_scratch`). A new scratch field therefore never trips it. Same class as #3693 and #3694 (both LOW, closed; their named scratches are still covered).

## Impact

These six scratches can grow with no visibility in `ctx.scratch`. That is the pre-R6 blind spot the maintenance rule exists to prevent. The rule is enforced only for names someone has already remembered.

## Related

- #3693 and #3694 (closed): the same class; this is new scratches, not a regression of theirs.
- #4193: added `instance_map_scratch`.
- #4607 (PERF-D1-2026-09-21-01): the ground-cover collectors that fill these App scratches, plus the per-frame intermediates that have no persistent scratch at all.

## Suggested Fix

Add the six rows. Make the guard enumerate the `ScratchBuffers` fields (and the App's scratch fields) from source instead of a fixed name list, in the style of #4050's `sweep_purges_every_entity_keyed_collection` (`crates/core/src/ecs/resources/skin_slot_pool.rs`).

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D8-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Every persistent scratch on `VulkanContext` sub-managers and on `App` is enumerated, not just the six named here
- [ ] **TESTS**: The coverage guard derives its expected set from source, so adding a scratch field without a row fails it

