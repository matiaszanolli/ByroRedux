# Batch: 5446 5459 5480 5502 5509 5534

---
## # PAR-D4-2026-10-08-01: three of the four HKX dimension-gate tests cannot fail if their own clause is removed [OPEN]

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-08.md` — `PAR-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Corpus Gates (test strength)
- **Location**: `crates/hkx/src/animation.rs:1538-1604` and `:1655-1711`, the fixtures of:
  - `decode_spline_animation_rejects_a_sample_count_bomb`
  - `decode_spline_animation_rejects_frames_beyond_the_declared_blocks`
  - `decode_spline_animation_rejects_a_block_claiming_its_whole_clip`

  The gate they target is `animation.rs:368-395`.
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness).
- **Description**:
  - Every clause of the dimension gate returns the same `InvalidData("unsupported spline clip dimensions")`.
  - Three fixtures leave `mask_size` (offset `0x44`) at 0, so `mask_size != transform_count * 4 + float_count` rejects them whatever clause each test claims to pin.
  - The #5006 fixture also declares 15,998,976 samples, which the #5316 1 M cap rejects on its own.
  - The test comments claim "ONLY the relative check can reject it" (#4655) and "only the MAX_FRAMES_PER_BLOCK ceiling can reject it" (#5006). Both claims are false.
  - Only the new #5316 test, `rejects_the_279kb_spline_probe`, sets `mask_size` correctly and isolates its clause.
- **Evidence**: a mutation probe. In a scratch copy of `crates/hkx`, both the #4655 clause (`num_frames > num_blocks * (mfpb - 1) + 1`) and the #5006 clause (`mfpb <= MAX_FRAMES_PER_BLOCK`) were deleted. `cargo test --lib decode_spline_animation_rejects` then reports **4 passed, 0 failed**.
- **Impact**: the #4655 and #5006 hardening (one-block bypass, frames-vs-blocks tie) can regress with a green unit lane. The real-data census asserts only vanilla maximums, not rejection of hostile shapes.
- **Related**: #4655, #5006, #5316, #5326 (census weakness, separate).
- **Suggested Fix**:
  - Set `mask_size = transform_count * 4` in each fixture.
  - Size the #5006 fixture under 1 M samples, for example 4 tracks × 250,000 frames in 1 block with mfpb 250,001.
  - Ideally return a distinct error string per clause, so each test asserts the clause it pins.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other HKX gate test sharing the single `unsupported spline clip dimensions` error)
- [ ] **TESTS**: A regression test pins this specific fix


---
## # SAVE-D2-2026-10-08-02: the `FORMAT_MAJOR` bump ledger in `snapshot.rs` stops at v32 while the constant is 33 [OPEN]

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D2-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: the ledger's last entry is `v31 -> v32`; `FORMAT_MAJOR` is `33`.

- **Severity**: LOW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none (doc-rot)
- **Location**: `crates/save/src/snapshot.rs:246-262`; v33's rationale exists only in `serde_default_guard_tests.rs:739-743`.
- **Status**: NEW
- **Description**: `14cff35ae` changed only the constant line. The skill and the crate doc treat this comment as *the* bump ledger. The v33 note in the guard comment also says pre-v33 saves can "legitimately re-qualify" a line after load, but pre-v33 saves are rejected outright, by both the major and the schema fingerprint.
- **Suggested Fix**: Add a `v32 -> v33 (#5367 Phase L): new saved resource DialogueSpokenInfoForms …` entry, and correct the guard comment's wording.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


---
## # UI-D4-2026-10-08-01: the shim normalises only the navigator's import scan; Ruffle receives dependency movies un-normalised, contrary to the shim's documented contract [OPEN]

