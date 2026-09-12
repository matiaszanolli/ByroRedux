# FO3-D1-2026-09-11-01: NiTexturingProperty outranks the FO3 BSShaderTextureSet for base + normal textures

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4235
**Labels**: bug, nif-parser, medium, legacy-compat, game:fo3, nifal
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-D1-2026-09-11-01

**Severity**: MEDIUM
**Dimension**: FO3 Rendering Path (Inline Shaders) — `/audit-fo3` Dimension 1
**Location**: `crates/nif/src/import/material/legacy_properties.rs:73-88` (dispatch order in `apply_legacy_property_chain`), `:179-191` (`apply_texturing_property`), `:405-417` (`apply_pp_lighting_property`)

**Description**: `apply_legacy_property_chain` runs `apply_texturing_property` before `apply_pp_lighting_property`; both texture writers are first-writer-wins (`if info.texture_path.is_none()` / `if info.normal_map.is_none()`). On any FO3/FNV shape carrying both a `NiTexturingProperty` and a `BSShaderPPLightingProperty`, the legacy chain's diffuse/normal-or-bump wins and the shader's own `BSShaderTextureSet` slots 0/1 are discarded. The Oblivion `bump_texture` fallback carries no game/BSVER gate.

**Evidence**: Confirmed dispatch order at `legacy_properties.rs:78-88` (`apply_texturing_property` called before `apply_pp_lighting_property`). The file's own #3517 note documents the mixed-family exposure ("FNV ships 58,706 `BSShaderPPLightingProperty` and 3,018 `NiTexturingProperty`"). `texture_clamp_mode`/`env_map_scale` got `_consumed` latches for this exact precedence problem (#2328/#3516/#3517); `texture_path`/`normal_map` did not.

**Impact**: On any FO3/FNV shape binding both property families, material sampling uses the wrong diffuse/normal slot, potentially with wrong normal-map encoding (bump vs tangent-space). Blast radius unmeasured (no per-shape corpus census run) — hence MEDIUM, not HIGH.

**Related**: #3517, #2328, #131, #208.

**Suggested Fix**: Latch `texture_path`/`normal_map` per-source like the scalar fields, or reorder so `apply_pp_lighting_property` runs first on FO3+; gate the Oblivion `bump_texture` fallback on `TextureSlotLayout`/BSVER.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix belongs at the NIF import → `MaterialInfo` boundary, not pushed into the renderer. See `/audit-nifal`.
- [ ] **SIBLING**: Check other first-writer-wins fields in `apply_legacy_property_chain` for the same missing `_consumed` latch pattern.
- [ ] **TESTS**: A regression test on a synthetic FO3 shape carrying both `NiTexturingProperty` and `BSShaderPPLightingProperty` pins the correct precedence.
