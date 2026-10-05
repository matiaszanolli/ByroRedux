# #5277: NIFAL-D8-2026-10-05-01: #5196's `IsGlass` → `bgem_glass` route is gated out for ~99% of Starfield glass, because the CDB merge forwards no blend state and Starfield shapes almost never carry `NiAlphaProperty`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5277
- **Labels**: medium,nifal,bug,import-pipeline,game:starfield
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D8-2026-10-05-01)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: MEDIUM. This uses the "translatable data dropped" row: authored blend state is never captured, and the glass renders
  opaque rather than disappearing. It is the same lineage and impact as #5196, which was MEDIUM.
- **Dimension**: Shader-flags/Effects (CDB merge boundary) → Material
- **Tier Violated**: parked-not-leak (an authored translation that is wired but cannot fire)
- **Game Affected**: Starfield
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:324-327`: `if cdb_mat.is_glass == Some(true) { material.bgem_glass = true; }`;
  - `byroredux/src/material_translate.rs:844-856`: classifier call with coverage `source.has_alpha || source.alpha_test`;
  - `byroredux/src/helpers.rs`: `classify_glass_into_material_with_provenance`, `if !has_transparent_coverage || is_decal { return; }`.
    This runs before the `bgem_glass` check, and only `is_mirror_pane` precedes it;
  - `crates/nif/src/import/material/mod.rs:1398-1417`: `effective_alpha_blend`, whose only source is `NiAlphaProperty`;
  - `crates/sfmaterial/src/index.rs:416-443`: capture of `AlphaSettingsComponent` (threshold and `HasOpacity` only) and
    `EffectSettingsComponent` (`IsGlass` only).
- **Status**: NEW. This is an incomplete fix of #5196 (closed by `978d25c19`). The blend-field capture itself falls within open #3398.
- **Description**: The classifier only promotes to glass when the surface has transparent coverage. That gate is what keeps an
  opaque `PawnShopWindow` out of the glass path, and it applies to the authoritative `bgem_glass` input too.
  - FO4 faced the same problem and solved it at the merge boundary: the BGSM and BGEM arms forward the material file's
    `alpha_blend_mode` into `has_alpha` (`merge.rs:1266-1271`, `:1560-1564`). The comment there reads "FO4+ moved per-material
    blend state out of NiAlphaProperty into BGSM … every Institute / lab pane renders fully opaque".
  - Starfield moved blend state into the CDB. `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:176-182` lists
    `AlphaSettingsComponent.Blender` → `AlphaBlenderSettings.Mode` and `EffectSettingsComponent.BlendingMode` (`"AlphaBlend"`).
  - `apply_cdb_material` captures neither field. It sets `alpha_test` only from `AlphaTestThreshold`, so CDB glass reaches the
    classifier with `has_alpha = false`, and the early return fires before `bgem_glass` is consulted.
- **Evidence**:
  - Per-block baseline (`crates/nif/tests/data/block_coverage_baselines/starfield.tsv`): 884 `NiAlphaProperty` against 190,549
    `BSGeometry`.
  - Full-corpus import census (a scratch tool over `Starfield - Meshes01/02/Patch.ba2` with the `.mesh` resolver, output in
    `/tmp/audit/nifal/sf_glass_census.txt`):
    - 188,936 shapes, of which `has_alpha` is set on 477 and `alpha_test` on 122.
    - **3,449 glass-named shapes** (material path or node name contains `glass`; 3,447 have a `.mat` path), of which only 23 are
      `has_alpha` and 6 are `alpha_test`.
    - Sample: `thelodgeexterior01.nif`'s `materials\…\naglasscleanopaque01_blue01.mat` (×12 sub-shapes).
  - The census cannot say which of those materials carry `IsGlass = true`. The CDB join needs the >10 GB `cdb_join_probe`.
  - The #5196 regression test `is_glass_routes_to_the_classifier_signal_without_thin_shell` asserts only the `bgem_glass` bool,
    never `MATERIAL_KIND_GLASS` out of `translate_material`, so the inert end-to-end path stays green.
- **Impact**: Authored Starfield glass (windows, visors, bottles) still renders as opaque kind-0 dielectric with the CDB colour
  texture: the exact symptom #5196 was filed for ("Authored Starfield glass is mostly not glass"). Any other alpha-blended
  Starfield material (effect cards, foliage cut-outs that use blending rather than test) is opaque for the same reason.
- **Related**: #5196 (closed), #3398 (open), #1823/#1651 (BGSM blend forwarding precedent), #4283, REN-D6-2026-10-03-02.
- **Suggested Fix**: Capture `EffectSettingsComponent.BlendingMode` and `AlphaSettingsComponent.Blender.Mode` in `MaterialIndex`,
  and forward them in `apply_cdb_material` to `has_alpha` plus `src/dst_blend_mode`, as the BGSM arm does (each enum string gets
  an explicit arm and a documented default). Add a merge → `translate_material` test asserting that a CDB `IsGlass` + `AlphaBlend`
  material classifies as `MATERIAL_KIND_GLASS`. If the blend capture is deferred to #3398, reopen #5196 or record the dependency
  there, because the current test certifies a path that cannot fire.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
