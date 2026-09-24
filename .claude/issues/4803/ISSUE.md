# #4803: PERF-D3-2026-09-23b-03: Content-shared geometry hashes each fresh shareable submesh's full vertex and index bytes three times

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D3-2026-09-23b-03)

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (the cost lands on cell load)
- **Location**: `crates/renderer/src/mesh/geometry_sharing.rs:12-18,51,82`; `byroredux/src/cell_loader/spawn/mesh_instance.rs:767,775,842`; `byroredux/src/scene/nif_loader.rs:1179,1217`
- **Status**: NEW (`b9e961eeb`, extended to the NPC path by `ff1b48d7c`)
- **Description**: The same bytes are SipHashed three times: in `acquire_matching_scene_mesh`, in `fresh_by_content.entry(...)`, and in `register_scene_geometry_for_sharing` (twice on the NPC path). `same_geometry` verifies every candidate anyway, so one fingerprint would do.
- **Impact**: *est.* 0.44 ms per MB of fresh shareable geometry on the main thread (~9 ms for a cell with 20 MB of fresh geometry). The sharing itself is a memory win.
- **Suggested Fix**: Compute the fingerprint once and thread it through; optionally use a faster byte hasher.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
