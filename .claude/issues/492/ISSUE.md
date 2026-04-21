# Issue #492

FO4-BGSM-3: expand GpuInstance with uv_offset/uv_scale/material_alpha — shader lockstep

---

## Parent
Split from #411. **Independent of #BGSM-1** — can land in parallel. **Required by #BGSM-4 and #BGSM-5.**

## Scope

Every BGSM-textured mesh authors UV transforms (`uv_offset`, `uv_scale`) and `material_alpha`. Today's `GpuInstance` has no slot for any of these — so even a complete BGSM parser (#BGSM-1) has nowhere to put the values once extracted.

### Current state

`GpuInstance` in `crates/renderer/src/vulkan/scene_buffer.rs` is 160 B (10×16). Grep for `parallax|heightmap|envmap` in shader tree returns zero matches (cross-ref FO3-REN-M2 / #453).

### Deliverables

Add 4 f32 fields (pad to 192 B = 12×16 for std140 alignment):

- `uv_offset_u: f32`, `uv_offset_v: f32`
- `uv_scale_u: f32`, `uv_scale_v: f32`
- `material_alpha: f32`
- 1 padding f32 to reach 192 B

Must update **in lockstep** per the Shader Struct Sync memory note — `GpuInstance` lives in 3 shaders:

1. `crates/renderer/shaders/triangle.vert`
2. `crates/renderer/shaders/triangle.frag`
3. `crates/renderer/shaders/ui.vert`

Plus the Rust-side struct at `crates/renderer/src/vulkan/scene_buffer.rs` (and its std140 size regression test at `:820-857`).

No field consumers in this issue — just plumbing. `#BGSM-5` wires the fragment shader to actually use the new fields.

### Defaults

- `uv_offset` = `(0.0, 0.0)`
- `uv_scale` = `(1.0, 1.0)` (identity UV)
- `material_alpha` = `1.0`

NIF import path must fill these from `MaterialInfo.uv_offset` / `uv_scale` / `alpha` (already parsed — grep `material.rs` — just hasn't had a GPU slot).

## Completeness Checks

- [ ] **TESTS**: `scene_buffer.rs:820-857` GpuInstance std140 size regression test updated + green
- [ ] **SIBLING**: Shader Struct Sync memory note — all 3 shader files moved in one commit
- [ ] **DROP**: No new Vulkan objects — just struct growth
- [ ] **SHADER**: recompile SPIR-V for all 3 files
- [ ] **BENCH**: no measurable regression on Prospector Saloon baseline (FPS + draw count unchanged)

## Reference

- Shader Struct Sync memory note
- Audit: `docs/audits/AUDIT_FO4_2026-04-17.md` Stage C
- Related: #453 (FO3-REN-M2) — parallax/env slots also need GpuInstance expansion; coordinate if both land in the same quarter
