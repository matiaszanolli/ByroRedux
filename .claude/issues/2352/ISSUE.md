# SF-D8-01: has_material_data proxy invariant broken — BSEffectShader/Sky/Water arms fabricate PBR metalness/roughness cross-game

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2352
**Labels**: bug,nif-parser,high,legacy-compat

---

**Severity**: HIGH
**Dimension**: 8 — NIFAL Canonical Material Translation (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/material/mod.rs:1159-1164` (`PbrClassifierInputs::specular_authored`), `crates/nif/src/import/material/dedicated_shader.rs:421,555,575` (`apply_bs_effect_shader`/`apply_bs_sky_shader`/`apply_bs_water_shader`)
**Status**: NEW, CONFIRMED against current code

## Description

`PbrClassifierInputs::specular_authored` is wired as `self.has_material_data`, on the documented invariant that `has_material_data` is set **only** by the `NiMaterialProperty`/`BSLightingShaderProperty` arms — the only two that populate `specular_color`. That invariant is false post-#2059: `apply_bs_effect_shader`, `apply_bs_sky_shader`, and `apply_bs_water_shader` all set `has_material_data = true` without ever touching `specular_color`, which stays at its `[1.0, 1.0, 1.0]` struct default.

## Evidence

Traced directly against current source:
- `dedicated_shader.rs:421` (`apply_bs_effect_shader`), `:555` (`apply_bs_sky_shader`), `:575` (`apply_bs_water_shader`) all set `info.has_material_data = true` — none of the three assigns `info.specular_color`.
- `mod.rs:1164` forwards `specular_authored: self.has_material_data` with a comment asserting the (broken) invariant.
- `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs:647-669`) branches on `specular_authored`: when false it correctly returns a safe dielectric default (`metalness: 0.0`, capped roughness) — this is the #1873 chrome-flyer fix. But when `specular_authored` is (wrongly) `true`, it reads `specular_color`'s `[1,1,1]` struct default as if it were an authored Phong tint: `spec_lum = 1.0` → `metalness = ((1.0-0.5)*0.8).clamp(0,0.4) = 0.4`, and since `spec_lum > 0.6`, `roughness_ceiling = 0.55` → `roughness = base_roughness.min(0.55)`.
- For any `BSEffectShaderProperty` with `env_map_scale > 0.3` (Starfield's material-reference stub ships `env_map_scale: 1.0`, and this applies to essentially every Starfield effect-shader block — ~748 in the Meshes01 corpus), this fires and fabricates `metalness = 0.4`, `roughness = 0.55` from an unauthored default.

## Impact

Extends the #1873 chrome-flyer bug (fixed only for the PPLighting/`NiMaterialProperty` arm) to three more walker arms, **cross-game** (Skyrim/FO4/FO76/Starfield) — any effect-shader, sky-dome, or water-mesh surface routed through these arms gets fabricated chrome-tier PBR values instead of the intended dielectric defaults. Also breaks keyword-only (non-BGEM) effect-shader glass promotion, since `helpers.rs`'s `metalness >= 0.3` gate now blocks it using a value no artist authored.

**Related** (thematically similar, different root cause — not duplicates): FO3's ungated `env_map_scale` forwarding (#2315/#2328), #1873 (closed — fixed only the `NiMaterialProperty`/`BSLightingShaderProperty` arm this issue's arms were missed by).

## Suggested Fix

Replace the `has_material_data` proxy with a dedicated `MaterialInfo::specular_authored` bool set only at the two sites that actually assign `specular_color` (the `NiMaterialProperty` and `BSLightingShaderProperty` arms), gated also on `!shader.material_reference` so the Starfield material-reference stub can't claim authorship either (see SF-D8-02, filed separately, for that stub's own fabrication bug).

## Completeness Checks
- [ ] **SIBLING**: This bug is explicitly cross-game (Skyrim/FO4/FO76/Starfield) — same `specular_authored` proxy is read by every game's classifier call, not Starfield-specific
- [ ] **CANONICAL-BOUNDARY**: Fix touches `crates/nif/src/import/material/mod.rs` / `dedicated_shader.rs`, upstream of `translate_material` (`byroredux/src/material_translate.rs`) — keep the fix at the NIFAL parser→`Material` boundary, never pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (e.g. a `BSEffectShaderProperty` with default `specular_color` and `env_map_scale > 0.3` must NOT produce `metalness > 0`)
