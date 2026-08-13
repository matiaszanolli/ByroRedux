# REN-D1-01: docs/engine/renderer.md's AS section contradicts the code on three points, including the #516 off-screen-occluder TLAS invariant

- **Severity**: MEDIUM
- **Dimension**: 1 — AS Correctness (code/doc divergence; the **doc** is the wrong side)
- **Location**: `docs/engine/renderer.md` — the "TLAS per frame", "Per-skinned-entity BLAS (M29)" and instance-staging bullets. Code sides: `crates/renderer/src/vulkan/acceleration/tlas.rs`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`.

## Description
Three statements describe behaviour the code deliberately does *not* have. (1) *"TLAS per frame: rebuilt every frame … **with frustum culling against the camera**"* — the code says the opposite and says so as a fixed bug: the only TLAS-eligibility gate is `draw_command_eligible_for_tlas` (`crates/renderer/src/vulkan/acceleration/predicates.rs`), which reads `in_tlas && !is_water` with no frustum term. (2) *"Per-skinned-entity BLAS (M29): keyed by `EntityId`, built sync at cell load"* — the synchronous per-NPC builder was deleted under #1141; `build_skinned_blas_batched_on_cmd` is the sole entry point and records onto the per-frame command buffer at first sight. (3) *"a two-stage barrier chain (HOST_WRITE→TRANSFER_READ→**AS_READ**)"* — the second barrier's `dst_access_mask` is `SHADER_READ`, which is precisely what #1436 changed.

## Evidence
`build_tlas_instances` carries the #516 rationale verbatim (*"off-screen occluders stay in the TLAS so on-screen fragments' shadow / reflection / GI rays hit them"*); `blas_skinned.rs`'s module doc names the batched builder the *"Sole entry point"*; `tlas.rs`'s `build_tlas` uses `.dst_access_mask(vk::AccessFlags::SHADER_READ)` with an in-code comment explaining that `ACCELERATION_STRUCTURE_READ_KHR` is what sync-validation flags as a copy→build RAW hazard. `docs/engine/memory-budget.md`'s AS section was checked against the same code and is **accurate** — the drift is confined to `renderer.md`.

Spot-checked against live code during publish: `predicates.rs:434-436` confirmed `in_tlas && !is_water` only; `docs/engine/renderer.md:382-390` confirmed still reads "frustum culling against the camera", "built sync at cell load", and "AS_READ".

## Impact
Doc-driven regressions. Item 1 is the dangerous one: it reads as a design statement, it sits in the doc `_audit-common.md` points readers at for BLAS/TLAS lifecycle, and acting on it silently deletes RT contributions from every off-screen occluder. Item 3 would re-trip the sync-validation hazard #1436 fixed.

## Related
#516, #1141, #1436.

## Suggested Fix
Three edits to `docs/engine/renderer.md` — drop "with frustum culling against the camera" and state the #516 rule (frustum gates `in_raster` only); change "built sync at cell load" to "built on the per-frame command buffer at first sight"; change the barrier chain to `HOST_WRITE→TRANSFER_READ→SHADER_READ @ AS_BUILD` and cite #1436.

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2746
