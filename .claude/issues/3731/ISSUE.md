# #3731: NIFAL-2026-08-30-D1-01: Material::sanitize_finite never descends into effect_falloff / shader_type_fields — 22 float slots reach GpuMaterial outside both save-path gates

**Labels**: bug, renderer, medium, nifal, save-load
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: 1 (Material) · **Tier violated**: no-leak
**Game affected**: all (FO3/FNV via `BSShaderNoLightingProperty` falloff; Skyrim+/FO4 via `BSEffectShaderProperty` falloff and the `BSLightingShaderProperty` shader-type payloads)

## Location
- `crates/core/src/ecs/components/material.rs` — `Material::sanitize_finite` (the sweep, currently `:1215`), the two uncovered carriers `shader_type_fields` (`:224`) and `effect_falloff` (`:232`), plus `EffectFalloff` / `ShaderTypeFields`

## Description
`sanitize_finite` is the single finiteness gate for the canonical `Material`, consumed by `crates/save/src/driver.rs` on restore and probed on a clone by `validate_material_finiteness` (`crates/save/src/validate.rs`) pre-save.

Its macro list covers every *directly-declared* float field — mechanically diffed, all 33 (31 explicit + `metalness`/`roughness` via `resolve_pbr`) are present, so **#3373's specific hole is closed**. But `Material` carries two further float payloads behind indirection that the macro list cannot reach and does not mention:

- `effect_falloff: Option<EffectFalloff>` — 5 f32 (`start_angle`, `stop_angle`, `start_opacity`, `stop_opacity`, `soft_falloff_depth`)
- `shader_type_fields: Option<Box<ShaderTypeFields>>` — 13 `Option<f32>`/`Option<[f32; N]>` (`skin_tint_color`, `skin_tint_alpha`, `hair_tint_color`, `eye_cubemap_scale`, `eye_left/right_reflection_center`, `parallax_max_passes`, `parallax_height_scale`, `multi_layer_*` ×4, `sparkle_parameters`)

That is **22 additional scalar slots outside both save-path gates**. Verified against current source: the body of `sanitize_finite` contains no reference to either carrier.

## Evidence
The values are live on the GPU path, not inert:
- `byroredux/src/render/static_meshes.rs` reads `m.effect_falloff` into `DrawCommand.effect_falloff` (gated on `material_kind == MATERIAL_KIND_EFFECT_SHADER` — i.e. exactly the materials that author a falloff cone), which `crates/renderer/src/vulkan/context/mod.rs` unpacks into `GpuMaterial.falloff_start_angle` … `soft_falloff_depth`, and hashes with `to_bits()` into the material-table dedup key.
- `byroredux/src/render/static_meshes.rs` reads `shader_type_fields` into the `skin_tint_rgba` / `hair_tint_rgb` / `sparkle_rgba` / `multi_layer_*` GPU slots.

Reachability: the parser applies no finiteness guard on this path — `NifStream::read_f32_le` returns `f32::from_le_bytes` verbatim for any bit pattern, and the only `is_finite` check in the shader-block parser is the unrelated FO4 rimlight sentinel in `crates/nif/src/blocks/shader.rs`.

## Impact
A non-finite authored/corrupted value in an effect-shader falloff cone or a `BSLightingShaderProperty` shader-type payload reaches `GpuMaterial` and the fragment shader unrepaired, and survives a save/load round trip that the same method exists to make safe for its 33 siblings. NaN/Inf into the GPU is exactly the hazard #2687 introduced this method for. Silent: no compile error, no test failure, and the pre-save probe reports the material clean.

## Related
#3373 (the identical omission for the BGEM glass-optics tail, fixed — this is the same defect class one level of indirection deeper), #3438 (the pin cannot catch this class structurally), #3073 (`parallax_height_scale`/`parallax_max_passes` bypass the canonical `Material` — the *same two fields*, different defect).

## Suggested Fix
Give `EffectFalloff` and `ShaderTypeFields` their own `sanitize_finite` returning `changed`, and call both from `Material::sanitize_finite` (`if let Some(f) = self.effect_falloff.as_mut() { changed |= f.sanitize_finite(); }`). No new constants — reset to each type's `Default`, matching the existing `fix_scalar!` semantics.

## Completeness Checks
- [ ] **SIBLING**: any further `Option<...>` float carrier added to `Material` needs the same descent — check the whole struct, not just these two
- [ ] **CANONICAL-BOUNDARY**: the repair stays on the canonical `Material` — no per-game logic, and nothing re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: extend `sanitize_finite_leaves_no_non_finite_float_anywhere` so it actually reaches the indirect carriers
