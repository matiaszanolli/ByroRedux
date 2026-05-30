# #1349 — D4-02: GPU struct layout test function names are stale (288/260 vs 304/300)

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d4-02). GitHub is authoritative for live state — query `gh issue view 1349 --json state`._

**Severity**: LOW · **Dimension**: RT Lighting Pipeline (test-name drift) · **Source**: AUDIT_FNV_2026-05-30 (D4-02)

**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:53` (`gpu_camera_is_288_bytes` asserts 304) ; `crates/renderer/src/vulkan/material.rs:1159` (`gpu_material_size_is_260_bytes` asserts 300)

**Description**: Two GPU-layout test functions embed a byte count in their names that no longer matches the asserted value: `gpu_camera_is_288_bytes` asserts `size_of::<GpuCamera>() == 304`; `gpu_material_size_is_260_bytes` asserts 300. Both are deliberately "kept for grep continuity" (documented at gpu_types.rs:173 and material.rs:41), and the assertions track the live size correctly — so there is NO layout drift, only the names mislead.

**Evidence**: gpu_instance_layout_tests.rs:53 fn name `gpu_camera_is_288_bytes`, asserts 304. material.rs:1159 fn name `gpu_material_size_is_260_bytes`, asserts 300. Inline docs at gpu_types.rs:173 / material.rs:41 acknowledge the name is intentionally stale "for grep continuity".

**Impact**: Cosmetic. A reader scanning test names could misread the live struct sizes. No functional risk.

**Suggested Fix**: Rename to `gpu_camera_is_304_bytes` / `gpu_material_size_is_300_bytes` (lose grep continuity but gain accuracy) and update the referencing doc comments; or leave as-is (low priority).

## Completeness Checks
- [ ] **SIBLING**: If renamed, update all doc-comment references to the old names (gpu_types.rs:173, material.rs:41,65,81,1176,1239).
