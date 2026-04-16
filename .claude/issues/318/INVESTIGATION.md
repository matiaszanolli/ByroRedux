# #318 — R6-01: ui.vert GpuInstance drift

## Drift

| File                          | Offset 152 | Offset 156 |
|-------------------------------|------------|------------|
| `scene_buffer.rs:85-86`       | `flags`    | `_pad1`    |
| `shaders/triangle.vert:38-39` | `flags`    | `_pad1`    |
| `shaders/triangle.frag:52-53` | `flags`    | `_pad1`    |
| `shaders/ui.vert:29-30`       | `_pad0`    | `_pad1`    |

Total struct size 160 B in every case — no layout corruption today.
`ui.vert` never reads `flags`, so behavior is unaffected. But the
Shader Struct Sync invariant (`feedback_shader_struct_sync.md`)
requires the three GLSL copies to name fields in lockstep so the
next person adding a UI-observable bit doesn't silently miss it.

## Fix

1. Rename `uint _pad0` → `uint flags` in `shaders/ui.vert:29`.
2. Recompile to SPIR-V.
3. Add a Rust-side regression test asserting
   `size_of::<GpuInstance>() == 160` and the offsets of every field
   (caught by `offset_of!`).

The issue's long-term suggestion of a shared `gpu_instance.glsl`
include is better but out of this issue's scope — it would touch all
three shaders and the build system. Leaving that as a follow-up.

## Completeness

- **SIBLING**: checked all three `GpuInstance` definitions; only
  `ui.vert` drifted.
- **TESTS**: size + offset asserts in `scene_buffer::tests`.
- **SHADER**: `glslangValidator -V ui.vert -o ui.vert.spv` recompiled.
