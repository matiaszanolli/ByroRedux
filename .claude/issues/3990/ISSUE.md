# #3990 — REN-2026-09-06-D3-02: `upload_instances`' `unsafe` SAFETY argument states the wrong field types, omitting exactly the three fields that could introduce implicit padding

**Labels**: medium, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::upload_instances`), `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`hash_instance_slice`)
- **Status**: NEW
- **Description**: `upload_instances` reinterprets the `&[GpuInstance]` slice as raw bytes via `std::ptr::copy_nonoverlapping`, justified by the comment *"SAFETY: GpuInstance is `#[repr(C)]` with plain f32/u32 fields."* That has been false since #2219: `GpuInstance` carries three `u64` fields (`skinned_vertex_address`, `morph_delta_address`, `morph_weight_address`), which raise the struct's alignment to 8 and are the *only* reason implicit padding could ever appear in it. The safety argument therefore asserts the absence of the one hazard it needs to rule out by describing a struct shape that no longer exists. `hash_instance_slice`'s companion doc has the same shape (`"f32 / u32 / packed-vec4 fields"`).

  Separately, this is the layer the `NoUninit` trait (#3761 / SAFE-2026-08-30-D4-01) was added to enforce rather than argue in prose — yet of the ten upload paths in `scene_buffer/upload.rs`, only the two UBO writers (`upload_camera`, `upload_dalc`) route through the `NoUninit`-bounded `write_mapped`. All eight SSBO paths (`upload_lights`, `upload_bone_worlds`, `upload_pending_bind_inverses`, `upload_instances`, `upload_previous_models`, `upload_materials`, `upload_indirect_draws`, `upload_terrain_tiles`) still hand-roll `copy_nonoverlapping` with prose. `GpuInstance` and `GpuMaterial` — the two most churn-prone structs in the workspace, five and one GLSL mirrors respectively — are among the eight.
- **Evidence**:
  - `gpu_types.rs`: `pub skinned_vertex_address: u64`, `pub morph_delta_address: u64`, `pub morph_weight_address: u64`.
  - `unsafe impl NoUninit for …` exists for `GpuCamera`, `GpuDalcCube`, `GpuSelectedRayProbe`, `GpuWaterParams`, `GpuFogVolume`, `Vertex`, `UiVertex` and others — but **not** for `GpuInstance` or `GpuMaterial`.
  - Verified no implicit padding exists today: `surface_id`@108 → `skinned_vertex_address`@112 is 8-aligned, and 160 % 8 == 0. **This finding is about the guard, not a live UB.**
- **Impact**: A future field insertion that lands a `u32` immediately before one of the `u64`s at an odd 4-byte offset introduces 4 bytes of implicit padding. `gpu_instance_is_160_bytes_std430_compatible` *would* catch the size change, but nothing would flag that the byte view now contains uninitialised bytes — which is UB in both the upload copy and `hash_instance_slice`'s dirty-gate hash, and reaches the GPU as garbage in whichever std430 lane the padding lands. The stale comment is what removes the reader's chance to notice; #3761 exists precisely because "`Copy` alone does not rule this out".
- **Related**: #3761 / SAFE-2026-08-30-D4-01 (introduced `NoUninit`); #2219, #3231 (added the `u64` fields the comment predates).
- **Suggested Fix**: Add `unsafe impl NoUninit for GpuInstance {}` and `for GpuMaterial {}` with a SAFETY note naming the `u64` fields and the 8-byte alignment argument, then convert `upload_instances` / `upload_materials` to `write_mapped`. At minimum, correct both comments to say "`#[repr(C)]` scalars plus three `u64`s, positioned so no implicit padding appears".

---

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
