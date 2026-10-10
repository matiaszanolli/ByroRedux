**HEAD**: 3bcf6c8e8 · **Baseline**: docs/audits/AUDIT_STARFIELD_2026-10-08.md (HEAD 00f580e09) · **Audited**: Dims 2, 3, 4, 6 (delta-touched), plus the streaming-deep area as it applies to Starfield (cell load / Cydonia, BSGeometry `.mesh` resolution through the stream worker, material resolution at cache fill / spawn, 9bb6b7dce) · **Unchanged since baseline (skimmed)**: Dims 1, 5 (no commits on their paths; guards run)

# Starfield Compatibility Audit — 2026-10-09

This report is one leg of `/audit-suite --preset streaming-deep`. One auditor ran every dimension in sequence, with no sub-agents.

The window is `00f580e09..3bcf6c8e8` (81 commits). The Starfield-relevant ones are:
- material: 46dfdcc6c (#5381, the 10-08 HIGH), faf09c6e6 (#5396 loose `.mat` layout), 7d1de4ad5 (#5210 docs), 3031d8976 (#5281), 9bb6b7dce (#5277/#5283 CDB blend + emissive capture);
- ESM / cell: a5484faf6 (#5364 WTHS), 302394f94 (#5424), 2dab4ff48 (#5170/#5171), 0d436e642 (#5422 PKIN VNAM);
- mesh: cf9dec258 (#5389, `skin.rs`).

Constraints honoured:
- No engine launch, no `--sf-smoke`, no `--ignored` plugin tests.
- No source, skill or issue edits.
- One CDB index build at a time, under `systemd-run … MemoryMax`.

Real-data probes (all read-only):
- Three Python GRUP walks of `Starfield.esm` (PKIN / CNAM / template-cell census, XPCS census, template child base types, CELL count). Scripts in `/tmp/audit/starfield/probe/`.
- A Rust probe that joins every `BSGeometry` in `Meshes01/02/Patch` to its CDB material. Crate and target dir: `/mnt/data/tmp/starfield-audit-probe/`.
- A BA2 name-table listing of the four Creation archives.
- `real_cdb.rs --ignored`.

## Guard runs (all green)

| Suite | Result |
|---|---|
| bsa `--lib ba2` | 44/44 |
| nif `--lib bs_geometry` / `-- shader::tests::starfield` / `--lib starfield` / `--lib skin` | 72 / 11 / 32 / 116 (1 ign) |
| sfmaterial (lib + header_smoke + integration) | 38 + 6 + 1 (3 ignored) |
| **sfmaterial `real_cdb.rs --ignored`** (release, real data, MemoryMax=3G) | **3/3 in 4.06 s**: #5320's strict build accepts the base and SFBGS007 CDBs with 9bb6b7dce's new capture arms |
| plugin `starfield` / `wths` / `spatial_units` / `txst` / `dat2` / `xcll` / `lgtm` / `pkin` | 21 (1 ign) / 2 / 9 / 15 / 7 / 25 (2 ign) / 5 / 11 (1 ign) |
| byroredux `starfield_mat` / `asset_provider::material` / `material_translate` / `loose_mat` / `cdb` | 17 / 22 / 77 / 6 / 28 |
| byroredux `light_anim` / `normalize_mesh_path` / `starfield` / `pre_parse` / `single_boundary` | 31 / 8 / 32 / 2 / 2 |

## Executive Summary

Starfield is a first-class `GameKind`:
- **NIF + BA2**: the readers are unchanged since the 10-08 measurement (100% over 120,543 NIFs).
- **CDB Phase 2 is partial.** Translated: roles, alpha, glass and flat colour. The 10-08 HIGH (slot 6 arming POM) is fixed by #5381. Parked: scalars and the five single-channel kinds.
- **Walkable Cydonia** is not re-verified, because no engine launch is allowed.

**Delta outcome: 0 CRITICAL, 1 HIGH, 1 MEDIUM, 2 LOW (all NEW).**

**Headline (HIGH, SF-D4-01): every Starfield pack-in placement renders nothing.**
- `Starfield.esm` has 11,281 PKIN records, and every CNAM is a template CELL.
- 97,981 REFRs place a PKIN. Behind them, the template cells hold 1,211,969 child REFRs, one level deep.
- The cell loader skips every template CELL with a warn (#5231's FO4 fix). #5231 rests on the premise that vanilla data places zero PKINs, which was measured on FO4 only.
- Cydonia alone loses 12,218 template children (98.9% STAT) behind 370 PKIN REFRs: directory signage, engineering greebles, storage tanks and structural trusses. That is 44% on top of the cell's 27,823 own REFRs.
- The resolve-rate metric is blind to all of it, because PKIN registers a nominal empty-model static (SF-D4-02).

**SF-D6-01 (MEDIUM): only one CDB blend mode is forwarded.**
- #5277 forwards `BlendingMode == "AlphaBlend"`, and no vanilla shape authors it.
- The 491 `BSGeometry` shapes whose CDB material authors `Additive`, `SourceSoftAdditive` or `DestinationInvertedSoftAdditive` all lack `NiAlphaProperty`, so they render as opaque lit surfaces. They include ship engine flames, projectile glows, space-mine glows and New Atlantis' MAST waterfall.
- nifal.md has no parked row for these modes.
- This finding is live today and independent of NIFAL-D8-01's inheritance gap.

## Dimension Summaries

| Dim | Verdict | Notes |
|---|---|---|
| 1 BA2 + corpus | unchanged | 0 commits; `ba2` 44/44 |
| 2 BSGeometry | clean | #5389 does not touch `convert_bs_geometry_skin_weights` (`skin.rs:350-380`), which still declines and never clamps. Every import site passes the `.mesh` resolver: the worker (`streaming/pre_parse.rs:314`) and the main thread (`references/import.rs:89`, `synth_child.rs:650`, `authored_cover.rs:98`, `precombined.rs:367`) |
| 3 CDB | clean (delta) | #5381 verified. 9bb6b7dce's capture gaps are already filed this suite (NIFAL-D8-01/02/03). #5465 is open and unchanged. #5466 is open and partly fixed (see below) |
| 4 ESM + bring-up | 1 HIGH, 2 LOW | SF-D4-01 (pack-in instancing), SF-D4-02 (metric), SF-D4-03 (XCLL doc rot). #5171, #5422 and #5364 verified |
| 5 NIF shader blocks | unchanged | 0 commits; 11/11 + 32/32 |
| 6 Material flow | 1 MEDIUM | SF-D6-01. Provenance (`external_material_resolved` on all CDB and loose arms, `from_bgsm` clear) is intact. The MSWP `.mat` swap re-merges through `merge_external_material` (#4290) |

Per-dimension detail: `/tmp/audit/starfield/dim_{1..6}.md`.

---

## Findings

### HIGH

### SF-2026-10-09-D4-01: Starfield pack-in REFRs spawn nothing, because template-CELL instancing is unimplemented and #5231's "0 vanilla PKIN REFRs" premise holds for FO4 only; Cydonia loses 12,218 template placements
- **Severity**: HIGH. Visible game content is dropped under normal conditions: every Starfield cell that places a pack-in is affected (3,557 cells), including the reference cell.
- **Dimension**: ESM Resolve Rate + Cell Bring-up (streaming-area: cell load / REFR expansion)
- **Location**:
  - `byroredux/src/cell_loader/refr.rs:586-600` (every CELL-typed CNAM gets `log::warn!` + `continue`);
  - `refr.rs:529-563` (the doc says "vanilla data contains **zero PKIN-based REFRs** and this expander only ever runs on mod-authored placements");
  - `refr.rs:652-669` (`index_resolves_to_cell`, a linear scan over every CELL, justified as "a cold path — vanilla data carries zero PKIN-based REFRs");
  - call site `byroredux/src/cell_loader/references/mod.rs:613`;
  - fallthrough `byroredux/src/cell_loader/references/synth_child.rs:377`.
- **Status**: NEW. #5231 (closed, d63131c57, 2026-10-07) corrected the FO4 data model and turned the CELL-typed CNAM into an explicit skip; its census covered Fallout4.esm and the FO4 DLC only. No open or closed issue covers instancing a pack-in's template cell. The FO4 audits (10-03 → 10-09) list it under FO4 "deeper coverage", which is mod-only there. No Starfield report has flagged it; four of them (06-23, 07-25, 08-16, 08-24) list "PKIN 370" among the *resolved* Cydonia types.
- **Description**:
  - Starfield's PKIN is `wbFormIDCk(CNAM, 'Cell', [CELL])` (xEdit `wbDefinitionsSF1.pas:16001`), the same template-cell model as FO4.
  - Unlike FO4, Starfield does not bake a placed pack-in into ordinary REFRs. The REFR keeps the PKIN as its base, and the game instances the template cell's references under the REFR's transform. The PKIN flags include `Instanced` / `Instanced Static`, `wbDefinitionsSF1.pas:15983-15985`.
  - The expander skips every CNAM (all are CELLs) and returns `None`. The outer REFR then falls to `expand_scol_placements`' single-entry default. Its base is the nominal empty-model static from `parse_pkin_group`, so `synth_child.rs:377` spawns only a logical quest reference.
  - The template cell's REFRs are never read.
- **Evidence** (read-only GRUP walks of `Starfield.esm`; scripts `/tmp/audit/starfield/probe/{pkin_census,xpcs_census,pkin_template_types,cell_count}.py`):
  - **PKIN records**: 11,281. CNAM entries: 11,281, and **11,281 resolve to a CELL**.
  - **Placements**: 97,981 of 3,291,860 REFRs have a PKIN base, spread over 3,557 cells. Their template cells hold **1,211,969 child REFRs** (one level; 0 templates are empty).
  - **`citycydoniamainlevel` (0x002B3DA2)**: 27,823 own REFRs, of which 370 are PKIN-based (138 distinct PKINs). Their template cells hold **12,218 child REFRs**:
    - by base type: STAT 12,081 · PDCL 74 · nested PKIN 40 · MSTT 12 · ACTI 5 · SOUN 3 · MISC 3;
    - largest templates: `Cydonia_Sign_Directory` 363, `ClutterPI_EngineeringGreebA03` 242, `PI_StorageTank_LG01` 235, `ClutterPI_EngineeringGreebA02` 165;
    - most-placed PKIN: `SCOL_StructKit_Truss03`, ×79.
  - **The children are not baked elsewhere.**
    - Only 181 Cydonia REFRs carry `XPCS` ("Source Pack-in", xEdit SF1:7051), from 18 sources.
    - Only 8 of the 138 placed PKINs are ever an `XPCS` source in that cell.
    - Globally, 267,507 REFRs carry `XPCS`, against 1.21 M template children.
- **Impact**:
  - Cydonia renders without about 12k static pieces of its authored dressing (signage, greebles, tanks, trusses), and the same holds for every Starfield cell and exterior that places a pack-in.
  - Pack-in template cells are 11,281 of Starfield's 11,985 interior CELLs.
  - Every load also emits one `warn!` per PKIN REFR (370 on Cydonia) and runs a linear CELL scan per CNAM, over 11,985 interior + 18,732 exterior CELLs. The "cold path" premise that justifies that scan is false for Starfield.
  - Under the baseline doc's own frequency rule, this outranks PDCL (74.9% of the *unresolved* Cydonia REFRs).
- **Related**: #5231 (closed), #589 / #1180 / #2611 (expander history), SF-D4-02 (why the gate missed it), FO4 audits' "PKIN template-CELL instancing" line, ESM 10-08 table row "PKIN CNAM = template CELL".
- **Suggested Fix**:
  - Instance the template cell's `references` under the outer REFR transform, composed exactly like the SCOL arm (`GlobalTransform::compose_trs`). Recurse into nested PKINs under the shared `MAX_PKIN_DEPTH`, and memoise the expanded child list per PKIN.
  - Replace `index_resolves_to_cell`'s scan with a `CELL form_id → cell` map.
  - Correct the "zero PKIN-based REFRs" docs to say "FO4 only".

### MEDIUM

### SF-2026-10-09-D6-01: #5277 forwards only `BlendingMode == "AlphaBlend"` (0 vanilla shapes); the 491 shapes whose CDB material authors an additive-family mode render as opaque lit surfaces, and nifal.md has no parked row for them
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

### LOW

### SF-2026-10-09-D4-02: `sf_smoke` counts PKIN-based REFRs as "resolved" through the nominal empty-model `StaticObject`, so the Cydonia resolve rate hides the whole pack-in gap
- **Severity**: LOW (measurement / test gap)
- **Dimension**: ESM Resolve Rate + Cell Bring-up
- **Location**: `crates/plugin/src/esm/cell/support.rs:892-918` (nominal `StaticObject`, `model_path: String::new()`, `RecordType::PKIN`); `byroredux/src/sf_smoke.rs:185-195` (any `statics` hit counts as resolved)
- **Status**: NEW
- **Description**:
  - `parse_pkin_group` registers every PKIN as an empty-model static, so that the expander can find the base.
  - `sf_smoke` treats any `statics` hit as resolved, so the 370 Cydonia PKIN REFRs count toward the 91.2% headline even though they spawn nothing.
  - Their 12,218 template children never enter the metric.
  - This is why four Starfield reports (06-23, 07-25, 08-16, 08-24) listed "PKIN 370" as resolved without anyone noticing SF-D4-01.
- **Evidence**: As above; the by-type rows in the 06-23, 07-25, 08-16 and 08-24 reports.
- **Impact**: The skill's Dim 4 regression gate cannot see SF-D4-01, or a fix for it.
- **Related**: SF-D4-01, #2637 (the precedent for a separate known-but-not-rendered bucket).
- **Suggested Fix**: Report PKIN-based REFRs in their own bucket with their template child count ("pack-in: N REFRs → M template children, instanced / not instanced"), not as resolved.

### SF-2026-10-09-D4-03: `game-compatibility.md` and `lighting-from-cells.md` still describe Starfield's 108-byte XCLL as "the Skyrim 92-byte layout plus a 16-byte tail" that is ignored
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

---

## Already-tracked items re-checked (not counted as NEW)

- **Fixed this window, verified**:
  - **#5381**: the 10-08 HIGH, SF-D6-01. `SLOT_HEIGHT` falls to the parked arm (`merge.rs:265-279`), pinned by `slot_height_texture_stays_parked_and_never_arms_parallax`.
  - **#5396 / #5436**: the loose `.mat` real layout. An undecodable file falls through to the CDB, and the loose arm sets `external_material_resolved` via `apply_cdb_material`.
  - **#5171**: the LGTM and XCLL lift is pinned through `parse_esm`.
  - **#5422**: `version: u32` is stored raw, matching xEdit SF1 `wbInteger(VNAM,'Version',itU32)`.
  - **#5281**.
  - **#5389**: the Starfield producer is untouched.
- **Open, unchanged**:
  - **#5465** (10-08 D3-01): `index.rs:662` / `:678`.
  - **#3398**, **#5291** (lazy main-thread index build; it lands inside `FinishImports` on the first Starfield NIF), **#5284**, **#1576**, **#5301**, **#5346**, **#5343**, **#5170** (capture-gated by design; the deferral is now pinned).
- **#5466** (10-08 D3-02), open and **partly fixed**:
  - faf09c6e6 moved the misplaced doc block.
  - These sites remain: `cdb.rs:7-8`, `cdb.rs:280-281` (the per-session INFO log), `merge.rs:38`, `merge.rs:477-483` ("Phase 1 (this commit)"), `merge.rs:643` ("not yet parsed (tracked in #4277)").
  - A sibling site not on #5466's list: `cell_loader/refr.rs:255-270`. The #2708 comment there says "Starfield content resolves no textures from either resolver". The code is harmless, because a swap target is re-merged through `merge_external_material` (`mesh_instance.rs:165-190`, #4290), so `.mat` swaps do reach the CDB. The site belongs with #5466.
- **Filed this suite by other audits, Starfield-relevant**:
  - NIFAL-D8-2026-10-09-01 (the CDB `Parent` inheritance gap; AlphaBlend and IsGlass inert);
  - NIFAL-D8-02 (raw `LuminousEmittance`, `LayeredEmissivityComponent` uncaptured);
  - NIFAL-D8-03 (loose-decoder parity). The four Creation `- main.ba2` archives carry 20 `.mat` files and no Creation CDB, so for those paths the loose file is the only source;
  - ESM D2-01 (WTHS DIFF walker), ESM D3-01 (SCOL/PKIN `FLTR` zstring, Shattered Space);
  - CONC-D7-02 (pre-parse skip memo).
- **Streaming-area notes**:
  - The stream worker resolves external `.mesh` through the immutable `TextureProvider` (`pre_parse.rs:314`). Those bytes sit outside `STREAM_PARSE_INPUT_BYTES`, as documented at `pre_parse.rs:395-397`.
  - Starfield exteriors are still a policy skip, so no Starfield tile reaches the worker today.
  - Material resolution is main-thread only and runs once per unique NIF: `partial.rs:109` and `references/import.rs:97`, both through `merge_external_materials`.

## CRC32 Flag Table

Unchanged. `crates/nif/src/shader_flags.rs` has had no commits since 2026-09-16; the full 32-row derivation is in `AUDIT_STARFIELD_2026-09-11.md` / `-09-16.md`. The #4279 `Own_Emit` dual read is still in place, and no vanilla block sets it.

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md`, Phases 0+1 are done, and Phases 2–4 were invalidated by the parity measurement. Both parsers have shipped, and the NIF truncation tail is cleared.

0. **Pack-in template-cell instancing** (**SF-D4-01**). This is new and leads the chain on frequency: 1.21 M template REFRs game-wide, and 12,218 in Cydonia against PDCL's share of the unresolved. Fix the metric alongside it (SF-D4-02).
1. **CDB Phase 2 remainder** (#3398):
   - resolve `Parent` inheritance (NIFAL-D8-01);
   - translate or explicitly park the additive-family blend modes (**SF-D6-01**);
   - an emissive scale decision plus `LayeredEmissivityComponent` (NIFAL-D8-02);
   - canonical roles for the five parked kinds (#4429);
   - `MaterialParamFloat` → roughness/metalness;
   - a `UseSSS` colour source;
   - one shared component-capture function for the loose and compiled decoders (NIFAL-D8-03);
   - an off-main-thread index build (#5291).
2. **PDCL ahead of GBFM.** Unchanged.
3. **Exterior worldspace tiles.** WTHS now decodes its schema (#5364); interpreting the instance half is ESM D2-01's territory.
4. **Space-cell / planet / GBFM records.** Unchanged.

## Coverage Notes

- **Not run**:
  - `--sf-smoke` and the engine binary, which are forbidden. SF-D4-01 was therefore measured from raw ESM data, not from a render.
  - `parse_rate_starfield_all_meshes`, because the readers are unchanged.
  - The `sf_smoke` GRUP-coverage example.
- **Probes**:
  - `/tmp/audit/starfield/probe/*.py`: Python with `-I`, read-only `mmap`, run against `Starfield.esm`, plus a BA2 name-table lister.
  - `/mnt/data/tmp/starfield-audit-probe/`: a scratch Rust crate with its own target dir, built in release. It ran once, under `MemoryMax=4G`, building one `MaterialIndex`.
  - `real_cdb.rs` was run under `MemoryMax=3G`.
- **Scratch**: `/tmp/audit/starfield/dim_{1..6}.md`, `guards_crates.txt`, `guards_bin.txt`, `probe/`.
  - Each finding traces to its scratch note: D4-01, D4-02 and D4-03 to `dim_4`; D6-01 to `dim_6`.
  - Dims 1, 2, 3 and 5 are clean.
  - Two notes were dropped with a reason recorded in `dim_3`: the `effect_blend` and_modify / or_insert inconsistency, which has no effect on vanilla data; and the `refr.rs` #2708 stale comment, which is folded under #5466.

## Total Findings Summary

| Severity | NEW |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 2 |
| **Total** | **4** |

| ID | Sev | Title | Status |
|---|---|---|---|
| SF-2026-10-09-D4-01 | HIGH | Pack-in REFRs spawn nothing; template-CELL instancing unimplemented (97,981 REFRs / 1.21 M children; Cydonia −12,218) | NEW |
| SF-2026-10-09-D6-01 | MEDIUM | Only `BlendingMode == "AlphaBlend"` forwarded (0 shapes); 491 additive-family shapes render opaque; no parked row | NEW |
| SF-2026-10-09-D4-02 | LOW | `sf_smoke` counts PKIN REFRs as resolved via the nominal empty-model static | NEW |
| SF-2026-10-09-D4-03 | LOW | Two engine docs still call SF XCLL "Skyrim 92 + ignored 16-byte tail" | NEW |

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-10-09.md`. Label every finding `game:starfield` + `legacy-compat`, and add:
- D4-01: `high` `bug` `esm-plugin` `import-pipeline`;
- D6-01: `medium` `bug` `nifal` `import-pipeline`;
- D4-02: `low` `test-gap` `esm-plugin`;
- D4-03: `low` `documentation` `doc-rot`.
