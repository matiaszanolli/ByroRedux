**HEAD**: 3bcf6c8e8 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-08.md (HEAD 00f580e09) · **Audited**: Dims 1, 2, 3, 8 (delta-touched), plus the streaming-deep area (`npc_spawn/seam_blend*`, `cell_loader/spawn.rs`, `spawn/mesh_instance.rs`, #5359, #5358) and the two named commits 9bb6b7dce / 3bcf6c8e8 · **Unchanged since baseline (skimmed)**: Dims 4, 5, 6, 7, 9 (no commits on their paths; first steps, guards and the D9 harness run)

# NIFAL Audit — 2026-10-09

This report is one leg of `/audit-suite --preset streaming-deep`. One auditor ran every dimension in sequence, with no
sub-agents. The delta is `00f580e09..3bcf6c8e8`, 81 commits. Of those, 9 touch NIFAL paths or docs:
3031d8976, 7d1de4ad5, cf9dec258, 9bb6b7dce, faf09c6e6, 46dfdcc6c, dd8c1a1f1, plus the in-area commits edb5fbdfe and
479414ffe. Scratch notes for each dimension are in `/tmp/audit/nifal/dim_{1..9}.md`. This report was reconciled against them.

## Executive Summary

**New findings: 3 (0 CRITICAL · 0 HIGH · 2 MEDIUM · 1 LOW).** All three are in Dim 8, the merge boundary. All three come
from measuring 9bb6b7dce (#5277 and #5283, the CDB blend and emissive capture) against the shipped `materialsbeta.cdb` and
the material names that Starfield's NIFs actually reference.

**Headline (MEDIUM, D8-01).** #5277 and #5283 are both closed, but neither capture fires on any vanilla material Starfield
references:
- `BlendingMode == "AlphaBlend"` fires on 0 of 10,158 looked-up names.
- `EmissiveSettingsComponent.Enabled == true` also fires on 0 of 10,158.

The root cause is older than either fix. `MaterialIndex` reads each object's `ObjectInfo.Parent` and then throws it away,
so it never resolves the CDB's inheritance. Nearly every object has a parent: 500,397 of 500,403, including all 10,080
NIF-referenced material roots. Glass and emissive state is mostly inherited from a shader-model parent:
- 182 referenced materials inherit from `1LayerEffectGlass.mat`, which carries `IsGlass = true`. Lookup sees `IsGlass` on 4.
- 355 referenced materials inherit from `ColorEmissive.mat`, which carries `Enabled = true` and a luminance of 139.24.
  213 of those 355 bind an emissive map.

The two new `== Some(…)` gates read "inherited" as "off". The commit's own end-to-end test pins IsGlass + AlphaBlend. The
shipped CDB has no instance of that combination: all 7 `IsGlass = true` instances author no `BlendingMode` at all. That is
the "certified the inert path" defect the commit diagnosed in #5196's test.

**D8-02 (MEDIUM).** When the emissive arm does fire, it writes Starfield's physical `LuminousEmittance` straight into
`emissive_mult`. Measured enabled values run from 50 to 5,199. It tags them `EmissiveSource::Lighting`, the same tag whose
measured modes are 1.0 (Skyrim) and 0.05 (FO4) in `nifal.md` §4. §4 has no Starfield row. The justification in the code,
"the EV100-metered HDR pipeline consumes linear luminance natively", has no source. The `AdaptiveEmittance` and
`ExposureOffset` values authored beside the luminance are dropped. The dominant Starfield emissive component,
`LayeredEmissivityComponent` (32,817 instances, 31× `EmissiveSettingsComponent`), is not captured at all. This is latent
today, but it goes live the moment D8-01 is fixed.

**D8-03 (LOW).** The loose `.mat` decoder (#5396) and the compiled lookup produce the same `CdbMaterial`. 9bb6b7dce
extended only the compiled one. 16 of the 20 installed loose files (Creation terminal and splash screens) author
`EmissiveSettingsComponent`, and the loose decoder drops it.

Per-category status against spec §2:

| Category | This delta |
|---|---|
| Material (D1) | Converged. #5281 verified (one 3-argument body for the texture-only boundary); 350 `byroredux` NIFAL guard tests pass. No second `Material` producer in `cell_loader/`, `npc_spawn/`, `streaming/` or `scene/` (test literals only) |
| Geometry/Transform (D2) | No coord/transform/units change. No findings |
| Skinning / Lights (D3) | #5389 verified: it declines and never clamps, on all three producers. #5282's scope gap is now in the ledger (dd8c1a1f1). No findings |
| Nodes (D4) | Unchanged: the 7 parked fields have 0 canonical consumers |
| Particles (D5) | Unchanged: exactly 2 overlay callers (`nif_loader.rs:1738`, `cell_loader/spawn.rs:1276`) |
| Collision (D6) | Unchanged: 16 resolve arms (counted fresh); dispatch-coverage guard green |
| Animation (D7) | Unchanged: the NIF and HKX completeness harnesses are green |
| Shader flags / roles / merge (D8) | **2 MEDIUM + 1 LOW** (the 9bb6b7dce captures, CDB inheritance, loose parity). #5396, #5436 and #5381 verified |
| Completeness (D9) | Harness 2/2 green on all 8 games; the table is identical to 2026-10-08. The harness measures the pre-merge tier, so it cannot see D8-01..03 |

New findings per invariant:

| Invariant | Findings |
|---|---|
| single-boundary | D8-03 (two `CdbMaterial` producers whose capture sets diverged) |
| no-fabrication | D8-02 (an unsourced "native luminance" claim; an unmeasured scale for a new emissive source) |
| no-leak | 0 |
| no-render-time-fallback | 0. D8-02 would push the scale decision onto `triangle.frag`'s 64 ceiling, which §4 already flags as a boundary smell |
| parked-not-leak / translation gap | D8-01 (inherited CDB state dropped; both captures inert), D8-02 (`LayeredEmissivityComponent` uncaptured) |
| harness-gap | D8-01 (both new tests pin combinations with zero vanilla instances) |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS: 4 production callers (`nif_loader:1301`, `mesh_instance:1114`, `placement_lod:552`, `object_lod:366`) | PASS | PASS | PASS | `translate_material_with_provenance` / `translate_texture_only_material*` |
| Geometry/Transform | PASS | PASS | PASS | PASS | `import/units.rs`, `import/transform.rs`, `rotation.rs` |
| Skinning / Lights | PASS: one decline helper for 3 producers (#5389) | PASS: declines, never clamps | PASS | PASS | `mesh/skin.rs`, `walk/lights.rs` |
| Particles | PASS | PASS | PASS | PASS | `systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS | PASS | `import/collision/` |
| Animation | PASS | PASS | PASS | PASS | `convert_nif_clip` + `convert_hkx_clip` |
| Shader flags / roles / merge | **PARTIAL**: D8-03, and #5284 still open | **FAIL**: D8-02 | PASS | PASS | `merge.rs::{apply_cdb_material, apply_loose_mat}`, `sfmaterial::MaterialIndex::lookup`, `loose_mat::parse_loose_mat` |

## Findings

### MEDIUM

#### NIFAL-D8-2026-10-09-01: The CDB index resolves no `ObjectInfo.Parent` inheritance, so inherited `IsGlass` and emissive `Enabled` read as absent, and both #5277 and #5283 fire on 0 of 10,158 referenced Starfield materials
- **Severity**: MEDIUM. This is the "translatable data silently dropped" row. It matches #5277's triage of the same symptom.
  The escalation clause, "HIGH if it removes visible game content", is arguable: about 182 surfaces render opaque instead of
  as glass, and 213 emissive maps are bound at zero weight.
- **Dimension**: Shader-flags/Effects (the merge boundary, the Starfield CDB arm)
- **Tier Violated**: parked-not-leak (the translation is wired but cannot fire) + harness-gap
- **Game Affected**: Starfield
- **Location**:
  - `crates/sfmaterial/src/index.rs:655-684`: the `"Objects"` stream keeps `DBID` and `PersistentID`; `Parent` is read past
    and dropped.
  - `index.rs:334-405`: `collect_slots` and `collect_settings` are first-wins over the root and layer materials only.
  - `byroredux/src/asset_provider/material/merge.rs:348` (`== Some("AlphaBlend")`) and `:363` (`== Some(true)`).
  - `byroredux/src/helpers.rs:205` (the coverage early return, fed by `material_translate.rs:891`).
  - Tests: `merge.rs:2026` (`cdb_is_glass_with_alpha_blend_classifies_as_glass_end_to_end`) and `:2082`
    (`cdb_emissive_settings_weight_the_emissive_role`).
  - `docs/engine/nifal.md:710-718` records both captures as landed.
- **Status**: NEW. It is an incomplete fix of #5277 (MEDIUM, closed) and #5283 (LOW, closed) by 9bb6b7dce. No open or closed
  issue covers CDB `Parent` inheritance (searched: "ParentPersistentID", "CDB parent inheritance", "shader model CDB"; #3398
  does not mention it).
- **Description**:
  - Every `BSComponentDB2::DBFileIndex::ObjectInfo` carries a `Parent` (spike doc §1). The CDB stores components as diffs:
    a field the object does not author comes from its parent chain. `loose_mat.rs:32-35` states this rule for the loose form
    of the same data ("a property it does not author is inherited").
  - `MaterialIndex::build` never records `Parent`, so `lookup` returns only what a root or layer material authors itself.
  - 9bb6b7dce then gates on explicit values: `blending_mode == Some("AlphaBlend")` sets `has_alpha`, and
    `emissive_enabled == Some(true)` forwards emissive. An inherited "on" therefore reads as "off".
  - The codebase now has three different rules for an absent CDB `Enabled`:
    - `TextureReplacement` treats it as enabled (`index.rs:345`, `loose_mat.rs:97-99`);
    - `EmissiveSettingsComponent` treats it as disabled (`merge.rs:363`);
    - the loose doc treats it as inherited.
- **Evidence** (a scratch probe in `/tmp/audit/nifal/cdbprobe`, read-only, using the crate's public `MaterialIndex::lookup`,
  `material_key` and `visit_instances_with_limits`, run over all 10,380 names that `sf_matpath_dump` collects from
  `Meshes01/02/Patch`):
  - **Lookup over all 10,158 hits**:
    - `BlendingMode` values: none 10,017; Additive 69; SourceSoftAdditive 67; others 5; **AlphaBlend 0**.
    - `IsGlass = true` on 4 names, all with no blend, opacity or threshold.
    - `emissive_enabled`: None 10,154; `Some(false)` 4; **`Some(true)` 0**.
    - 716 names bind a `SLOT_EMISSIVE` texture.
  - **Raw CDB**:
    - `EffectSettingsComponent`: 2,412 instances. `"AlphaBlend"` appears on 2 of them. All 7 `IsGlass = true` instances
      author no `BlendingMode`.
    - `EmissiveSettingsComponent`: 1,058 instances. `Enabled` is true on 59, false on 8, and **absent on 991**. 607 of the
      991 still author a `LuminousEmittance`.
  - **Inheritance**:
    - 500,397 of 500,403 objects have a non-zero `Parent`, including all 10,080 NIF-referenced roots.
    - `1LayerEffectGlass.mat` looks up with `IsGlass = Some(true)`. 182 referenced roots have a parent chain that reaches it.
      Lookup returns `glass=None` on all 182.
    - `ColorEmissive.mat` looks up with `Enabled = Some(true)` and a luminance of 139.243. 355 referenced roots reach it.
      Lookup returns `em_en=None` on all 355. Of those, 213 bind an emissive map and 215 author their own
      `LuminousEmittance` (100, 150, 200 or 500).
    - These are chain-reach counts. An intermediate ancestor could override the value; only resolving the chain settles it.
- **Impact**:
  - The #5196/#5277 symptom survives both fixes: CDB-authored glass still renders as an opaque kind-0 dielectric.
  - The #5283 symptom also survives: every Starfield emissive map stays bound at zero weight, including the
    `GenericGlow01_*` family, whose `_Base` binds slot 7 with a luminance of 150 and no explicit `Enabled`.
  - Every other CDB setting that is inherited (alpha-test threshold, `HasOpacity`, `UseSSS`, `TextureReplacement`) is
    under-read in the same way. The four CDB fill sites (#5284) all share this lookup.
- **Related**: #5277, #5283, #5196 (closed); #3398 (open, CDB per-field data); #5284 (open); D8-02; D8-03; #5465 (open, the
  `PersistentID` column order).
- **Suggested Fix**:
  - Record `Parent` in `stream_db_file_index`, and resolve `lookup` nearest-wins along the parent chain, with a depth and
    cycle bound.
  - Re-run this census once inheritance is resolved. If the inherited-glass materials still author no coverage, then gating
    CDB `IsGlass` on `has_alpha || alpha_test` is the wrong precondition. Decide which signal carries Starfield glass
    coverage, and record the decision in `nifal.md`.
  - Replace the two synthetic-combination tests with a pin taken from a real looked-up material (for example a
    `1LayerEffectGlass` child and a `ColorEmissive` child).

#### NIFAL-D8-2026-10-09-02: #5283 forwards Starfield's physical `LuminousEmittance` raw into `emissive_mult` under the `Lighting` tag, with no §4 measurement, drops `AdaptiveEmittance` / `ExposureOffset`, and never captures the dominant `LayeredEmissivityComponent`
- **Severity**: MEDIUM. This is a divergent canonical `Material` field across games. It is latent today, because the arm
  fires on 0 referenced materials (D8-01). It becomes live as soon as D8-01 is fixed.
- **Dimension**: Shader-flags/Effects → Material
- **Tier Violated**: no-fabrication (an unsourced scale claim) + parked-not-leak (the dominant emissive component is
  uncaptured)
- **Game Affected**: Starfield (and, by tag, every consumer that reads `EmissiveSource`)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:352-386`, in particular `:361-362`, the "consumes linear luminance
    natively" comment;
  - `crates/sfmaterial/src/index.rs:521-539` (captures only `Enabled`, `EmissiveTint` and `LuminousEmittance`);
  - `docs/engine/nifal.md:945-1000` (§4 has no Starfield row) and `:718`;
  - `crates/renderer/shaders/triangle.frag:1692` (`min(emissiveColor * emissiveMult * emissiveMask, vec3(64.0))`).
- **Status**: NEW (an incomplete fix of #5283).
- **Description**:
  - §4 measured three emissive sources: FNV `Material` mode 1.0, Skyrim `Lighting` mode 1.0, FO4 `Lighting` mode 0.05. It
    concluded that "no source is systematically offset from the others by a fixed factor".
  - #5283 adds a fourth source under the existing `Lighting` tag without measuring it. Starfield's emittance is a physical
    luminance, offset by orders of magnitude.
  - The comment asserts the HDR pipeline "consumes linear luminance natively", but the rest of every scene is Gamebryo
    monitor-space radiance (*feedback_color_space*). The only scale decision left is the shader's 64 ceiling, which §4
    already records as a render-time material decision doing the canonical tier's work.
  - `EmittanceSettings` carries `AdaptiveEmittance` and `ExposureOffset`. Starfield authors them right beside the luminance
    (for example `requisitionkiosk_splashscreen_ryujin.mat`: `AdaptiveEmittance "true"`, `ExposureOffset "6"`,
    `LuminousEmittance "100"`). Both are dropped.
  - Separately, `BSMaterial::LayeredEmissivityComponent` has 32,817 instances, against 1,058 for `EmissiveSettingsComponent`.
    It has no capture at all.
- **Evidence**:
  - `EmissiveSettingsComponent` `LuminousEmittance`: enabled instances run from 50 to 5,199.2 (mode 50, p90 1,362.9); across
    all instances, p50 is 200 and the maximum 30,000.
  - The 215 `ColorEmissive`-inheriting referenced roots author luminances of 100–500.
  - `LayeredEmissivityComponent` `LuminousEmittance`: on 31,759 instances, mode 55.7299 and maximum 120,000. With `Enabled`
    true, p50 is 437 and the maximum 8,192. `AdaptiveEmittance` is true on 811 instances; `ExposureOffset` is present on
    1,820.
- **Impact**:
  - Once D8-01 resolves inheritance, about 215 or more referenced emissive materials would carry `emissive_mult` of 100–500.
    That is 100× to 10,000× the other games' modes, and every bright texel would saturate at the 64 ceiling.
  - Any later per-source emissive policy keyed on `EmissiveSource::Lighting` would also mix Skyrim/FO4 multipliers with
    Starfield nits.
- **Related**: #5283 (closed), D8-01, D8-03, `nifal.md` §4 (Q2 resolved as a no-op, on measurement).
- **Suggested Fix**:
  - Census Starfield emittance with the §4 tooling, in a row of its own.
  - Decide the physical-luminance → canonical mapping from that measurement (or keep the arm parked), and either give
    Starfield its own `EmissiveSource` or document the conversion.
  - Delete the "natively" sentence unless it can be sourced.
  - Treat `LayeredEmissivityComponent` as the primary capture target, ahead of `EmissiveSettingsComponent`.

### LOW

#### NIFAL-D8-2026-10-09-03: `parse_loose_mat` and `MaterialIndex::lookup` are two producers of `CdbMaterial`, and 9bb6b7dce extended only the compiled one, so 16 of the 20 installed loose `.mat` files drop their authored `EmissiveSettingsComponent`
- **Severity**: LOW. This is Creation (mod) content only, and the arm would be inert anyway until D8-01 and D8-02 are
  settled.
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: single-boundary (the same authored component lowers differently depending on its container)
- **Game Affected**: Starfield (Creation archives)
- **Location**: `byroredux/src/asset_provider/material/loose_mat.rs:84-143`. The match has arms for `MRTextureFile`,
  `TextureReplacement`, `MaterialParamFloat`, `AlphaSettingsComponent`, `EffectSettingsComponent` (`IsGlass` only) and
  `Translucency`. There is no `EmissiveSettingsComponent` or `LayeredEmissivityComponent` arm, and no `BlendingMode` read.
  Compare the compiled side at `crates/sfmaterial/src/index.rs:492-539`.
- **Status**: NEW. #5396, which closed the layout defect, is verified. This is the parity gap it left once 9bb6b7dce landed
  the same evening.
- **Description**: All 20 installed loose files were extracted (to `/tmp/audit/nifal/matsamples/`):
  - All 20 decode (each carries `MRTextureFile`), so each one wins over the CDB.
  - 16 author `BSMaterial::EmissiveSettingsComponent`. These are the 11 QOG pawnshop-terminal parts and the 5 requisition-kiosk
    splash screens. None of them author `Enabled`: they inherit it from the shader model they `Import`. `ColorEmissive.mat`
    looks up with `Enabled = true`.
  - 1 (`lasersight_white.mat`) authors `LayeredEmissivityComponent` with `Enabled "true"` and a luminance of 432.
  - All of it is ignored. Their emissive maps, for example `TerminalCase_emissive.dds` at slot 7, bind at zero weight.
- **Evidence**: A JSON census of the extracted files (component-type counts: `EmissiveSettingsComponent` 16,
  `EmittanceSettings` 16, `LayeredEmissivityComponent` 1). `loose_mat.rs` has no arm for either component.
- **Impact**: Glowing terminal screens from installed Creations render unlit. The two decoders will keep drifting each time
  one of them gains a capture.
- **Related**: #5396 (closed), D8-01, D8-02, #5466 (open, the loose `.mat` doc text).
- **Suggested Fix**: Factor the per-component capture (class name plus field map → `CdbMaterial` fields) into one function
  that both decoders call. `serde_json` string-typed values and CDB `Value`s can both feed one small accessor trait. Then
  apply D8-01's inheritance rule and D8-02's emissive decision once, in that function.

## Existing open issues re-checked (not counted as new)
- **#5437** (OPEN): the inverted resolver-table row is still at `material_translate.rs:42` and `nifal.md:846`.
- **#4246** (OPEN): unchanged.
- **#5284** (OPEN): there are still 4 `lookup_cdb_material` tail copies (`merge.rs:559`, `:674`, `:785`, `:1457`). The loose arm
  now also routes through `apply_cdb_material`. D8-01's inheritance fix lands under all four through the shared lookup.
- **#5285** (OPEN): `nifal.md:137-140` still lists six beam converters.
- **#5466, #5465, #3398, #4256** (OPEN): unchanged by the window, except that #3398 is the natural home for D8-01's fix.

## Verified fixed in this window (for the next run's dedup)
- **#5396 / #5436** (`faf09c6e6`):
  - The decoder now reads the real `Objects[].Components[].Data` layout, and all 20 installed files decode.
  - An undecodable file falls through to `lookup_cdb_material` and `apply_cdb_pbr_fallback`, which set
    `external_material_resolved`.
  - One latent observation, not filed: the decoder flattens all objects in JSON order, with last-wins settings, while the
    lookup uses layer order and first-wins. No installed file has more than one texture-set object, so the difference does
    not show today.
- **#5381** (`46dfdcc6c`): CDB slot 6 is parked, pinned by `slot_height_texture_stays_parked_and_never_arms_parallax`, and
  `nifal.md:701` is updated.
- **#5389** (`cf9dec258`): `decline_unbounded_packed_indices` covers the classic `NiSkinData`, BSTriShape inline/SSE and FO4
  arms, and it declines rather than clamps. One cosmetic note: its warning reads "BSTriShape skin" on the `NiSkinData` arm too.
- **#5281** (`3031d8976`): one 3-argument texture-only body. The `terrain_lod_btr.rs:466` caller passes 3 arguments.
- **#5279 / #5282** (`dd8c1a1f1`): the NaN-classifier wording and the affected-node light-scope ledger row are both present.
- **#5210** (`7d1de4ad5`): the Phase-2 prose is reconciled. Its #5277/#5283 rows are now inert per D8-01.

## Streaming-area notes (priority paths for this suite)
- **#5359** (`edb5fbdfe`): `worn_skin.or(race.default_skin)` (`npc_spawn.rs:1315`) only chooses which ARMO feeds
  `resolve_armor_meshes`. The meshes still go through `load_nif_bytes_with_skeleton` → `spawn_nif_mesh` →
  `translate_material_with_provenance`, so there is no new `Material` producer. A spot check of the population
  (`SkinDraugrMale01..07` → `NakedDraugrBodyAA0N`) shows variants that differ by `MOD2` mesh (`DraugrMale0N.nif`), with no
  alternate-texture subrecord left untranslated.
- **#5358** (`479414ffe`): the BODT biped-mask decode feeds equip slots and `NpcAppearanceHidden`. It has no material-translation
  angle.
- **`npc_spawn/seam_blend.rs` + `seam_blend/{tone,vertex_grid}.rs`**: no commits since 2026-09-29.
  - The pass is a pre-spawn hook. It mutates raw-tier `ImportedMesh` normals and vertex colours on a private copy
    (`apply_spawn_hook`), before translation.
  - The head hook overwrites `ImportedMaterial.base_color` before translation. Neither builds a canonical type.
  - `is_skin_texture` is a spawn-time engine feature, gated to Oblivion/FO3/FNV at `resumable/runtime.rs:381`; it is not a
    render-time classifier.
  - Its constants are documented as engine tuning beyond legacy.
  - No finding. One edge was dropped for lack of evidence: tone baked into vertex colours would modulate emissive on a skin
    mesh whose `NiVertexColorProperty` is in Emissive mode. No census shows such a skin mesh exists.
- **`cell_loader/spawn.rs`, `spawn/mesh_instance.rs`**: no commits since baseline. The translate call (`:1114`) and the overlay
  call (`spawn.rs:1276`) are unchanged.
- **3bcf6c8e8 (#5482)**: this is not a NIFAL path; it was checked because the prompt named it. It adds one game-agnostic
  `gradeContrast` with cited constants: the vanilla ISHDR pivot and the Community Shaders ISHDR toe of 0.1. It has a Rust
  mirror and no per-game shader branch. It respects single-boundary and no-fabrication.

## Documented-limitation ledger (parked, not leaks)
The baseline ledger (`AUDIT_NIFAL_2026-10-08.md` and earlier) still stands:
- the 7 parked node fields;
- the BhkNP blob, phantoms and undecoded constraint kinds;
- `BhkPlaneShape` → `None`;
- the particle size-over-life curve and `initial_color`;
- ambient and morph animation channels;
- light affected-node scope (now recorded in `nifal.md`, #5282);
- the `NiSpotLight` inner angle and exponent;
- the structural 0% `tex`/`nrm` fill on FO76 and Starfield;
- Starfield CDB metalness and roughness (#3398).

New rows:
- The CDB `BlendingMode` strings other than AlphaBlend stay untranslated on purpose ("not guessed"). On referenced materials
  they are Additive (69), SourceSoftAdditive (67), DestinationInvertedSoftAdditive (2), DestinationSoftAdditive (1),
  Multiply (1) and TakeSmaller (1). These 141 effect materials are the whole live blend population.
- `AlphaSettingsComponent.Blender` (23,434 inline `AlphaBlenderSettings`) is an opacity-composition setting (Mode,
  VertexColorChannel, OpacityUVStream). It is not framebuffer blend state, and it is untranslated.

The baseline's "loose `.mat` is recognition-only" row is retired: #5396 decodes all 20 files.

## Dimension 9: harness (release, `--ignored`, all 8 games; 2 passed in 63.50 s)
The table is identical to `AUDIT_NIFAL_2026-10-08.md` § Dimension 9 (Oblivion 567 … Starfield 811 meshes; every floor and
ceiling passes; 8 dark-role hits, all in Oblivion). No extractor changed in the window.

## Method notes
- **Tests at HEAD** (toolchain 1.96.0):
  - `cargo test -p byroredux --bin byroredux`, filtered to `material_translate`, `merge::`, the animation harness, the HKX
    sample, overlay, secondary-role walk, tint-alpha, spawner/marker guards, `seam_blend` and `npc_spawn::`: **350 passed**,
    13 ignored.
  - `cargo test -p byroredux-nif --lib`, filtered to dispatch coverage, role/value counts, light dispatch, walkers,
    `slot_role`, decal, tangent, units and skin: **209 passed**.
  - `cargo test -p byroredux-sfmaterial --lib`: 38 passed.
  - The D9 harness: 2 passed.
- **CDB measurements**:
  - Material names came from the repo's `sf_matpath_dump` example (10,380 names, 0 limit).
  - The CDB measurements used a scratch crate outside the repo (`/tmp/audit/nifal/cdbprobe`). It has path dependencies on
    `byroredux-sfmaterial` and `byroredux-bsa`, uses public APIs only, and ran under `ulimit -v`. The full-index walk peaked
    at 7.0 GB RSS for 6 s. Outputs are `cdb_census_all_ext.txt`, `cdb_raw{,2,3,4b}.txt`, `cdb_look2.txt` and
    `{glass,coloremissive}_chain_*`.
  - Skyrim ARMO/ARMA records were dumped with the repo's `dump_record_subs` example.
- **No engine launch**, no plugin `--ignored` tests, and no repo edits besides this file.
- **Dedup**:
  - `/tmp/audit/issues.json` (147 open).
  - `gh issue list --state closed --search` for "NIFAL in:title", "NIFAL-D8-2026-10-08", "loose .mat",
    "external_material_resolved", "ParentPersistentID", "CDB parent inheritance", "shader model CDB" and "alternate textures".
  - `gh issue view` on #5277, #5283, #5196, #3398 and #5436.
  - None of this suite's already-reported findings (EXT / PERF / CONC / SAVE) touches the merge boundary.
- **Skill staleness** (route to `/audit-sync`): Dim 8 should list `loose_mat.rs` and `crates/sfmaterial/src/index.rs` among
  its paths. Its Starfield `.mat`/CDB bullet should add `BlendingMode` → `has_alpha` and the `EmissiveSettingsComponent` capture
  (9bb6b7dce), and note that the lookup does not resolve CDB inheritance.

**Suggested publish labels** (domain `nifal` on all):
- **D8-01**: `medium` `bug` `import-pipeline` `game:starfield`
- **D8-02**: `medium` `bug` `import-pipeline` `game:starfield`
- **D8-03**: `low` `bug` `import-pipeline` `tech-debt` `game:starfield`

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-10-09.md
```
