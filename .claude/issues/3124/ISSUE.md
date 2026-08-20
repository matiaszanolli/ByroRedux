# REN-D15-01: GpuWaterParams has three declaration sites and no lockstep guard

**Issue**: #3124 — https://github.com/matiaszanolli/ByroRedux/issues/3124
**Labels**: `medium,renderer,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-20.md`

---

**Severity**: MEDIUM
**Dimension**: Water / GPU-Struct Layout
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-20.md` (REN-D15-01)

## Location

- `crates/renderer/src/vulkan/water.rs` — `GpuWaterParams` (`:65`), `MAX_WATER_DRAWS` (`:172`), `water_gpu_contract_layouts_are_stable`, `water_vertex_shader_keeps_the_full_material_array_stride`
- `crates/renderer/shaders/water.vert` — `struct WaterParams` (`:92`) + `WaterParams params[186]` (`:135`)
- `crates/renderer/shaders/water.frag` — `struct WaterParams` (`:62`) + `WaterParams params[186]` (`:129`)

## Description

`GpuWaterParams` is a 352-byte std140 UBO record declared in **three** places — once in Rust and hand-mirrored in `water.vert` and `water.frag` — with the same blast radius as `GpuInstance` and `GpuMaterial`, but **none of their protection**.

**This is the one unguarded member of an otherwise-guarded family.** Every *other* GPU-struct mirror in the renderer was verified field-by-field by script during this audit and **all are in lockstep at HEAD**:

| Struct | Sites | Guard | Result |
|---|---|---|---|
| `GpuInstance` | 5 (`include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`) | `gpu_instance_glsl_copies_stay_in_lockstep` (parses all five mirrors) | 5/5 identical, 128 B, all carry `surfaceId` |
| `GpuMaterial` | 2 (`material.rs`, `include/bindings.glsl`) | the #1657 cross-check (parses both, compares field-for-field) | **87 fields**, 348 B, name *and* type parity |
| `GpuCamera` / `CameraUBO` | 5 | size pin + declaration audit | 352 B, `renderDebug` present and last in all five |
| **`GpuWaterParams` / `WaterParams`** | **3** | **none** | 352 B / 22 slots identical — but by luck, not by guard |

What exists instead of a guard is `water_vertex_shader_keeps_the_full_material_array_stride`, which asserts that eight `vec4 <name>;` substrings appear *somewhere* in `water.vert`. It **cannot** detect a reordered slot, a `vec4`↔`uvec4` type flip, or a slot inserted mid-struct in one mirror only.

Separately, the UBO array bound is the bare literal `186` in both shaders, with no pin against `MAX_WATER_DRAWS`. `water_gpu_contract_layouts_are_stable` checks only that `MAX_WATER_DRAWS × 352 ≤ 64 KiB`, which stays true if the constant and the shaders diverge.

## Evidence

A field-by-field script diff of the three declarations (Rust snake_case → GLSL snake_case, including `uvec4 noise_indices` at slot 9) shows **zero drift at HEAD** — the contract is currently intact. **The gap is the absence of a guard, on the delta's two most-edited shader files**: `water.frag` took **29** commits and `water.vert` **15** in this delta alone.

```
$ grep -rn "struct GpuWaterParams\|struct WaterParams" crates/renderer/src crates/renderer/shaders
crates/renderer/src/vulkan/water.rs:65:pub struct GpuWaterParams {
crates/renderer/shaders/water.frag:62:struct WaterParams {
crates/renderer/shaders/water.vert:92:struct WaterParams {

$ grep -rn "params\[186\]" crates/renderer/shaders/
crates/renderer/shaders/water.vert:135:    WaterParams params[186];
crates/renderer/shaders/water.frag:129:    WaterParams params[186];

$ grep -n "MAX_WATER_DRAWS" crates/renderer/src/vulkan/water.rs
crates/renderer/src/vulkan/water.rs:172:pub const MAX_WATER_DRAWS: usize = 186;
```

`GpuWaterParams` appears in exactly one Rust file (`water.rs`) — it is absent from `scene_buffer/gpu_instance_layout_tests.rs`, where the lockstep machinery for the whole rest of the family lives.

## Impact

**Defense-in-depth only — nothing is broken today.** But per `feedback_shader_struct_sync.md` this hand-mirrored-GLSL-struct pattern is the project's documented **#1 source of silent GPU desync**, and a slot inserted into `water.frag`'s mirror but not `water.vert`'s would shift every subsequent `vec4` in the vertex stage by 16 bytes with **no test failure and no validation error**. It would surface as wrong wave amplitude or wrong scroll velocity — i.e. as a *tuning* bug, which is the hardest kind to trace back to a layout cause. The exposure is highest precisely because these two files are where the active water work is landing.

## Suggested Fix

Extend the existing `parse_rust_struct_fields` / `parse_glsl_struct_fields` machinery in `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` to cover `GpuWaterParams` against **both** `water.vert` and `water.frag`, and add an assertion that both shaders contain `params[{MAX_WATER_DRAWS}]` built from the constant rather than a hand-typed `186`.

## Related

- #2787 (OPEN — `water.frag` `ampScale`/`freqScale` sentinel lockstep, tautological test): **adjacent but distinct** — that one is about shader-internal constants, this is about the struct/array contract
- #2763 (OPEN — `water.vert`'s stale "112-byte invariant" comment)
- `feedback_shader_struct_sync.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — after adding the guard, sweep for any other hand-mirrored GPU struct with no cross-check
- [ ] **TESTS**: A regression test pins this specific fix (this finding *is* a missing test — the fix is the guard)
