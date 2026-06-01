# PERF-D9-NEW-01: LOD-ring boot upload — ~1200 serial fence-waits + ~1250 device-local sub-allocations

**Severity**: MEDIUM (one-time boot stall) · **Dimension**: GPU Memory / Streaming (merged PERF-D2-NEW-01 + PERF-D9-NEW-01)
**Location**: `byroredux/src/scene/world_setup.rs:753-761` → `byroredux/src/cell_loader/terrain_lod.rs:79-320` (`spawn_lod_ring`/`spawn_lod_block`) → `crates/renderer/src/mesh.rs:244-296`
**Status**: NEW (introduced this session by the distant-terrain LOD ring)

~500-600 non-hole blocks each call `upload_scene_mesh(staging_pool: None)` → `create_vertex_buffer` + `create_index_buffer`, each routing through `with_one_time_commands` which creates a fresh fence and blocks on `wait_for_fences` per submission = **2 synchronous GPU round-trips per block (~1000-1200 serialized fence-waits)** + ~1250 tiny (6-29 KB) device-local sub-allocations. Multi-hundred-ms to multi-second one-time boot stall, after all full-detail cells already loaded. Byte budget is fine (~22 MB); the cost is fence serialization + sub-allocation count/fragmentation.

**Fix**: (1) accumulate all block geometry into the global `pending_vertices`/`pending_indices` pool + single `rebuild_geometry_ssbo` (LOD draws read the global SSBO, so per-mesh buffers are unused) — collapses ~1250 sub-allocs → ~2 and ~1200 fence-waits → ~1; OR use `with_one_time_commands_reuse_fence` (texture.rs:569) + shared StagingPool. (2) defer the ring past first present (distant geometry; 1-2 frame pop-in invisible). Pairs with the Slice-2 streaming wiring.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
