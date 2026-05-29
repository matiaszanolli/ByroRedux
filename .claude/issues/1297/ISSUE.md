# #1297 — DIM12-A-01: SkinSlot.vertex_count not reconciled vs per-frame mesh.vertex_count — latent OOB compute write

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_RENDERER_2026-05-28_DIM12). GitHub is authoritative for current state._

**Severity**: LOW (latent / not currently reachable) · **Dimension**: GPU Skinning — compute dispatch
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-28_DIM12.md` (finding DIM12-A-01)

**Location**: `crates/renderer/src/vulkan/context/draw.rs:913-980,1014-1053`; `crates/renderer/src/vulkan/skin_compute.rs:74-77` (doc invariant), `:116-118` (dead `vertex_count()` accessor).

**Issue**: The first-sight loop calls `create_slot` only when the slot is absent; an existing slot is reused verbatim with no comparison of live `mesh.vertex_count` vs `slot.vertex_count`. The dispatch pushes `push.vertex_count = mesh.vertex_count` into the existing slot's output buffer (sized at alloc time) and the shader writes `outputVertexData[vid …]` for `vid in 0..push.vertex_count`. If an entity's `mesh_handle` is remapped to a larger-vertex-count mesh, the write runs past `output_size`. The shader bounds-check gates on `push.vertex_count`, not the slot's allocated capacity. `SkinSlot::vertex_count()` exists but has zero callers.

**Risk**: OOB compute write into the skinned-vertex SSBO. **Not reachable today** — per the #907 comment no in-engine path remaps `entity_id → mesh` between frames. The BLAS side already guards this exact remap (`validate_refit_counts`) but the compute dispatch runs *before* the refit guard → asymmetric protection.

**Suggested fix**: In the first-sight loop, when the slot exists, compare `slot.vertex_count() != vertex_count` and on mismatch `destroy_slot` + recreate (+ drop the skinned BLAS). Makes the `SkinSlot` doc invariant load-bearing and activates the dead accessor — symmetric with `validate_refit_counts`.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in the related path (refit vs batched-build; raster vs compute)
- [ ] **DROP**: if Vulkan objects change, Drop impl still correct
- [ ] **CANONICAL-BOUNDARY**: if the fix touches `material_translate.rs::translate_material` / `Material::resolve_pbr` / import-walk emitter params, per-game logic stays at the NIFAL parser→Material boundary (see /audit-nifal). _(N/A for these skinning/accel findings)_
- [ ] **TESTS**: regression test added for this specific fix
- [x] **SIBLING**: BLAS side guarded via `validate_refit_counts`; this makes the compute side symmetric.
