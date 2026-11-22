# #7246: INC-2026-11-22-04: About ten small heap allocations per exterior frame in the patch BLAS record path

**Labels**: low,renderer,performance,terrain-exterior,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-A-03**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: audit-performance Dim 1 / Dim 5 (per-frame allocations in the record path)
- **Location**: `crates/renderer/src/vulkan/terrain_patch.rs:803-831` (`blas_requests` returns a fresh `Vec`); `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs:264-411` (`live`, `plan_input`, the planner's returned `Vec`, `regions`, `recorded`, `geometries`, `ranges`, `infos`, `range_refs`)
- **Changed in**: `blas_terrain_patch.rs` and `terrain_patch.rs`, `ff339f658`
- **Status**: NEW
- **Description**: Every frame with at least one resident patch allocates the request `Vec` plus nine more in `record_terrain_patch_blas`. The renderer's other recording paths keep such scratch on `self.scratch.*` and shrink it through the existing shrink policy; this one does not. The cost is microseconds at 16-24 elements, so it is hygiene, not a hitch.
- **Evidence**: `Vec::with_capacity(...)` / `.collect()` sites at the lines above; none are stored on the manager or the pass. With zero requests nothing is allocated (empty `collect` does not allocate and `live` is `with_capacity(0)`), so interiors pay nothing.
- **Impact**: Allocator traffic in a hot recording path, exterior only. Not measured.
- **Related**: #2923 (hot-path discipline).
- **Suggested Fix**: Keep the vectors on `TerrainPatchBlasSet` (cleared each frame), and have `blas_requests` fill a caller-owned buffer.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