**Source**: `docs/audits/AUDIT_UI_2026-10-08.md` — `UI-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW. No visible effect today.
- **Dimension**: Resource Navigator
- **Profile**: Starfield (and any future dependency movie in the same dialect)
- **Location**:
  - `crates/ui/src/navigator.rs:252-313` (`load_archive_resource`)
  - `crates/ui/src/navigator.rs:524-536` (`import_asset_paths`)
  - `crates/ui/src/prepare.rs:35-53` (doc: "normalize … before Ruffle sees it")
- **Status**: NEW
- **Description**:
  - `import_asset_paths` normalises a private copy only to run its `swf::parse_swf` scan.
  - `load_archive_resource` then returns `body`, which is either the raw archive bytes or `prepare_import_asset_swf`'s
    raw-tag rewrite (never shimmed), as the `MemoryResponse` Ruffle parses.
  - The 9813af435 commit message ("dependency movies go through the same shim in the navigator") and the skill's Dim 1
    line both overstate this.
  - It is inert today:
    - The only fatal consumer of the bad records was this crate's own `swf::parse_swf`.
    - Ruffle's runtime `tag_utils::decode_tags` logs `Error running definition tag` and continues per tag (pinned Ruffle
      `0dde981`, `core/src/tag_utils.rs:75-78`).
    - So a dependency's class-name PlaceObject3 is skipped, while the root's is turned into an inert Modify.
- **Impact**:
  - The root and its dependencies take two different code paths for the same records.
  - The commit's live verification ("zero parse failures") counted only `log` lines. Ruffle's `tracing` errors for
    dependency records would not appear there.
  - A future upstream fix that models class-name placement through the shim would cover the root only.
- **Related**: #4470, UI-D1-2026-10-08-01
- **Suggested Fix**: apply `normalize_scaleform_dialect` to `body` in `load_archive_resource`, once, and reuse the
  result for the import scan. This also removes UI-D1-01's extra scan inflate. Alternatively, correct the docs to say
  that only the scan is normalised.

## Completeness Checks
- [ ] **SIBLING**: Root and dependency movies take the same normalisation path
- [ ] **TESTS**: A regression test pins this specific fix


---
## # NIFAL-D8-2026-10-09-01: The CDB index resolves no `ObjectInfo.Parent` inheritance, so inherited `IsGlass` and emissive `Enabled` read as absent, and both #5277 and #5283 fire on 0 of 10,158 referenced Starfield materials [OPEN]

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

---
## # SF-2026-10-09-D6-01: #5277 forwards only `BlendingMode == "AlphaBlend"` (0 vanilla shapes); the 491 shapes whose CDB material authors an additive-family mode render as opaque lit surfaces, and nifal.md has no parked row for them [OPEN]

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-09.md` — finding `SF-2026-10-09-D6-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM (the "translatable data silently dropped" row; visible artifacts on effect cards)
- **Dimension**: Material Flow (NIFAL boundary), CDB arm
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:331-351`: the comment reads "any other authored string stays untouched until its mapping is sourced", and the gate is `:348`;
  - `docs/engine/nifal.md:710` (records only the AlphaBlend arm);
  - capture `crates/sfmaterial/src/index.rs` (`effect_blend`).
- **Status**: NEW. #5277 (closed by 9bb6b7dce) scoped AlphaBlend only. NIFAL-D8-01 (this suite) covers the inheritance gap that leaves AlphaBlend and IsGlass inert. It does not cover these modes, and they are live without inheritance. No issue mentions Starfield additive blending.
- **Description**:
  - Starfield shapes almost never carry `NiAlphaProperty` (884 blocks), so the CDB `EffectSettingsComponent.BlendingMode` string is their only blend signal.
  - 9bb6b7dce captures it for every material, but forwards it only for `"AlphaBlend"`.
  - Every other authored mode leaves `has_alpha == false`. The surface goes down the opaque, depth-writing, lit path, and its glow sprite becomes a solid card.
