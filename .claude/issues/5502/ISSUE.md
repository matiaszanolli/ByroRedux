# #5502: NIFAL-D8-2026-10-09-01: The CDB index resolves no `ObjectInfo.Parent` inheritance, so inherited `IsGlass` and emissive `Enabled` read as absent, and both #5277 and #5283 fire on 0 of 10,158 referenced Starfield materials

**Labels**: bug, game:starfield, import-pipeline, medium, nifal

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-09.md` — finding `NIFAL-D8-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary (`translate_material` / `Material::resolve_pbr`) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
