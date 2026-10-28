# REN-D2-01: The #6250 moved-surface set (cap 64) overflows in any populated scene, and `upload_moved_surface_ids` then warns on every frame

Labels: medium, renderer, bug
Source: docs/audits/AUDIT_RENDERER_2026-10-27.md

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-27.md` — finding `REN-D2-01` (MEDIUM, Dim 2 Ray Queries). Follow-up defect in the fix for #6250 (closed, `bed162262`), not a re-open of its original bug.
**Location (current)**: `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_moved_surface_ids`); producer `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`instance_flags`, `moved_surface_ids`); cap `RESTIR_MAX_MOVED_SURFACES = 64` in `crates/renderer/src/shader_constants_data.rs`.

## Description
The #6250 moved-surface set pushes one ID per moving draw *entity*, where "moving" means `previous_source != m || pose_dirty.contains(entity)`. `pose_dirty` is per `SkinnedMesh` entity, so each skinned part (body, head, hands, hair, eyes, armour) counts separately, and `byroredux/src/render/skinned.rs` (`try_mark_pose_dirty`) re-marks a part every frame its bone hash changes, which includes idle animation. Roughly ten animated NPCs exceed the cap of 64. `upload_moved_surface_ids` then truncates to the first 64 in draw-sort order and emits `log::warn!` unconditionally, i.e. every frame.

## Evidence
```rust
let kept = moved_surface_ids.len().min(RESTIR_MAX_MOVED_SURFACES as usize);
if moved_surface_ids.len() > kept {
    log::warn!("moved-surface set over capacity: {} movers, kept {} (#6250) — ...", ...);
}
```
There is no once-gate, and the commit cites no census for the value 64. (The existing doc comment on the function acknowledges over-cap ids are dropped and "still taint via the hit arm / the K-jitter re-roll a few frames later".)

## Impact
- About 60 warn lines per second for as long as a crowd is on screen (Whiterun market, the Strip, Megaton).
- The prev-occluder arm (first-moving-frame and teleport taint) is silently lost for an order-dependent subset of movers, in exactly the scenes the arm was added for. Overflowed movers still taint through the hit arm, so the image cost is bounded.

## Related
#6250 (closed), `bed162262`.

## Suggested Fix
- Warn once per overflow episode and re-arm when the count drops back under the cap.
- Expose the overflow count in `rt.integrity`.
- Size the cap from a populated-cell census, or collapse the per-part fan-out by keying both the set and `occluderSurface` on the actor.

## Completeness Checks
- [ ] **SIBLING**: GLSL side (`movedSurfaceSet` count clamp, `RESTIR_MAX_MOVED_SURFACES` define) and the 5 standalone shader copies stay in lockstep if the cap or layout changes
- [ ] **DROP**: If buffer sizing changes, `SceneBuffers::destroy` still frees every moved-surface buffer (see SAFE-2026-10-27-01)
- [ ] **TESTS**: A regression test pins the once-per-episode warn / the overflow counter

