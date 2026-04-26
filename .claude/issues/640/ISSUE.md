# Issue #640: SH-2: caustic_splat.comp ray query has no flags AND no rtEnabled gate

**File**: `crates/renderer/shaders/caustic_splat.comp:229`
**Dimension**: Shader Correctness

`rayQueryInitializeEXT(rq, topLevelAS, 0u, 0xFFu, …)` passes `0u` as ray flags — no `gl_RayFlagsOpaqueEXT`, no `gl_RayFlagsTerminateOnFirstHitEXT`. Driver runs full closest-hit traversal across the 1000-unit reach for every (light × pixel) pair. Compounding: the whole compute pass has no `sceneFlags.x > 0.5` early-out — pays full TLAS-query cost even when RT is disabled at the camera UBO.

This is the same closest-hit cost multiplier #420 fixed for triangle.frag. caustic_splat.comp was missed.

**Fix**:
- Add `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT` to the ray-flags arg.
- Gate the `for (uint li …)` loop on `if (sceneFlags.x < 0.5) return;` after the meshId reject (~line 171).
- CPU-side: skip the dispatch when RT is disabled (mirrors the AccelerationManager enable bit).

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
