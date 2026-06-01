# PERF-D2-NEW-02: LOD pool-cap headroom verified safe but not compile/test-asserted (retune could erode silently)

**Severity**: LOW · **Dimension**: GPU Memory (PERF-D2-NEW-02)
**Location**: `crates/renderer/src/mesh.rs:27-32` (caps), `:308-348` (check_pool_growth); ring sizing `byroredux/src/cell_loader/terrain_lod.rs:41-53`
**Status**: NEW

Confirmed clean today: worst-case LOD ring uses ~4.5% verts / ~6% indices of the pool soft caps (~22× headroom). But the caps are bare consts with no compile/test-time assertion that `(2·LOD_RADIUS_BLOCKS+1)² × verts_per_block < VERTEX_POOL_SOFT_CAP`. A future LOD retune (STRIDE=4 quadruples verts/block; LOD_RADIUS_BLOCKS=24 quadruples block count) could silently approach the cap, surfacing only as a one-shot runtime warn deep in a boot log.

**Fix**: add a `#[test]` (or `const _: () = assert!(...)`) in `terrain_lod` pinning the worst-case ring footprint against the published soft caps, so a retune that erodes headroom fails CI.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
