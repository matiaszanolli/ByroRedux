# #5197 — REN-D6-2026-10-03-03: CDB hits now feed the CDB colour path into `classify_pbr_keyword`, so Starfield metalness and roughness become filename guesses (eyes, creatures and fruit bins included) where the source authors them

**Labels**: medium,nifal,renderer,game:starfield,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`translate_material`: `texture_path = textures.base_color`, NaN sentinel), `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`, `classify_pbr_keyword`), `byroredux/src/asset_provider/material/merge.rs` (`apply_cdb_material`)
- **Status**: NEW. This is the #4941 shape on a new producer; related #3398 and #2707.
- **Description**:
  - Starfield `material_reference` stubs leave both overrides `None`, which reaches `translate_material` as NaN, and `resolve_pbr` then runs the keyword classifier on `texture_path`.
  - Before 18fce7e43 a stub had no texture path, so it landed on the terminal arm.
  - Now a CDB hit fills `base_color` from the CDB, and the keyword arms fire on it. `metal/steel/iron` gives metalness 0.9 and roughness 0.55. `gold/silver/bronze/copper` gives 0.95 and 0.25.
  - This happens although Starfield authors metalness and roughness explicitly, in the metal and rough slots (parked) and in `MaterialParamFloat` (deliberately untranslated). The `apply_cdb_material` doc refuses to guess a param index because "a guess would poison … every Starfield surface", yet the classifier guess now lands anyway.
  - The merge comment at the `.mat` gate still says Phase 2 "should overwrite … with CDB-authored data". Phase 2 does not.
- **Evidence**: A census of `Starfield - Textures*.ba2` `*_color.dds` (prebuilt `ba2_grep`, read-only) found 275 of 12,581 colour maps on a metal arm: 226 metal-word and 49 gold/silver-word. They include clear misfires: `actors\human\faces\eyes\iris_iron_color.dds` (eyes become a 0.9 conductor), `bipeda_silverfish_*_color.dds` (a creature becomes a 0.95 conductor), and `ak_bin_metal01_fruits_color.dds`. Per #4941, a keyword conductor with no specular authority renders as roughly 10% diffuse with no highlight.
- **Impact**: About 2% of Starfield colour maps resolve to fabricated conductors. This is the "chrome" class, produced by the boundary rather than by missing textures.
- **Suggested Fix**: On a CDB hit (`apply_cdb_material`), set dielectric-neutral overrides (`metalness_override = Some(0.0)`, roughness at the classifier's no-data neutral) until `MaterialParamFloat` or the metal/rough roles are translated. Alternatively, have `resolve_pbr`'s backstop skip keyword arms when an external material resolved. Pin it with a CDB fixture whose colour path contains `iron`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the CDB merge (`apply_cdb_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
