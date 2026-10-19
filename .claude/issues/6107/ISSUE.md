# REN-SD-2026-10-19-02: NPC and loose-NIF spawn builds a static BLAS for every skinned mesh, and the TLAS never reads it

Labels: medium,renderer,performance,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6107

**Severity**: MEDIUM (VRAM billed against the BLAS budget, plus synchronous fence-waited builds on the streaming path) · **Dimension**: AS Correctness / Skinning

## Location
- `byroredux/src/scene/nif_loader.rs` `spawn_nif_mesh` (`blas_specs.push((mesh_handle, ..))` gated only on `for_rt && fresh_source`) and the caller's `ctx.build_blas_batched(&blas_specs)`
- `crates/renderer/src/vulkan/acceleration/tlas.rs` `build_tlas_instances` (skinned draws, `bone_offset != 0`, read only `self.skinned_blas`)
- `crates/renderer/src/vulkan/context/resources.rs` `restore_missing_static_blas_for_draws` (filters `cmd.bone_offset != 0`)

## Description
`spawn_nif_mesh` queues a static (mesh-handle keyed) BLAS for every freshly uploaded mesh with `for_rt`, including meshes that carry `mesh.skin`. A skinned mesh that binds (`attach_nif_skin_binding` succeeds) is drawn with `bone_offset != 0`; `build_tlas_instances` takes the per-entity skinned BLAS for those draws and never consults `blas_entries[mesh_handle]`. The bind-pose static BLAS built at spawn is dead weight: the rigid recovery pass skips skinned draws and nothing else reads it. The only case that would use it is a skinned mesh that fell back to rigid drawing (bone cap exceeded or skin-slot pool full), and for that case the recovery pass would build it lazily anyway.

## Evidence
`tlas.rs`: `if draw_cmd.bone_offset != 0 { let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) ... } else { ... self.blas_entries.get_mut(mesh_handle) ... }`; `resources.rs`: `if cmd.bone_offset != 0 || !draw_command_eligible_for_tlas(cmd) { return None; }`. The missing-skinned arm `continue`s and counts `missing_skinned_blas` (no static fallback). `build_blas_batched` filters only on `mesh.rt_capable`. Cell-loader placement attaches no `SkinnedMesh` (#2440), so only the `nif_loader` path is affected.

## Impact
Every unique skinned NPC mesh (per-NPC FaceGeom heads, hair, shared bodies and outfits) carries a compacted BLAS that counts in `static_blas_bytes` and `shared_blas_scratch_peak`. `build_blas_batched` is a synchronous submit with host fence waits and a compaction-size readback per call, and `spawn_nif_mesh` runs once per NPC part unit (`restore_missing_static_blas_for_draws` documents ~0.2-0.5 ms per mesh for the same primitive). Skinned-handle BLAS never get their LRU stamp bumped so they are evicted first, but eviction forces a full TLAS rebuild. Not measured; an NPC-dense interior with unique heads is the worst case.

## Related
#4180 (recovery between frames), #3840 (pending bytes), `crates/renderer/src/vulkan/acceleration/blas_static.rs` `shared_blas_scratch_peak` users.

## Suggested Fix
In `spawn_nif_mesh`, skip the `blas_specs.push` when `skin_attached` will be true (or `mesh.skin.is_some()` and the bone count fits), leaving the rigid-fallback case to the lazy recovery pass. Pass `for_rt` unchanged so skinned-output BLAS input flags stay as they are. Add a source-shape test that pairs the push with the skin gate.

Source: docs/audits/AUDIT_RENDERER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
