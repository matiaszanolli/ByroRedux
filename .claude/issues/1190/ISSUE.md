# #1190 — TD4-NEW-01: `inst.flags` / `MAT_FLAG_*` bare numeric literals in shaders

**Severity**: LOW
**Dimension**: Magic Numbers (lockstep drift surface)
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-05-19.md`
**Created**: 2026-05-19
**Effort**: small (~20 min, mechanical)

## Sites

`inst.flags` literals:
- `crates/renderer/shaders/triangle.vert:174` — `& 1u` (NON_UNIFORM_SCALE)
- `crates/renderer/shaders/triangle.frag:793` — `& 128u` (FLAT_SHADING — test-pinned)
- `crates/renderer/shaders/triangle.frag:856` — `& 8u` (TERRAIN_SPLAT)
- `crates/renderer/shaders/triangle.frag:901, 995, 1427` — `& 2u` (ALPHA_BLEND, 3 sites)

`MAT_FLAG_*` shader-local consts (all in `triangle.frag`):
- `:141` — `VERTEX_COLOR_EMISSIVE = 0x1u`
- `:1135-1138` — `EFFECT_SOFT = 0x2u`, `_PALETTE_COLOR = 0x4u`, `_PALETTE_ALPHA = 0x8u`, `_EFFECT_LIT = 0x10u`

Plus #1147 Phase 2a added `BGSM_PBR/_TRANSLUCENCY/_MODEL_SPACE_NORMALS` (host-side bits 5/6/7) — Phase 2b shader consumer pending.

## Pin status

Only `INSTANCE_FLAG_FLAT_SHADING = 128u` is test-pinned (`flat_shading_bit_pinned_at_128_for_shader_constant`). The other 10 bits have no Rust↔shader test.

## Fix recipe

1. Add `pub const INSTANCE_FLAG_*` + `pub const MAT_FLAG_*` to `crates/renderer/src/shader_constants_data.rs`
2. `cargo build -p byroredux-renderer` regenerates `crates/renderer/shaders/include/shader_constants.glsl` with `#define` entries
3. Replace bare-literal sites with the included defines
4. Recompile SPIR-V for `triangle.vert` + `triangle.frag`
5. Replace the per-bit test with a single table-driven round-trip test (`include_str!` the generated GLSL + parse `#define` lines + assert each Rust const has a matching entry)

## Why this matters

`feedback_shader_struct_sync.md` policy. Today the bits align; tomorrow a bit-reshuffle silently corrupts. #1147 Phase 2b will inherit the drift surface unless this lands first.

## Related

- #869 (introduced the only test-pinned bit)
- #1147 Phase 2b (pending consumer of BGSM_* bits)
- `feedback_shader_struct_sync.md`

## Next step

```
/fix-issue 1190
```
