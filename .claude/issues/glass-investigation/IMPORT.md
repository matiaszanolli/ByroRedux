# Glass-Like Material Detection in ByroRedux NIF Importer

## 1. Alpha-Blend Detection

**File**: `crates/nif/src/import/material.rs`

Alpha-blend is set via three pathways:

### Explicit NiAlphaProperty
Lines 635–650 call `apply_alpha_flags()` which unpacks flags from `NiAlphaProperty.flags`:
- **Bit 0 (0x0001)**: Enables blend mode → `info.alpha_blend = true`
- **Bit 9 (0x200)**: Enables alpha-test (cutout) → `info.alpha_test = true`, `info.alpha_blend = false` (prefer cutout)
- **Bits 1–4**: Unpack src-blend function → `info.src_blend_mode`
- **Bits 5–8**: Unpack dst-blend function → `info.dst_blend_mode`

When both bits 0 and 9 are set (common on foliage/hair), alpha-test wins because discard + z-write path sorts cleanly without z-sort artifacts.

### Implicit Alpha-Blend on BSEffectShaderProperty
Lines 565–609 detect `BSEffectShaderProperty` shader block:
```rust
if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
    // ... capture effect-shader data ...
    if !info.alpha_blend && !info.alpha_test {
        info.alpha_blend = true;  // Line 608
    }
}
```

**Commit 783acaa** ("Fix #354: implicit alpha-blend when BSEffectShaderProperty is present") introduced this logic. Bethesda effect NIFs (`meshes/effects/*.nif` glow rings, smoke cards, dust planes) frequently omit `NiAlphaProperty` entirely because the BGEM material file + effect-shader falloff cone own the blend contract. Without this implicit flip, effect-shader meshes rendered as opaque rectangles. The guard `!info.alpha_blend && !info.alpha_test` ensures explicit `NiAlphaProperty` wins—that path owns src/dst blend factors and must not be overwritten. Default blend factors (SRC_ALPHA / INV_SRC_ALPHA) are correct for falloff cones.

### BSLightingShaderProperty (Skyrim+)
Lines 528–564 process `BSLightingShaderProperty` but do **not** set `alpha_blend` directly from shader data. Blend is expected from `NiAlphaProperty` or effect-shader fallback. This is correct for Skyrim lighting meshes which pair with explicit alpha properties.

---

## 2. Two-Sided Detection

**Primary File**: `crates/nif/src/import/material.rs`, lines 860–865

Two-sided rendering is detected via `NiStencilProperty.draw_mode`:
```rust
if !info.two_sided {
    if let Some(stencil) = scene.get_as::<NiStencilProperty>(idx) {
        if stencil.is_two_sided() {
            info.two_sided = true;
        }
    }
}
```

**NiStencilProperty draw_mode values** (`crates/nif/src/blocks/properties.rs` lines 1418–1421):
- `0` or `3`: CCW_OR_BOTH / BOTH → `is_two_sided()` returns true
- `1` or `2`: CCW / CW → backface cull enabled

**Game-Specific Paths**:

1. **Skyrim+ (BSLightingShaderProperty)**: Lines 546–547 check `SF2_DOUBLE_SIDED` flag bit on `shader_flags_2`:
   ```rust
   if shader.shader_flags_2 & SF2_DOUBLE_SIDED != 0 {
       info.two_sided = true;
   }
   ```
   This is the per-mesh flag that Skyrim's nif.xml documents as bit 4 (0x10).

