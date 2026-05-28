Surfaced by the 2026-05-28 renderer audit (`docs/audits/AUDIT_RENDERER_2026-05-28.md` Dim 14). Sibling of [#1190 / TD4-NEW-01](https://github.com/matiaszanolli/ByroRedux/issues/1190) which closed the same contract for bits 0-4.

## Issue

The `#1190 / TD4-NEW-01` invariant says: "`MAT_FLAG_*` bits in `crates/renderer/src/shader_constants_data.rs` emit into the auto-generated `crates/renderer/shaders/include/shader_constants.glsl` consumed by `triangle.frag`. Any new flag added Rust-side must NOT be hand-written into the shader — the generated header is the source of truth."

Bits 0-4 follow this correctly:
- `crates/renderer/src/shader_constants_data.rs:115-119` — `MAT_FLAG_VERTEX_COLOR_EMISSIVE` + `MAT_FLAG_EFFECT_*`
- Auto-emitted into `shader_constants.glsl` via `crates/renderer/src/shader_constants.rs:54+`
- Pinned by `tests::generated_header_contains_all_defines`

Bits 5-9 (PBR / SSS / model-space-normals suite) live in TWO parallel locations with NO lockstep test:

**Rust side** (`crates/renderer/src/vulkan/material.rs:455-476`):
```rust
pub const PBR_BSDF: u32 = 1 << 5;
pub const TRANSLUCENCY: u32 = 1 << 6;
pub const MODEL_SPACE_NORMALS: u32 = 1 << 7;
pub const TRANSLUCENCY_THICK_OBJECT: u32 = 1 << 8;
pub const TRANSLUCENCY_MIX_ALBEDO: u32 = 1 << 9;
```

**Shader side** (`crates/renderer/shaders/triangle.frag:183-187`):
```glsl
#define MAT_FLAG_PBR_BSDF                  (1u << 5)
#define MAT_FLAG_TRANSLUCENCY              (1u << 6)
#define MAT_FLAG_MODEL_SPACE_NORMALS       (1u << 7)
#define MAT_FLAG_TRANSLUCENCY_THICK_OBJECT (1u << 8)
#define MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO   (1u << 9)
```

## Risk

Drift risk. A future flag-bit reassignment (e.g., dropping a deprecated `TRANSLUCENCY_MIX_ALBEDO` and shifting subsequent bits down) could land Rust-side without touching `triangle.frag`, silently mis-routing material flags. Today the values happen to match by manual maintenance.

This is the exact failure mode #1190 fixed for bits 0-4.

## Suggested fix

1. Move bits 5-9 from `material.rs::material_flag` into `shader_constants_data.rs` alongside bits 0-4 (adopt the `MAT_FLAG_` prefix to match the generated #define names).
2. Add them to the `shader_constants.rs::tests::generated_header_contains_all_defines` table:
   ```rust
   ("MAT_FLAG_PBR_BSDF", format!("#define MAT_FLAG_PBR_BSDF {MAT_FLAG_PBR_BSDF}u")),
   // ... etc
   ```
3. Remove the hand-written `#define`s from `triangle.frag:183-187`.
4. Keep `material_flag::PBR_BSDF` etc. as `pub const = crate::shader_constants_data::MAT_FLAG_PBR_BSDF` aliases for the Rust-side ergonomic name (mirrors the `BGSM_*` legacy aliases at `material.rs:482-486`).

## Completeness Checks

- [ ] **UNSAFE**: N/A — no unsafe involved
- [ ] **SIBLING**: same pattern checked for all other `MAT_FLAG_*` and `INSTANCE_FLAG_*` and `DBG_*` bit groups (Dim 16 pins `DBG_*` lockstep; verify no other escaped sibling)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: `generated_header_contains_all_defines` extended to cover bits 5-9
