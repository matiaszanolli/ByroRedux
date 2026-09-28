# #4943: REN-D7-2026-09-27-02: SVGF's "scene unchanged" signal ignores draw-set changes (spawn/despawn/enable/disable of rigid instances), so a parked camera keeps ~1/256 α over GI that changed

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4943
- **Labels**: medium,renderer,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D7-2026-09-27-02**._

- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite
- **Location**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: the draw loop (`let previous_source = … previous_rigid_models.get(&draw_cmd.entity_id).unwrap_or(m)`; `rigid_instance_moved |= previous_source != m;`) and `let caustic_scene_static = !rigid_instance_moved && pose_dirty.is_empty() && caustic_scene_key == self.prev_caustic_scene_key;` → `next_svgf_temporal_alpha(self.svgf_recovery_frames, caustic_history_valid)`.
  - Consumer: `svgf_temporal.comp` (`floorC = params.w > 0.5 ? 0.0 : params.x`).
- **Status**: NEW (#4046 added the light rig and did not cover the instance set).
- **Description**:
  - `scene_static` is built from rigid instances that **moved** (a first-sight instance compares `m` to itself and counts as unmoved), skinned poses, caustic-source placement and the light rig.
  - An instance that **appears** (first sight) or **disappears** (present in `previous_rigid_models`, absent this frame) changes nothing in that signal.
  - With the camera parked, `params.w = 1` removes the 0.2 α floor. `histAge` is capped at 255, so the neighbouring surfaces' GI then converges as an EMA with α ≈ 1/256 (τ ≈ 256 frames).
  - Pixels the object used to cover are fine: their mesh ID changes and history is rejected. The walls, floor and tabletop around it keep the object's colour bleed and contact occlusion in their indirect term for several seconds.
  - Streaming and cell loads are covered separately by `signal_temporal_discontinuity`. Gameplay-driven set changes are not: picking up or looting an item, `disable`/`enable`, corpse cleanup, and the spawning of any rigid object.
  - Emissive or material animation (no transform change) is likewise invisible to the key. This is a lesser case.
- **Evidence**: The instance set is not folded into `caustic_scene_key` anywhere. `current_rigid_models` / `previous_rigid_models` are swapped each frame in `draw.rs` (`std::mem::swap(&mut self.history.previous_rigid_models, &mut current_rigid_models)`), so the set difference is available for free.
- **Impact**: A multi-second GI ghost after the scene changes in front of a stationary player. This is the typical situation for looting, which happens with a parked camera.
- **Related**: #4046, #3995, #2468.
- **Suggested Fix**: In the loop, treat a first-sight rigid instance (`previous_rigid_models.get(..) == None` while `!camera_cut && !suppress_rigid_history`) as `rigid_instance_moved`. After the loop, treat `previous_rigid_models.len() != current_rigid_models.len()` (or any missing key) as a removal. Both are O(1) per draw on data already in hand.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
