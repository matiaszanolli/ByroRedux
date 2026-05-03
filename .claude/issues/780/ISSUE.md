**Severity**: LOW
**Dimension**: Material Table & SSBO Upload (R1)
**Source**: AUDIT_PERFORMANCE_2026-05-01.md

## Locations
- [crates/renderer/src/vulkan/material.rs:275-329](../../tree/main/crates/renderer/src/vulkan/material.rs#L275-L329) (`MaterialTable`)
- [byroredux/src/render.rs:786](../../tree/main/byroredux/src/render.rs#L786), [byroredux/src/render.rs:932](../../tree/main/byroredux/src/render.rs#L932) (intern call sites)
- [crates/renderer/src/vulkan/scene_buffer.rs:957-992](../../tree/main/crates/renderer/src/vulkan/scene_buffer.rs#L957-L992) (upload path)
- `ScratchTelemetry` resource (currently 5 tracked scratches: gpu_instances, batches, indirect_draws, terrain_tile, tlas_instances)

## Description

The R1 dedup win is the headline perf change of the 2026-04-20 → 2026-05-01 audit period — but no per-frame metric exposes the dedup ratio (`material_count_unique / material_intern_calls`). Existing `ScratchTelemetry` resource does not include materials.

A future regression that breaks the byte-equality dedup — e.g. someone adds a `[f32; 3]` field that breaks std430 alignment, or a non-deterministic float in the producer that yields different bytes for "identical" materials — would silently inflate material counts without any visible signal until VRAM pressure or upload cost shows up in late-cycle profiling.

## Impact

Observability gap. Today's win is real (~40× dedup ratio on Prospector baseline: 1200 DrawCommands → 30 unique materials) but unverified against larger cells; tomorrow's regression goes undetected.

This issue also blocks **PERF-N2** (`MAX_MATERIALS=4096` right-sizing) and **PERF-N3** (`MAX_TOTAL_BONES=32768` right-sizing) — both deferred until empirical peak data is collected via this telemetry.

## Suggested Fix

Add two fields to `ScratchTelemetry` (the resource refreshed per-frame and surfaced via `ctx.scratch` console command):

```rust
pub struct ScratchTelemetry {
    // ... existing 5 scratches ...
    /// R1 — unique material count after dedup (== materials.len() at end of frame).
    pub materials_unique: usize,
    /// R1 — total intern() calls (1 per DrawCommand). Dedup ratio = unique / interned.
    pub materials_interned: usize,
}
```

Wiring:
1. Add `interned_count: usize` field to `MaterialTable`, increment in `intern()` (after the dedup early-return decision).
2. In `build_render_data`'s tail where the existing telemetry is updated, copy `material_table.len()` and `material_table.interned_count()` into the resource.
3. `ctx.scratch` console output formatter prints both (e.g. `materials: 30 unique / 1200 interned (40.0× dedup)`).
4. Add a unit test in `material.rs` confirming `interned_count` increments on hit AND miss paths.

~10 lines wiring + 1 test.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify the existing 5 scratches' update sites match the new pattern (interned_count cleared in `MaterialTable::clear`, same as the existing scratch counters)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Yes — assert `MaterialTable::interned_count()` returns 0 after `clear()`, increments on every `intern()` call (hit OR miss), and `unique_count + (interned - unique)` correctly reflects insertions vs hits.
