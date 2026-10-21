# #6250: REN-M-02: The ReSTIR mover taint misses pixels a mover un-occludes on its first moving frame or after a teleport, and keys on the first ray hit rather than the occluder; those pixels keep the 1024-frame tail

**Labels**: medium,renderer,shaders,bug

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Ray Queries / Denoiser (ReSTIR direct-light EMA) · **Location**: `crates/renderer/shaders/triangle.frag (moverHit/prevTainted block)`

- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `moverHit` / `prevTainted` / `deep` block before `Lo += restirUnshadowedSum * accum`, and the `if (sr == 0)` capture of `selectedRayHitInstance`.
  - `crates/renderer/shaders/include/shadow_transport.glsl`: `traceShadowTransmittanceDetailed`, where `committedInstance` is the first hit.
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: `instance_moved`, `restir_rig_static`. From `3934f46d7`.
- **Status**: NEW
- **Description**:
  - Since `3934f46d7`, movers no longer clear `restir_rig_static` scene-wide. A parked camera with a mover anywhere sits in mode 3 (rig static), or mode 2 when nothing moved, and both run the deep tail.
  - A pixel leaves the deep tail only if this frame's selected shadow ray hits an instance flagged `INSTANCE_FLAG_MOVED_THIS_FRAME`, or if it was already tainted. Three holes:
    1. **First moving frame.** `instance_moved` is `previous_source != m`. On the first frame an occluder moves, the pixels it uncovers already miss it and were never tainted, because it was static the frame before. They blend toward lit at α = max(1/histLen, 0.002), about a 346-frame half-life, so a sliver of the old shadow edge lingers. The same applies to every pixel uncovered by a teleport (one frame flagged, then static). An instance that *vanishes* is handled by the set-change gate; one that *jumps* is not.
    2. **First hit, not the occluder.** `committedInstance` is set on the first committed hit, including alpha-tested layers that `rayHitHasCoverage` then skips (foliage, fences) and effect cards. An NPC standing behind a fence relative to the light reports the fence, which is static, so its shadow is never tainted.
    3. **Sample 0 only.** Only `sr == 0` records the hit. With `directShadowSamples > 1`, penumbra pixels whose other cone taps hit the mover stay untainted.
- **Evidence**: the code paths above. The skill states that no test pins the taint, and none does.
- **Impact**:
  - With the camera parked, an NPC that starts walking, a door that starts swinging, or a scripted reposition leaves ghost shadow strips that take seconds to clear. That is the ghosting #4942 removed with the scene-wide gate.
  - Hole 2 makes it systematic for actors seen through foliage or fences.
  - Not measured; the `restir_history` view (orange = tainted) is the on-device check.
- **Related**: #5369, #4942, `3f8194117` (SVGF made the same scene-wide → per-pixel move), REN-M-01.
- **Suggested Fix**:
  - Taint on the *previous* frame's occluder as well. For example, store the hit's stable surface ID or instance in the reservoir, and taint when last frame's recorded occluder carries the moved flag this frame. Or keep `INSTANCE_FLAG_MOVED_THIS_FRAME` set for `RESTIR_MOVER_SETTLE_FRAMES` after motion stops and for one frame before (the previous-frame transform is already uploaded).
  - Record the instance that actually returned zero visibility (the covered or opaque hit), not the first committed candidate. Consider OR-ing the flag across all `sr` samples.
  - Pin with a source-shape test, since the GPU behaviour needs RenderDoc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