- **Evidence**: The probe (`/mnt/data/tmp/starfield-audit-probe`, release, MemoryMax=4G; output `/tmp/audit/starfield/probe/blend_alpha.out`) visited every `BSGeometry` in `Meshes01/02/Patch` (344,355 shapes), looked up its shader-property material in the vanilla base CDB, and crossed the result with `NiAlphaProperty`:

  | `BlendingMode` | shapes | NiAlphaProperty | samples |
  |---|---|---|---|
  | Additive | 182 | none | `shipwep_*_projectile_*` (`ShipBallisticBoltCore`, `ShipEM_SwirlGlow_01`, `ShipKineticCannonGlow`), `starborntempleintpedestal_b02` (`SBStoneTrimGlowFXOff`) |
  | SourceSoftAdditive | 277 | none | `smod_fx_enginemain_*` (`EngineGlowFlames.mat`), `spacemine.nif` (`RadialGlow_01_Red`) |
  | DestinationInvertedSoftAdditive | 29 | none | `nathemast_topwaterfall.nif` (`Water\Waterfall01.mat`) |
  | DestinationSoftAdditive / Multiply / TakeSmaller | 1 / 1 / 1 | none | `effects\debugfiles\testblendingmodes_*` |
  | **AlphaBlend** | **0** | — | — |

  All 491 shapes sit on `BSLightingShaderProperty` stubs. Raw CDB (this suite's NIFAL probe, `cdb_raw.txt`): `BlendingMode` is authored on 526 `EffectSettingsComponent`s (Additive 234, SourceSoftAdditive 283, AlphaBlend 2).
- **Impact**:
  - Ship engine flames, weapon projectile glows, space-mine glows and the New Atlantis MAST waterfall render as opaque, depth-writing cards instead of additive effects.
  - This is visible wherever ships and those set pieces appear.
- **Related**: #5277 (closed), NIFAL-D8-01 (inheritance; would add more hits), NIFAL-D8-02 (emissive on the same effect materials), #1651 / #1823 (the FO4 BGSM additive history).
- **Suggested Fix**:
  - Record the dropped modes now as a named parked row in nifal.md, with these counts.
  - Then source the mapping:
    - **Additive / Multiply.** The same studio's FO4 BGSM preset vocabulary exists locally (`/mnt/data/src/reference/Material-Editor/MaterialLib/BaseMaterialFile.cs:363-427`: Additive = (SRC_ALPHA, ONE), Multiplicative = (DEST_COLOR, ZERO)). The renderer already keys those pipelines (`crates/renderer/src/vulkan/pipeline.rs:752-755`). Confirm the cross-game carry-over against a capture before landing it.
    - **The soft variants and `TakeSmaller`.** These have no local source and stay parked until one is found.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary (`translate_material` / `Material::resolve_pbr`) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
## # SF-2026-10-09-D4-03: `game-compatibility.md` and `lighting-from-cells.md` still describe Starfield's 108-byte XCLL as "the Skyrim 92-byte layout plus a 16-byte tail" that is ignored [OPEN]

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-09.md` — finding `SF-2026-10-09-D4-03` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (doc rot)
- **Dimension**: ESM Resolve Rate + Cell Bring-up
- **Location**: `docs/engine/game-compatibility.md:284-286`; `docs/engine/lighting-from-cells.md:139-142`
- **Status**: NEW. #2364 (closed) fixed only a test assertion message, and #1293 (closed) shipped the decode. Neither touched these two docs.
- **Description**: The code decodes Starfield's own layout:
  - `crates/plugin/src/esm/cell/walkers.rs:36-40`: "NOT 'Skyrim + 16-byte tail' — Starfield's XCLL shares only bytes 0-39 with Skyrim and then diverges into a distinct volumetric height-fog model";
  - `parse_cell_starfield_xcll_decodes_volumetric_height_fog_tail`;
  - #5002's LGTM twin.

  The two docs still say the dispatch "reads the Skyrim 92-byte prefix and ignores the trailing 16 bytes for now". The skill records this framing as stale.
- **Impact**: A reader is told Starfield cells carry Skyrim's ambient cube / specular / fresnel and drop the height fog, which is the opposite of the truth.
- **Related**: #1291, #1293, #5002, #5346 (the two SF 108-byte decoders).
- **Suggested Fix**: Rewrite both passages to the SF1 layout (bytes 0-39 shared; 40-107 height-fog model) and cite `walkers.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix

