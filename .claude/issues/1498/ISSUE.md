## Finding REN2-13 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:196-210` (drift guard `every_shader_struct_gpu_instance_names_material_kind_slot`); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:27-28` (mirror list doc)
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`, with one premise correction (below).

## Description

`water.vert:44` declares `struct GpuInstance` (consumes `model` at `:36`/`:92`) — it is a 5th GpuInstance mirror. The struct-name drift guard covers only 4 shaders: `triangle.vert`, `triangle.frag`, `ui.vert`, `caustic_splat.comp`; **water.vert is missing** (its layout currently matches — verified).

Premise correction from validation: the audit said `gpu_types.rs:27-32` "lists the 5 mirrors" — it does not; the doc-comment names only the same four. So both the doc AND the test omit water.vert. Both should be fixed.

## Suggested Fix

Add `water.vert` to the guarded list in the test and to the mirror enumeration in the `gpu_types.rs` doc-comment.

## Completeness Checks
- [ ] **SIBLING**: Grep all shaders for `struct GpuInstance` to confirm the full mirror set before pinning (per the shader-struct-sync rule, all mirrors update in lockstep)
- [ ] **TESTS**: The extended guard is the test — verify it actually parses water.vert

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
