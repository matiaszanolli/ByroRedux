# 3245: D3-01: GpuMaterial 348->364 B growth (#2221) missed five documentation citation sites

**Severity**: MEDIUM (doc) · **Report**: `docs/audits/AUDIT_RENDERER_2026-08-24.md` (D3-01)

## Description

`#2221` added `shader_color_r/g/b`+`shader_float` (offsets 348-360) to `GpuMaterial`, growing it 348→364 B. The Rust struct, its tests, and the GLSL mirror in `include/bindings.glsl` are all correct and lockstepped (`gpu_material_size_is_364_bytes` passes). Five other locations were not updated, including `shader-pipeline.md`'s field table — the doc `_audit-common.md` names as authoritative for this exact byte layout — which has no row past offset 344/348 and so silently drops the two newest fields for any reader reconstructing the struct from the doc, the identical failure mode `#3201` already documented for `GpuCamera`'s missing `render_debug` row.

This is the fourth recurrence of this exact doc-drift class on this struct: `#2222`→`#2308`→`#2415`→`#2483`, now compounding with this 348→364 growth; `#3240` closed 2026-08-23 fixed only the `bindings.glsl` copy of this same comment, the same day the growth landed.

## Location
- `crates/renderer/src/vulkan/scene_buffer/constants.rs:172-176` (cites dead test name `gpu_material_size_is_348_bytes`, understates Material SSBO VRAM by ~4.4%)
- `docs/engine/shader-pipeline.md:283,326,330,395`
- `docs/engine/memory-budget.md:34`
- `docs/engine/renderer.md:134,528`
- `docs/engine/rt-lighting-material-recovery.md:618`

## Impact

No runtime effect — struct, GLSL mirror, and layout-pin tests are correct; VRAM reservation computes from `size_of::<GpuMaterial>()` dynamically. Damage is confined to documentation the project's own audit guidance designates authoritative.

## Related

`#2483`, `#3201` (identical pattern on `GpuCamera`), `#3240`, `feedback_shader_struct_sync.md`

## Suggested Fix

Same mechanical pass as `#3201`'s suggested fix — update the five sites to 364 B, add the missing field-table row, rename the dead test citation. Given this is the fourth recurrence, consider a doc-glob size-literal regression check rather than a fifth manual fix pass next growth.

## Completeness Checks
- [ ] **TESTS**: A regression test (doc-glob size-literal check) pins this so it doesn't recur a fifth time
