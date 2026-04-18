# NIF-D4-N01: BsTriShape drops ShaderTypeData payload — re-opens half of #343 for Skyrim+/FO4 meshes

**Issue**: #430 — https://github.com/matiaszanolli/ByroRedux/issues/430
**Labels**: bug, nif-parser, renderer, critical

---

## Finding

`crates/nif/src/import/mesh.rs:310-327` (inside `extract_bs_tri_shape`) has an exhaustive `match shader.shader_type_data` that only reads `EnvironmentMap.env_map_scale` and collapses every other variant to a `1.0` fallback:

```rust
env_map_scale = match shader.shader_type_data {
    ShaderTypeData::EnvironmentMap { env_map_scale } => env_map_scale,
    ShaderTypeData::None
    | ShaderTypeData::SkinTint { .. }
    | ShaderTypeData::Fo76SkinTint { .. }
    | ShaderTypeData::HairTint { .. }
    | ShaderTypeData::ParallaxOcc { .. }
    | ShaderTypeData::MultiLayerParallax { .. }
    | ShaderTypeData::SparkleSnow { .. }
    | ShaderTypeData::EyeEnvmap { .. } => 1.0,
};
```

Comment at lines 313-316 acknowledges: *"The rest … carry per-NPC/per-effect data that doesn't have a counterpart field on ImportedMesh yet — tracked at SK-D3-01 via the `extract_material_info` path."*

But `MaterialInfo` DID grow fields for all 9 variants when #343 closed (material.rs ~140-175: `skin_tint_color`, `hair_tint_color`, `eye_cubemap_scale`, `parallax_max_passes`, `multi_layer_inner_thickness`, `sparkle_parameters`, etc.), and `ImportedMesh.material_kind` IS set on the BsTriShape path at line 438-443:

```rust
material_kind: shape
    .shader_property_ref
    .index()
    .and_then(|i| scene.get_as::<BSLightingShaderProperty>(i))
    .map(|s| s.shader_type as u8)
    .unwrap_or(0),
```

**So BsTriShape captures the dispatch key (`material_kind`) but drops the per-variant payload fields.** The renderer will later route on `material_kind == 5` expecting `skin_tint_color` to be populated, but BsTriShape meshes leave it at `None`.

## Impact

- Skyrim SE + FO4 + FO76 + Starfield BsTriShape meshes with `shader_type != EnvironmentMap` silently lose:
  - SkinTint RGB (race/character skin color — type 5)
  - HairTint RGB (type 6)
  - EyeEnvmap scale + left/right reflection centers (type 16)
  - ParallaxOcc max_passes + height scale (type 7)
  - MultiLayerParallax inner thickness / refraction scale / layer scale / envmap strength (type 11)
  - SparkleSnow params (type 14)
  - Fo76 SkinTint RGBA (FO76-specific, type 4)

Re-opens half of #343 (SK-D3-01) for the BsTriShape import path. NiTriShape path correctly populates these via `extract_material_info` → `apply_shader_type_data`.

## Fix

Two approaches; (a) is the cleanest:

**(a) Promote `apply_shader_type_data` to `pub(super)` + add BsTriShape output fields**:
1. Extend `ImportedMesh` with the same 13+ fields `MaterialInfo` already has (skin_tint_color, hair_tint_color, eye_*, parallax_*, multi_layer_*, sparkle_parameters).
2. Also mirror them on BsTriShape path — build a small `ShaderTypeFields` struct locally, call `apply_shader_type_data` (the helper is already pub(super) per #343), copy to ImportedMesh.

**(b) Refactor `extract_bs_tri_shape` to build a full `MaterialInfo` via a new helper `extract_material_info_for_bs_tri_shape`**:
- Matches the audit command's suggested name (and eliminates duplication between the NiTriShape and BsTriShape material extraction paths).
- Larger refactor but pays down tech debt.

Either way the fix must also land the ImportedMesh fields — they don't exist today, and GpuInstance expansion for SK-D3-02 (#344) is the downstream consumer.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify `find_effect_shader_bs` (mesh.rs:432 — `effect_shader: find_effect_shader_bs(...)`) doesn't need parallel shader-type dispatch; effect shaders use a different variant set.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Import a Skyrim SE NPC head mesh (uses SkinTint shader_type=5) via BsTriShape path; assert `ImportedMesh.skin_tint_color == Some([r,g,b])`. Parallel test on the NiTriShape path asserting the same output.

## Related

- Extends / depends on #343 (SK-D3-01 — the same fix on NiTriShape path).
- Unblocks #344 (SK-D3-02 — material_kind dispatch in triangle.frag) for BsTriShape-era content.
- Also interacts with #346 (BsTriShape import path BSEffectShaderProperty), #345 (BSEffectShaderProperty rich fields discarded).

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 4 N01.
