# #7316 ECS-D1-02: `gather_export_scene` (Mitsuba export) holds storages in an order the render collector inverts

Labels: medium,ecs,concurrency,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: MEDIUM. Lock-order inversions have a HIGH floor, but this was downgraded: it is a one-shot, opt-in (`--export-mitsuba`), main-thread dev tool that runs between frames. It cannot deadlock, and no test reaches it.
- **Dimension**: 1 Lock Ordering
- **Location**: `byroredux/src/cornell/mitsuba_export.rs` `gather_export_scene`.
- **Status**: NEW. Closed #6618 fixed the same class in `material_dump::material_rows`, not here.
- **Description**: the function holds `MaterialTextureHandles → TextureHandle → AlphaBlend → SkinnedMesh → MeshHandle → GlobalTransform → Material`, then `LightSource → GlobalTransform` and `Camera → GlobalTransform`. The render collector takes `GlobalTransform → MeshHandle → TextureHandle → …`. Several 2-cycles result, for example `GlobalTransform ↔ MeshHandle` and `MeshHandle ↔ TextureHandle`.
- **Impact**: with the detector on, the first exported frame panics and the Cornell reference workflow aborts. Nothing happens without the environment variable.
- **Suggested Fix**: read one storage at a time into owned collections, as `material_rows` does since #6618.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
