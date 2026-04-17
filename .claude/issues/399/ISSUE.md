# OBL-D4-H3: NiTexturingProperty glow/detail/gloss slots populate ECS but never reach shader

**Issue**: #399 — https://github.com/matiaszanolli/ByroRedux/issues/399
**Labels**: bug, nif-parser, renderer, high

---

## Finding

`NiTexturingProperty` has 7 classic slots: base(0), dark(1), detail(2), gloss(3), glow(4), bump/normal(5), decal_0(6). The Oblivion-era importer extracts all 7 into `MaterialInfo`.

Only **3 of 7** reach `GpuInstance` and the fragment shader:
- `texture_path` → `texture_index` (base) ✓
- `normal_map` → `normal_map_index` ✓
- `dark_map` → `dark_map_index` ✓

The others populate the ECS `Material` component (confirmed at `byroredux/src/scene.rs:821-823` and `cell_loader.rs:1054-1056`) but there is **no** corresponding `GpuInstance` field at `crates/renderer/src/vulkan/scene_buffer.rs:44-93`:
- `glow_map` ❌ — no `glow_map_index` field
- `detail_map` ❌ — no `detail_map_index` field
- `gloss_map` ❌ — no `gloss_map_index` field

## Impact

- **Glow maps**: enchanted weapons, daedric runes (on Sigil Stones, enchantment circles), lava, the Sigil Stone itself — emissive comes only from `NiMaterialProperty.emissive` constant, not the authored glow map. Every enchanted weapon looks dim/flat.
- **Detail maps**: terrain and rock meshes use a high-frequency detail overlay sampled at a different UV scale. Without it, distant terrain looks blurry vs Oblivion's shipping look.
- **Gloss maps**: armor authors per-texel specular strength (polished buckles vs. dull leather straps). Without it, specular highlights are uniform across the whole mesh.

## Fix

1. Extend `GpuInstance` with `glow_map_index: u32`, `detail_map_index: u32`, `gloss_map_index: u32`.
2. Update all 3 shaders that consume `GpuInstance` — `triangle.vert`, `triangle.frag`, `ui.vert`. See memory `feedback_shader_struct_sync.md` — all three must be updated in lockstep.
3. Add sampling logic to `triangle.frag`:
   - Glow: multiply emissive by glow_map sample, add to final color before tone map.
   - Detail: multiply base color by detail_map.rgb at 2× scale after base sample.
   - Gloss: scale specular intensity by gloss_map.r.
4. Thread the texture registry path from `Material` into GpuInstance build-up in `byroredux/src/render.rs`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Skyrim TXST equivalent at #357 (diffuse-only extraction) — separate issue but related root cause. Skyrim has 8 slots; `BSShaderTextureSet` feeds `BSLightingShaderProperty`. Keep both issues scoped to their respective eras.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Visual test with an enchanted dagger NIF (has glow map) → emissive regions brighten in dark cells.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 4 H4-03.
