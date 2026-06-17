# REN-D3-01: Reservoir removal broke two GPU-struct layout-pin guards (suite RED)

- **Issue**: #1645
- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Source audit**: docs/audits/AUDIT_RENDERER_2026-06-16.md
- **Labels**: medium, renderer, bug
- **Location**: `crates/renderer/src/vulkan/material.rs::tests::gpu_material_glsl_field_names_pinned` (material.rs:1319) + `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs::every_shader_struct_gpu_instance_names_material_kind_slot` (:223)

## Description
Both drift guards `include_str!` `triangle.frag` and assert it contains
`struct GpuInstance` / `materialFlags;`. The reservoir-removal refactor
(`218b425b`) moved those struct declarations into `include/bindings.glsl`, so the
tests now panic.

## Evidence
`cargo test -p byroredux-renderer --lib` → `329 passed; 2 failed` (confirmed at
HEAD). `grep -rl "struct GpuInstance" crates/renderer/shaders/` →
`bindings.glsl` + `triangle.vert`/`ui.vert`/`water.vert`/`caustic_splat.comp`,
not `triangle.frag` (0 matches). Struct bytes intact; Rust-side offset/size pins
still PASS. Test-fixture path bug, not real layout drift.

## Impact
GLSL-side lockstep guards are dead-red — a future shader-side field change in
`bindings.glsl` would go uncaught. Whole `byroredux-renderer` suite RED masks
further regressions.

## Suggested Fix
Repoint both `include_str!`s from `triangle.frag` to `include/bindings.glsl`
(mind relative-path depth), update doc-comments citing `triangle.frag:110-184`,
keep the 4 `.vert`/`.comp` GpuInstance mirrors asserted.

## Completeness Checks
- [ ] SIBLING: 4 `.vert`/`.comp` mirrors stay asserted; only triangle.frag entry swapped
- [ ] TESTS: `cargo test -p byroredux-renderer` GREEN; needle assertions fire vs bindings.glsl
