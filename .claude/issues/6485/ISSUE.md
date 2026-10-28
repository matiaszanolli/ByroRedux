# PERF-D4-01: The #6250 mover list is a fresh per-frame `Vec`, deduplicated by linear `contains`, and keeps growing past the 64 entries it can upload

Labels: low, performance, renderer, bug
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_PERFORMANCE_2026-10-27.md` -- finding `PERF-D4-01` (LOW)
**Location (re-mapped to current code)**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`let mut moved_surface_ids: Vec<u32> = Vec::new();` ~:393, filled in `instance_flags` ~:972-976); consumer `SceneBuffers::upload_moved_surface_ids` in `crates/renderer/src/vulkan/scene_buffer/upload.rs` (keeps `RESTIR_MAX_MOVED_SURFACES` = 64). Premise re-read at HEAD: `if !moved_surface_ids.contains(&moved_sid) { moved_surface_ids.push(moved_sid); }` present, vec created empty per build.
**Status at filing**: NEW

## Description
Every draw with `instance_moved` runs `moved_surface_ids.contains(&sid)` then `push`. `instance_moved` is true when the rigid model changed, or when the entity is in `pose_dirty` (every animated skinned part, every frame). The other buffers in the instance loop are persistent `self.scratch.*`, but this one is rebuilt from empty each frame and regrows every time. Dedup is O(movers^2). Every entry past the 64th is collected and compared only to be dropped by the upload. It also has no `ScratchTelemetry` (`ctx.scratch`) row.

## Evidence
`if !moved_surface_ids.contains(&moved_sid) { moved_surface_ids.push(moved_sid); }` (`instance_flags`); `let kept = moved_surface_ids.len().min(RESTIR_MAX_MOVED_SURFACES as usize);` (`upload_moved_surface_ids`).

## Impact
Render-thread CPU every frame with movers. Derived: ~150 movers (30 animated NPCs x ~5 parts) = ~11 k compares + ~6 reallocations; ~500 movers (dense city) = ~125 k compares, tens of microseconds. No quantitative guard.

## Related
AUDIT_RENDERER_2026-10-27 REN-D2-01 (64 cap overflows at ~10 NPCs, per-frame warn; same producer, its capacity decision fixes the bound); SAFE-2026-10-27-01 (these buffers are not destroyed).

## Suggested Fix
Move the list onto `self.scratch` as a persistent buffer cleared per build, dedup through an `FxHashSet<u32>` scratch or sort+dedup before upload, cap it at whatever capacity REN-D2-01 settles on, and add a `ScratchTelemetry` row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

