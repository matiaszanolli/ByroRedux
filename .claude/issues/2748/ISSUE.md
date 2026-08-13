# REN-D3-2026-08-12-01: GpuInstance's five-mirror lockstep guard is presence-only, unlike its GpuLight/GpuMaterial siblings

- **Severity**: MEDIUM
- **Dimension**: 3 — GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (`every_shader_struct_gpu_instance_names_material_kind_slot`); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuInstance` doc comment)

## Description
`GpuInstance` has the largest hand-mirror fan-out of any GPU struct — 5 declaration sites (`crates/renderer/shaders/include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`) — and the documented recurring trap is precisely a mirror reading wrong offsets (#785 / #1498). Yet its only cross-mirror guard is a `src.contains()` needle check: it asserts each file declares `struct GpuInstance`, contains three field-name strings, does not contain `"uint _pad1"`, and does not re-introduce 26 retired names. It never compares the mirrors to each other, never compares them to the Rust struct, and never checks field **order** or **completeness**. Compounding it, the `gpu_types.rs` doc instructs contributors to "update the expected suffix in the assertion and rename the sentinel" — there is no expected-suffix logic and no sentinel in the test. The test's own name refers to a field that was removed from the struct.

## Evidence
Fields with **no** coverage in any mirror: `model`, `textureIndex`, `boneOffset`, `vertexOffset`, `indexOffset`, `vertexCount`, `flags`, `ior`, `avgAlbedoR/G/B`, `_reserved`. Worked failure the guard cannot see: delete `float ior;` from `ui.vert`'s mirror — `skinnedVertexAddress` re-aligns to 8, **stride stays 128 B**, so no stride check catches it, while `surfaceId` now reads `avg_albedo_b`'s bytes, and every assertion still passes. Its two siblings already have the guard it lacks, in the *same file*: `gpu_light_glsl_copies_stay_in_lockstep` (#1916) does full stripped-body equality; `gpu_material_glsl_field_order_matches_rust_struct` (#1657) does a full ordered Rust↔GLSL comparison. CI does not backstop it — `scripts/check-shader-artifacts.sh` proves each `.spv` is reproducible from its GLSL, which says nothing about whether that GLSL matches the Rust struct; SSBO `ArrayStride` has no reflection helper at all.

Spot-checked against live code during publish: `gpu_instance_layout_tests.rs:203-233` confirmed the test only performs `src.contains(...)` checks across the 5 mirrors, no ordered field comparison.

## Impact
A one-line edit to any of the 4 standalone mirrors can silently desync per-instance reads while `cargo test` and the CI shader job both stay green. Blast radius by mirror: `triangle.vert` = every drawn vertex; `water.vert` = every water plane; `caustic_splat.comp` = every caustic deposit; `ui.vert` = the UI overlay. **No drift exists today** — all 5 mirrors and all 6 `.spv` were verified byte-identical this run.

## Related
#1916 (the pattern to copy), #1657, #2463 (`GpuTerrainTile`, same class one rung lower — Cluster C), #785 / #1498 (the historical incidents), #2164 (`_pad_id0` → `ior`), #2433 (a different GpuInstance test-hygiene gap, same file), #2483 (stale byte-size comments, same neighborhood — not a duplicate, filed separately).

## Suggested Fix
Add `gpu_instance_glsl_copies_stay_in_lockstep`, modelled directly on `gpu_light_glsl_copies_stay_in_lockstep`: `extract_struct_body` + `strip_struct_body` across all 5 sites, assert byte-identical stripped field lists, then reuse `parse_rust_struct_fields` / `normalize_ident` to assert the shared list matches the Rust field order. Fix the `gpu_types.rs` protocol comment to describe the real mechanism and rename the test off `material_kind`.

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2748