2. **FO3/FNV (BSShaderPPLightingProperty)**: NO Double_Sided bit exists on flags—bits are documented in nif.xml lines 6148–6218 as `Fallout3ShaderPropertyFlags1/2`. Flags1 bit 12 (0x1000) is `Unknown_3`, not Double_Sided. Flags2 bit 4 (0x10) is `Refraction_Tint`, also not Double_Sided (regression #441). FO3/FNV relies **entirely on NiStencilProperty** fallback.

3. **Oblivion**: No shader-property two-sided bit. Relies solely on `NiStencilProperty.draw_mode` (0–3).

**BsTriShape Path** (`crates/nif/src/import/mesh.rs` lines 533–550): Mirror logic via `bs_tri_shape_two_sided()` which checks for `BSEffectShaderProperty` + `SF2_DOUBLE_SIDED` flag, then falls back to `NiStencilProperty`.

---

## 3. Environment-Map Slots

**File**: `crates/nif/src/import/material.rs`

Environment map and mask reach `MaterialInfo` from `BSShaderTextureSet` slots 4 and 5:

**BSLightingShaderProperty** (Skyrim+, lines 793–803):
```rust
if info.env_map.is_none() {
    if let Some(env) = tex_set.textures.get(4).filter(|s| !s.is_empty()) {
        info.env_map = Some(env.clone());
    }
}
if info.env_mask.is_none() {
    if let Some(mask) = tex_set.textures.get(5).filter(|s| !s.is_empty()) {
        info.env_mask = Some(mask.clone());
    }
}
```

**BSShaderPPLightingProperty** (FO3/FNV, lines 530–539): Identical—slots 4 (env cubemap for glass/armor/metal) and 5 (env reflection mask).

**env_map_scale**: Routed from `BSLightingShaderProperty.env_map_scale` (line 929–930) via `ShaderTypeData::EnvironmentMap { env_map_scale }` variant dispatch.

**NiTexturingProperty** (Oblivion): **No environment-map slot equivalent**. Oblivion glass materials route through `NiMaterialProperty` (specular/emissive) + texture paths, without a dedicated env-map texture slot. Pre-#452 the env-map texture path was read and discarded during import.

---

## 4. Material Kind Tagging

**File**: `crates/nif/src/import/material.rs`, line 561

```rust
info.material_kind = shader.shader_type as u8;
```

This captures the raw `BSLightingShaderProperty.shader_type` enum (0–19) and stores it on `MaterialInfo.material_kind`. Values include:
- **0**: Default lit
- **1**: EnvironmentMap (glass-like reflections, power armor)
- **5**: SkinTint
- **14**: SparkleSnow

**No distinct glass kind**: Glass materials are tagged with `material_kind = 1` (EnvironmentMap type) and are **not distinguished from generic alpha-blend + env-mapped materials at import time**. The renderer (`crates/core/src/ecs/components/material.rs` lines 84–92) dispatches on this value but has no special glass-only branch. Glass detection lives in the renderer (`crates/renderer/...`) where depth state and refraction/Fresnel logic apply.

---

## 5. Concrete Oblivion Glass Example

**Typical vanilla Oblivion glass mesh path**: `meshes\clutter\glass\glass01.nif`

Expected field population:
- **alpha_blend**: `true` (from `NiAlphaProperty` bit 0)
- **two_sided**: `true` (if `NiStencilProperty.draw_mode == 0` or `3`)
- **texture_path**: `textures\clutter\glass\glass01.dds` (from `NiTexturingProperty` base slot)
- **normal_map**: `None` or `textures\clutter\glass\glass01_n.dds` (slot 1 bump)
- **env_map**: `None` (Oblivion has no env-slot equivalent in `NiTexturingProperty`)
- **env_map_scale**: `0.0` (default; Oblivion glass relies on material specularity, not cubemap reflection)
- **material_kind**: `0` (Oblivion has no `BSLightingShaderProperty`, so default lit)
- **z_test**: `true`, **z_write**: `true` (default; Oblivion glass typically writes depth and tests normally)

**Skyrim glass example**: `meshes\clutter\glass\glassbottle.nif` with `BSLightingShaderProperty` shader_type = 1:
- **alpha_blend**: `true` (from `NiAlphaProperty` or implicit effect-shader)
- **two_sided**: checked via `shader_flags_2 & 0x10` then `NiStencilProperty` fallback
- **env_map**: `textures\clutter\glass\glassbottle_e.dds` (slot 4 of `BSShaderTextureSet`)
- **env_map_scale**: `1.0` or higher (from shader)
- **material_kind**: `1` (EnvironmentMap type—signals Fresnel/reflection path in renderer)

---

## 6. Depth State Handling

**File**: `crates/nif/src/import/material.rs`, lines 641–650

Depth test/write are extracted from `NiZBufferProperty`:
```rust
if let Some(zbuf) = scene.get_as::<NiZBufferProperty>(idx) {
    info.z_test = zbuf.z_test_enabled;
    info.z_write = zbuf.z_write_enabled;
    if zbuf.z_function < 8 {
        info.z_function = zbuf.z_function as u8;
    }
}
```

**No implicit z_write override on alpha-blend**: When `NiAlphaProperty` enables blend, z_write is **not** automatically disabled in the importer. This is correct—Gamebryo relies on explicit `NiZBufferProperty` to opt out of depth-write on transparent surfaces. Vanilla glass meshes typically retain z_write=true because glass is depth-sorted at the renderer level (not in the importer). Pre-#398 this data was extracted but never reached the GPU, causing z-fighting on foreground transparent geometry against world interiors.

**Transfer to ImportedMesh** (`crates/nif/src/import/mesh.rs` lines 172–173):
```rust
z_test: mat.z_test,
z_write: mat.z_write,
```

Defaults (lines 479–481) mirror Gamebryo: z_test=true, z_write=true, z_function=3 (LESSEQUAL).

---

## Summary

Glass-like materials in ByroRedux are detected via:
1. **Alpha-blend** set by `NiAlphaProperty` flags or implicit BSEffectShaderProperty presence (Skyrim+)
2. **Two-sided** via `NiStencilProperty.draw_mode` (all games) or `BSLightingShaderProperty` flags2 bit 4 (Skyrim+ only)
3. **Environment maps** from `BSShaderTextureSet` slots 4–5 (Skyrim+/FO3) or absent in Oblivion
4. **Material kind** = 1 (EnvironmentMap) for Skyrim glass, 0 for Oblivion
5. **Depth state** preserved as-is from `NiZBufferProperty`; no implicit override on alpha-blend

Glass is not tagged distinctly at import—renderer dispatches on material_kind + env_map + alpha_blend + z_write state to enable Fresnel/refraction paths.
