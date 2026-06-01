# PERF-D4-NEW-02: billboard_system arms per-billboard GlobalTransform dirty every frame — defeats static bounds fast path

**Severity**: MEDIUM · **Dimension**: ECS Query Patterns (PERF-D4-NEW-02)
**Location**: `byroredux/src/systems/billboard.rs:47-55`; consumed at `byroredux/src/systems/bounds.rs:118-130`
**Status**: NEW

`billboard_system` runs every frame with no change-detection gate and `get_mut`s every billboard's GlobalTransform (writing rotation), each push arming the TRACK_CHANGES dirty Vec. `world_bound_propagation` then drains a dirty set the size of the billboard population every frame and re-runs pass-1 leaf-bound composition for each — defeating the incremental-bounds static-cell fast path (landed this session) in any billboard-heavy cell (vegetation impostors, sprite quads). For a sphere bound, rotation is irrelevant to center/radius unless `local.center != ZERO`.

**Fix**: gate billboard_system on camera motion (cache last cam_pos/forward, early-return if unchanged — billboards only need re-rotation when the camera moved, exactly when transform_propagation already marks the camera dirty), OR skip the dirty-mark when `LocalBound.center == ZERO`. Camera-motion gating is cleanest.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
