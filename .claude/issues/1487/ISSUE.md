## Finding REN2-02 — Renderer Audit 2026-06-11

- **Severity**: HIGH
- **Dimension**: Acceleration Structures / GPU Skinning (Dims 8 + 12, independently converged)
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:180` (shared transform site; `bone_offset` check at `:132` only selects the BLAS device address); vertex space: `crates/renderer/shaders/skin_vertices.comp:117,137-143`; matrix source `byroredux/src/render/static_meshes.rs:351,553,573`
- **Status**: NEW — pre-existing since M29 Phase 2, independent of the camera-relative delta. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

Skinned BLAS geometry is built/refit from the `skin_vertices.comp` output buffer, which is **already absolute world** (placement included via bone GlobalTransforms — "skinned meshes encode the world transform through the bone palette", `skin_vertices.comp:117`). The TLAS instance for `bone_offset != 0` draws nevertheless falls through to the shared `column_major_to_vk_transform(&draw_cmd.model_matrix)` — the mesh entity's absolute GlobalTransform (never identity for placed actors; mesh entities are parented under the REFR placement root, `spawn.rs:213-218`). The placement is applied twice.

The "skinned draws carry identity model_matrix" hypothesis is disproven: `static_meshes.rs:351` emits `transform.to_matrix()` for all draws with no skinned override (`:553` `model_matrix`, `:573` `bone_offset` emitted together). Raster was unaffected pre-delta only because `triangle.vert` ignores `inst.model` in the skinned branch.

## Impact

The RT presence (shadow caster, reflection/GI subject) of every placed skinned actor sits at `R·w + t` instead of `w` (≈2× placement displacement near identity rotation): actors cast no shadow at their visual location and a phantom occluder exists elsewhere. Affects all games' NPCs since M29 Phase 2, at any render origin.

Severity note: the `_audit-severity` "TLAS build with wrong geometry/address → CRITICAL" row was considered; rated HIGH because geometry and addresses are valid — the instance transform is wrong, producing mis-placed (not corrupt) RT geometry and no crash path.

## Suggested Fix

Emit an identity `VkTransformMatrixKHR` for TLAS instances with `draw_cmd.bone_offset != 0` (one branch at `tlas.rs:180`). Verify at runtime via a `byro-dbg` attach on an FNV/Skyrim NPC cell (actor shadow position) — the fix itself is code-inspectable; no RenderDoc-dependent sync change involved.

## Related

REN2-01; the prior audit's "Dim 12 clean" verdict covered sync/layout, never the space-convention × TLAS-transform composition — coverage gap, not regression.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files — TLAS refit path, skinned BLAS rebuild path
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer
- [ ] **TESTS**: Regression test added for this specific fix

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
