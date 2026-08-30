# #3665 — PERF-D4-2026-08-30-03: the bone-world staging memcpy and its GPU copy are O(high-water skin slots), not O(dirty slots), and the two-thirds of #1794 that was left undone has no live tracker

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D4-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,memory,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3665

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:231-262`
  (`upload_bone_worlds`), `:269-315` (`record_bone_world_copy`), `:216-229` (the deferral
  comment), `byroredux/src/render/skinned.rs:138-149` (`required_slots =
  (pool.max_used_slot() + 1) * MAX_BONES_PER_MESH`),
  `crates/core/src/ecs/resources/skin_slot_pool.rs:355-357` (`max_used_slot`),
  `crates/renderer/src/vulkan/context/draw.rs:2473-2497` (the #1811 streak gate)
- **Status**: NEW (**Regression-adjacent to CLOSED #1794**: that issue's own "three-fold" cost
  was closed with only the CPU-fill third fixed; the code comment says so and names no successor)
- **Description**: `bone_world` is a sparse array indexed by `slot_id × MAX_BONES_PER_MESH`
  (144), sized to `(max_used_slot + 1) × 144` matrices. `upload_bone_worlds` memcpys **the whole
  array** into the per-FIF staging buffer and `record_bone_world_copy` issues a single
  `cmd_copy_buffer` for the same byte count, which `draw.rs` then also uses to size the
  `skin_palette.comp` dispatch (`bone_input_upload_bytes`). The only gate is #1811's
  `clean_skin_frames` streak: it skips the trio entirely while **no** pose changed, and runs the
  full-width trio as soon as **any** pose changed. There is no per-slot granularity.

  So one walking NPC in a scene whose pool high-water mark is 260 slots costs
  `261 × 144 × 64 B` ≈ 2.4 MB of host memcpy **plus** 2.4 MB of device copy **plus** a
  proportionally-sized compute dispatch, every frame, to deliver 9 KB of changed data. The
  information needed to narrow it already exists and is already threaded to the renderer:
  `SkinSlotPool::pose_dirty()` is passed into `FrameInputs.pose_dirty` (`app_frame.rs:540`) and
  is consumed per entity by `record_skinned_blas_refit` (`skinned_blas_refit.rs:412`, `:613`) —
  the bone-world copy is the one consumer that ignores it.

  Two independent multipliers, not one. `upload.rs:216-229` names only the *within-slot* one
  ("the full `MAX_BONES_PER_MESH`-wide stride … most of which is padding tail beyond that
  entity's own bone count") and defers it as "the remaining two-thirds of the issue's
  three-fold cost". The *across-slot* multiplier — copying clean slots at all — is not named in
  that comment, in #1794's body, or anywhere else. `max_used_slot` is a high-water figure that
  `sweep` only compacts when the freed set includes the contiguous top of the range, so a single
  surviving high-numbered entity keeps the full span alive.
- **Evidence**:
  ```rust
  // byroredux/src/render/skinned.rs:148-149
  let required_slots = (pool.max_used_slot() as usize + 1) * MAX_BONES_PER_MESH;
  bone_world_out.resize(required_slots, IDENTITY_4X4);
  ```
  ```rust
  // upload.rs:238-259 — no dirty-slot narrowing; whole array, one range
  let count = bone_world.len().min(MAX_TOTAL_BONES);
  let byte_size = (std::mem::size_of::<[[f32; 4]; 4]>() * count) as vk::DeviceSize;
  std::ptr::copy_nonoverlapping(bone_world.as_ptr() as *const u8, world_mapped.as_mut_ptr(), byte_size as usize);
  world_buf.flush_range(device, 0, byte_size)?;
  self.bone_input_upload_bytes[frame_index] = byte_size;
  ```
  ```rust
  // crates/core/src/ecs/resources/skin_slot_pool.rs:355-357
  pub fn max_used_slot(&self) -> u32 { self.next_slot.saturating_sub(1) }
  ```
  Closed #1794's own impact line, for the same workload: *"≥261 slots × 9216 B ≈ 2.4 MB/frame
  ≈ 144 MB/s sustained host-write + flush + GPU copy at 60 fps, most of it identity padding.
  Full-pool worst case (1365 slots) ≈ 12.6 MB/frame ≈ 755 MB/s."* Those figures still stand for
  the memcpy + copy halves; only the CPU identity-refill was removed (verified INTACT this
  session by sibling Dimension 1).
- **Impact**: The dominant per-frame host→device transfer in any scene with actors — larger
  than the instance SSBO — and it scales with skinned-entity *density and slot fragmentation*
  rather than with how much actually animated. It is also the sizing input for the
  `skin_palette.comp` dispatch, so the wasted bytes buy wasted GPU threads too. Nothing renders
  incorrectly. The process problem is the same one #1794 was filed about: a code comment
  deferring to work that no open issue owns, now one level deeper (it defers to a closed issue's
  unfinished remainder).
- **Related**: #1794 (closed, 1/3 delivered), #1811 (the frame-streak gate), #1284 (the
  `MAX_TOTAL_BONES` bumps), #2923 (`pose_dirty` kept `FxHashSet` across the crate boundary —
  the very set this path should be reading), M29.5/M29.6.
- **Suggested Fix**: Two separable steps. (1) Cheap and self-contained: build per-slot
  `vk::BufferCopy` regions from `pose_dirty` + `SkinSlotPool::entity_to_slot`, with a per-FIF
  "slot last written in this slot's buffer" stamp so a slot dirtied once is refreshed into both
  FIF copies (the same `MAX_FRAMES_IN_FLIGHT` safety margin `clean_skin_frames` already uses).
  (2) The #1794 remainder: plumb per-slot `skin.bones.len()` into `FrameInputs` so each region
  covers only the real prefix. Either way, re-file the untracked remainder so the comment stops
  pointing at a closed issue.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
