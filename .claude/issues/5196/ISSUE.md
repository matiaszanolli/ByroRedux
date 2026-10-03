# #5196 — REN-D6-2026-10-03-02: The CDB settings arm lands three flags their consumers cannot act on: `IsGlass` → `thin_glass` only, `UseSSS` → a zero-colour translucency lobe that still buys back-light rays, and alpha test ignores `HasOpacity`

**Labels**: medium,nifal,renderer,game:starfield,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/asset_provider/material/merge.rs` (`apply_cdb_material`); consumers are `byroredux/src/helpers.rs` (`classify_glass_into_material_with_provenance`), `crates/renderer/shaders/triangle.frag` (`isThinGlass`, `sssGate`) and `crates/renderer/shaders/include/lighting.glsl` (translucency block).
- **Status**: NEW (related #3398)
- **Description**:
  1. **Glass.** `cdb_mat.is_glass == Some(true)` sets only `material.thin_glass`.
     - `thin_glass` is not a glass-classifier input. `classify_glass_into_material_with_provenance` takes `bgem_glass`, the keyword match and the provenance pair.
     - The shader honours `MAT_FLAG_THIN_GLASS` only under `isGlass` (`bool isThinGlass = isGlass && …`).
     - The result: an authored-glass CDB material without a glass keyword never becomes `MATERIAL_KIND_GLASS`, and the flag is inert.
     - One that is keyword-classified is forced onto the *thin-shell* variant. In the BGEM arm that variant means `non_occluder` (`bgem_uses_thin_glass_behavior`), not "is glass". A closed bottle therefore renders as a thin sheet.
  2. **Translucency.** `UseSSS` sets `has_translucency` and `translucency_transmissive_scale`, but leaves `translucency_subsurface_color` at the default `[0,0,0]` and `mix_albedo` false.
     - `lighting.glsl` multiplies the lobe by `subsurfaceCol`, so it evaluates to exactly 0.
     - `triangle.frag`'s `sssGate = max(-rawNdotL, 0) * translucencyTransmissiveScale` ignores the colour, so every back-facing light passes the early-out and traces visibility for a zero term.
  3. **Alpha.** Alpha test is enabled whenever `AlphaTestThreshold > 0`. `CdbMaterial::has_opacity` (`AlphaSettingsComponent.HasOpacity`) is parsed, but no code in `byroredux/src` reads it, so an explicit `HasOpacity = false` does not gate the test. The test samples base-colour alpha, while Starfield's opacity source (slot 2) is parked.
- **Evidence**: grep shows `thin_glass` has no reader in `helpers.rs`; `has_opacity` has zero hits under `byroredux/src`. None of the three paths has a test: the synthetic CDB fixture carries no alpha, effect or translucency component, and `starfield_mat.rs` asserts none of them.
- **Impact**: Authored Starfield glass is mostly not glass. Authored SSS shades nothing but costs rays. Opaque materials can enter the alpha-test path.
- **Suggested Fix**: Route `IsGlass` to the classifier's positive glass signal, as `bgem_glass` does, and set `thin_glass` only from an authored thin/non-occluder fact. Do not set `has_translucency` until a subsurface colour source is translated, or have `sssGate` include the colour. Gate `alpha_test` on `has_opacity != Some(false)`. Add fixture components for all three.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the CDB merge (`apply_cdb_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
