# 3241: SAFE-D6: material.rs points ui.vert lockstep test at gpu_instance_layout_tests.rs, moved to shader_contract_tests.rs

**Severity**: LOW · **Dimension**: Safety Dimension 6 (R1 Material Table Layout Soundness) · **Report**: `docs/audits/AUDIT_SAFETY_2026-08-23.md` (SAFE-D6-NEW-03)

## Description

The doc comment on `GpuMaterial` (`crates/renderer/src/vulkan/material.rs:65-67`) names the file backing the `ui.vert` lockstep guard as `` `scene_buffer/gpu_instance_layout_tests.rs` ``. That was accurate when the comment was written (2026-07-25, `2cb86be5a`), but the 2026-08-20 test-file split (`f5abee08`) moved this specific test into `scene_buffer/shader_contract_tests.rs` without updating the cross-reference.

## Evidence

```
material.rs:65-67:
/// MUST NOT mirror the struct or index the material buffer — the
/// test `ui_vert_reads_texture_index_from_instance_not_material_table`
/// (`scene_buffer/gpu_instance_layout_tests.rs`)
```
Actual current location: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:297`.

## Impact

No runtime effect — the test itself is intact and passing. Purely a stale backticked path in the exact doc block that explains the `ui.vert` lockstep contract this dimension is scoped to.

## Suggested Fix

`s/gpu_instance_layout_tests.rs/shader_contract_tests.rs/` at `material.rs:67`.

## Completeness Checks
- [ ] **SIBLING**: Check for other stale cross-references to `gpu_instance_layout_tests.rs` left over from the same test-file split (closed #2415 covered its own former contents; this is a cross-reference from a different file)
