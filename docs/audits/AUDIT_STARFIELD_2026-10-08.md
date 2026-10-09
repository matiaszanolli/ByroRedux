**HEAD**: 00f580e09 · **Baseline**: docs/audits/AUDIT_STARFIELD_2026-09-29.md (HEAD 9fcfdc3fc) · **Audited**: Dims 1, 2, 3, 4, 6 (each had commits in the window; 3, 4 and 6 carried the substantive change) · **Unchanged since baseline (skimmed)**: Dim 5 (one comment-only commit; guards run)

# Starfield Compatibility Audit — 2026-10-08

This report is one leg of `/audit-suite --preset comprehensive`. One auditor ran all six dimensions, with no sub-agents.

The window is `9fcfdc3fc..00f580e09` (2026-09-29 → 2026-10-08), larger than other audits' windows. It contains the whole of CDB Phase 2 (#3398):
- `224a19372`: the streaming `MaterialIndex` and the XMCOLOR offset fix;
- `18fce7e43`: role translation;
- `978d25c19`: scalars, per-slot replacements and flags;
- the follow-on fixes #5319, #5320 and #5323.

It also contains the loose `.mat` resolver (#4277), the fixes for all three 09-29 Dim-4 findings, and the WTHS stand-in (#5363).

Constraints honoured:
- No engine launch, and no `--sf-smoke`.
- No `--ignored` plugin tests.
- No edits to source, skills or issues.

The bounded real-CDB tests (`crates/sfmaterial/tests/real_cdb.rs`, release, `--test-threads=1`) were run. One schema probe was run: a read-only Python walk of the CLAS chunks in the vanilla base CDB, extracted to scratch and deleted afterwards.

## Guard runs (all green)

| Suite | Result |
|---|---|
| bsa `--lib ba2` | 44/44 |
| nif `--lib bs_geometry` | 72/72 |
| nif `--lib -- shader::tests::starfield` | 11/11 |
| nif `--lib starfield` | 32/32 |
| sfmaterial (lib + header_smoke + integration) | 38 + 6 + 1 (3 ignored) |
| **sfmaterial `real_cdb.rs --ignored`** (real data) | **3/3** |
| plugin `spatial_units` / `starfield` / `txst` / `dat2` / `xcll` / `lgtm` | 6 / 19 (1 ign) / 15 / 7 / 23 (2 ign) / 3 |
| byroredux `starfield_mat` / `starfield_single_channel` / `material_translate` | 17 / 1 / 77 |
| byroredux `light_anim` / `normalize_mesh_path` / `cdb` / `loose_mat` / `asset_provider::material` | 31 / 8 / 25 / 4 / 17 |

Real-CDB measurements, taken today:
- The base CDB keys **500,403** objects. The index build takes **1.6 s**, with a process high-water mark of **242 MB**. Both full-size indexes were built serially in one process.
- SFBGS007 keys **500,385**.
- The streaming validator reads **97 classes / 1,438,780 values**.
- #5320's stricter `MaterialIndex::build` (row/instance join, trailing bytes, duplicate index) accepts the real data.

## Executive Summary

Starfield is a first-class `GameKind`:
- **NIF + BA2 are clean**: 100% over 120,543 NIFs, 13/13 archives, 0 `NiUnknown` (measured today by `/audit-nif`).
- **CDB Phase 2 is partial**:
  - Translated: the colour, normal, emissive and height roles, plus the alpha, glass and flat-colour settings.
  - Still parked: authored metalness/roughness scalars (`MaterialParamFloat`) and the five single-channel kinds.
- **Walkable Cydonia** was not re-verified, because no engine launch is allowed. #3540 still has no real-device confirmation.

**Delta outcome: 0 CRITICAL, 1 HIGH, 0 MEDIUM, 2 LOW (all NEW).** All five 09-29 findings are fixed:
- D4-01 → #5151;
- D4-02 → #5001;
- D4-03 → #5002;
- D3-01 → #5003;
- META-01 → #5004 (the skill's Dim 5 first step now runs 11 tests).

#4268 and #4441, open-cited at 09-29, are closed and verified.

**Headline (HIGH, SF-D6-01).** Phase 2 routes CDB texture slot 6 (`*_height.dds`) into the canonical `height` role, and the renderer reads that role as a **parallax-occlusion** height map. Starfield's schema authors POM only on `ProjectedDecalSettings`, which has its own toggle, scale and nested height texture. On ordinary materials, slot 6 feeds layer height-blending (`AlphaBlenderSettings.HeightBlend*`) and terrain displacement. Every Starfield material with a slot-6 map therefore gets an unauthored POM ray-march at the engine default scale. It is the same kind of near-miss misroute that nifal.md's parked table forbids for `_rough` → `smooth_spec`.

## Dimension Summaries

| Dim | Verdict | Notes |
|---|---|---|
| 1 BA2 + corpus | clean | #5008 guards present (`dx10_open_rejects_a_non_24_chunk_hdr_len_as_invalid_data`, `dx10_open_tolerates_non_monotonic_start_mip_without_panicking`). #5232/#5260 harness hardening. Corpus 100% / 120,543, and the Starfield block total of 770,322 matches the TSV (`/audit-nif` today). |
| 2 BSGeometry | clean | #4268 verified (`skin.rs:331-350`, declines and never clamps). The `as_chunks` rewrite (#5121) behaves the same. The skeleton solver is unchanged. |
| 3 CDB | 2 LOW | Phase 2 reviewed end to end. The index join, cache race, failure memoization and per-slot replacement are correct, and the real CDB builds. Doc/fragility only (D3-01, D3-02). |
| 4 ESM + bring-up | clean | All 09-29 findings are fixed in `spatial_units.rs` / `weather.rs` / `world.rs`. A unit sweep over every distance field found nothing unlifted that has a consumer. #5309's walker dedup lost no arm. |
| 5 NIF shader blocks | unchanged | Comment-only commit `2be7c7c0b`; the CRC32 table is unchanged. |
| 6 Material flow | 1 HIGH | SF-D6-01. The provenance split (#4283 / #4855) and `pack_imported_material_flags` are intact. |

Per-dimension detail: `/tmp/audit/starfield/dim_{1..6}.md`.

---

## Findings

### HIGH

### SF-2026-10-08-D6-01: CDB slot 6 (`_height`) lands in the canonical `height` role, which the renderer consumes as a parallax-occlusion map, so Starfield materials get an unauthored POM ray-march at the engine default scale
- **Severity**: HIGH (the `_audit-severity.md` NIFAL row: a wrong Material out of the NIFAL boundary)
- **Dimension**: Material Flow (NIFAL boundary) / CDB Material Database
- **Location**: `byroredux/src/asset_provider/material/merge.rs:265-271` (`SLOT_HEIGHT => fill(&mut material.textures.height, …)` in `apply_cdb_material`); consumer `byroredux/src/render/static_meshes.rs:657` (`parallax_map_index = texture_indices.height`); `crates/renderer/shaders/triangle.frag:292-305`; defaults `crates/core/src/ecs/components/material.rs:17-21`
- **Status**: NEW. It arrived with `224a19372` / `18fce7e43` (#3398 Phase 2). A search of open and closed issues and of the 10-03 to 10-08 reports found no mention.
- **Description**: `apply_cdb_material` fills the canonical `height` role from `MRTextureFile` slot 6. The render path binds `height` to `parallaxMapIndex`, and `triangle.frag` runs parallax-occlusion displacement whenever that index is non-zero. The CDB arm authors no parallax scale or step count, so POM runs at `DEFAULT_PARALLAX_HEIGHT_SCALE = 0.04` and `DEFAULT_PARALLAX_MAX_PASSES = 4`.
  - Starfield does not use slot 6 as a POM input. In the vanilla CDB's 97-class schema, the only parallax-occlusion fields are on `BSMaterial::ProjectedDecalSettings`: `UseParallaxOcclusionMapping`, `SurfaceHeightMap: BSMaterial::TextureFile` (a nested field, not an `MRTextureFile` slot), `ParallaxOcclusionScale`, `ParallaxOcclusionShadows` and `MaxParralaxOcclusionSteps`. That is, decals only.
  - Height is consumed elsewhere by `BSMaterial::AlphaBlenderSettings` (`HeightBlendThreshold`, `HeightBlendFactor`; layer height-blending) and `BSMaterial::TerrainSettingsComponent` (`MaxDisplacement`, `DisplacementMidpoint`).
  - Ordinary layered materials have no POM toggle or scale.
- **Evidence**:
  - A read-only CLAS dump of `Starfield - Materials.ba2:materials\materialsbeta.cdb` (field lists quoted above).
  - The base CDB carries 1,521 `*_height.dds` FileName strings (394 unique), against 24,827 unique `*_color.dds`.
  - Sampled height maps are layer-blend landscape and architecture sets: `SnowScalloped01_height`, `DirtForerstRoots01_height`, `CaveRoughMacro01_height`, `NAStone01Mossy01_height`, `NATechPatternConcrete02_height`.
  - The synthetic CDB fixture carries slots 0/1/3 only (`index.rs` `synthetic_cdb_chunks`), so no test exercises the slot-6 route.
- **Impact**:
  - POM displaces `sampleUV` before every later fetch (base, normal, detail, emissive…). Each affected Starfield surface swims and distorts with view angle, from a height field authored for blending layers, not for displacing UVs.
  - This hits the landscape, cave and New Atlantis architecture materials that carry height maps.
  - It is a fabricated shading effect at the single translation boundary, the same near-miss class nifal.md forbids for the five parked kinds ("the near-miss roles are wrong, not merely imperfect").
- **Related**: #3398 (Phase 2), #4429 (the parked-kinds contract), #5283 (sibling: the CDB emissive role is zero-weighted), `docs/engine/nifal.md` § Starfield single-channel kinds.
- **Suggested Fix**:
  - Stop forwarding `SLOT_HEIGHT` into `height`. Park it with the single-channel kinds until a Starfield layer height-blend consumer exists.
  - If decal POM is wanted, translate it from `ProjectedDecalSettings` (toggle + scale + steps + `SurfaceHeightMap`).
  - Add a slot-6 fixture that pins the decision, and fix `apply_cdb_material`'s "6=height" doc.

### LOW

### SF-2026-10-08-D3-01: `MaterialIndex` assigns `PersistentID` columns by `BTreeMap` alphabetical field order (Dir < Ext < File), while its docs call the read "positional"
- **Severity**: LOW
- **Dimension**: CDB Material Database
- **Location**: `crates/sfmaterial/src/index.rs:582-604` (the `Objects` stream closure); docs at `:13-14` and `:165-168`
- **Status**: NEW
- **Description**: The closure collects the `PersistentID` struct's three `U32` columns from `pid.fields.values()`. `ObjectInstance.fields` is a `BTreeMap<String, Value>` (`value.rs:55`), so iteration order is alphabetical: `Dir`, `Ext`, `File`. The destructure `[stem, dir, _ext]` therefore reads `Dir` → stem and `Ext` → dir, and binds the `"mat"` constant column `File` as `_ext`.
  - That is correct, but only because the three field names happen to sort that way.
  - On disk the positional order is `Dir`(stem)@0, `File`("mat")@4, `Ext`(dir)@8, which the fixture spells out at `index.rs:931-936`. A truly positional read with the same destructure would key every object on the `"mat"` constant.
  - The module doc ("the columns are positional") and `material_key`'s doc ("this is positional") describe the opposite of what the code relies on.
- **Evidence**: As above. Real-data lookups pass today (`material_index_resolves_a_vanilla_material`, run green this audit).
- **Impact**: None today. A refactor that follows the docs, or switches `fields` to an insertion-ordered map, breaks every CDB lookup. The synthetic fixture would catch it, so this is fragility plus a doc contradiction, not a live bug.
- **Related**: #3398; the `.Dir` = stem / `.Ext` = dir label rotation recorded in the 08-29 spike.
- **Suggested Fix**: Read the columns by name (`fields.get("Dir")` → stem, `fields.get("Ext")` → dir) and correct both comments.

### SF-2026-10-08-D3-02: Phase-1 and "no `.mat` resolver" text survives Phase 2 and #4277, including an INFO log printed every session, and `apply_loose_mat` was inserted under the `single_boundary_tests` doc comment
- **Severity**: LOW
- **Dimension**: CDB Material Database (doc rot)
- **Location**:
  - `byroredux/src/asset_provider/material/cdb.rs:7-8` (module doc: "until the full parse lands");
  - `cdb.rs:277-281` (`probe_starfield_cdb` INFO log: "Phase 1 — full parse + per-field extraction is the deferred Phase 2 follow-up");
  - `cdb.rs:346-350` ("there is no resolver for a `.mat` path to miss");
  - `merge.rs:436-442` ("no JSON `.mat` resolver exists yet");
  - `merge.rs:574-577` ("The .mat format is not yet parsed (tracked in #4277)", which is closed);
  - `merge.rs:1664-1683` (the #2412/#3857 doc block for `mod single_boundary_tests` now precedes `fn apply_loose_mat`, so rustdoc attaches it there and the test module loses its header).
- **Status**: NEW. #5210 covers only the separate `merge_external_material` "forwards no authored field … Phase 2 should return `Merged`" comment and nifal.md. These sites were written stale by `224a19372` / `c2f28e06c`.
- **Description / Evidence**: As listed. The INFO line is runtime output. It is emitted once per discovered CDB on every session, and it tells an operator that per-field extraction does not exist while `MaterialIndex` is translating roles.
- **Impact**: Misleading logs and docs. No runtime effect.
- **Related**: #5210, NIFAL-D8-2026-10-08-01 (the loose `.mat` module doc's false "zero files" claim, a separate site), #3398, #4277.
- **Suggested Fix**: Reword the log to "index built lazily on first lookup". Drop the "no resolver" and "not yet parsed" claims. Move `apply_loose_mat` above the `#2412 / #3857` doc block so the block sits directly on `mod single_boundary_tests`.

---

## Already-tracked items re-checked (not counted as NEW)

- **CDB / material**:
  - #3398 (Phase 2 remainder: scalars, the five kinds, `UseSSS`).
  - #5291 (lazy main-thread index build and resident indexes; real HWM 242 MB measured today, release).
  - #5284 (CDB tail copied at 4 sites; the BGSM/BGEM copies skip trace and provenance).
  - #5277 (`IsGlass` gated out by missing blend state).
  - #5283 (CDB emissive zero-weighted).
  - #5210 (Phase-2 spec rot).
  - Cross-referenced from this suite: NIFAL-D8-2026-10-08-01 (loose `.mat` invented JSON layout; 20/20 real files → `None`), NIFAL-D8-2026-10-08-02 (loose arm skips `external_material_resolved`), PAR-D3-2026-10-08-02 (loose `.mat` slot `as u8` wrap / alpha from blue).
- **ESM / exterior**:
  - #5301 (WTHR fog lifted with no FNAM).
  - #5346 (two hand-copied SF 108-byte decoders and lifts).
  - #5171 (no test pins the LGTM lift).
  - #5170 (WATR noise-UV tiles unlifted).
  - #5343 (FO76 pigment scale).
  - #5364 (WTHS decode).
  - #1576 (model-less BFCB forms).
  - EXT-D1-2026-10-08-03 (#5363's stand-in skips the XCCM path).
  - ESM D4-01 (Story Manager behaviour, incl. Starfield).
- **Other**:
  - PEX-D4-01 (the `Conditional` ScriptName flag rejects 75 Starfield scripts).
  - #4470 / M48.8 (Starfield HUD; `/audit-ui`).
- **Verified fixed this window**:
  - #4268 (`.mesh` bone indices bounded; also confirmed by `/audit-safety`).
  - #4441.
  - #5001, #5002, #5003, #5004, #5134, #5151, #5299.
  - #5319, #5320, #5323.
  - #5190, #5196, #5197.

## CRC32 Flag Table

The table is unchanged: `crates/nif/src/shader_flags.rs` has had no commits since 2026-09-16. The full 32-row derivation is in `AUDIT_STARFIELD_2026-09-11.md` / `-09-16.md`. The #4279 `Own_Emit` dual read (typed bit OR CRC `EMIT_ENABLED`) is still in place. No vanilla block sets it.

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md`, Phases 0+1 are done, and Phases 2–4 were invalidated by the parity measurement. The BGSM parser and the ESM parser have both shipped, and the NIF truncation tail is cleared.

0. **The units boundary is closed** for every consumed distance field (WTHR, WATR, LGTM, XRGD shipped). What remains is test and dedup: #5171, #5346, #5301, and the noise-UV tile decision (#5170, which needs a capture).
1. **CDB Phase 2 remainder** (#3398):
   - Un-route slot 6 from POM (**SF-D6-01**).
   - Give the five parked kinds canonical roles (#4429 contract).
   - Map `MaterialParamFloat` → roughness/metalness.
   - Find a subsurface-colour source for `UseSSS`.
   - Forward blend state so `IsGlass` reaches the classifier (#5277).
   - Weight emissive (#5283).
   - Rebuild the loose `.mat` decoder against the real `Objects[].Components[].Data` dialect (NIFAL-D8-01).
   - Move the index build off the main thread (#5291).
2. **PDCL ahead of GBFM**: PDCL is 74.9% of unresolved Cydonia REFRs, against GBFM's 0.081%. Unchanged.
3. **Exterior worldspace tiles**: still a named policy skip in `m-exteriors.sh`. WTHS decode (#5364) replaces the `DefaultWeather` stand-in.
4. **Space-cell / planet / GBFM records**: PNDT / STDT / BIOM, and SFTR, which walks but is unrouted.

## Coverage Notes

- **Not run**:
  - `--sf-smoke` and the engine binary (forbidden). Cydonia's resolve rate is not re-measured.
  - The `sf_smoke` example. `/audit-esm` measured Starfield top-level routing at 96.17% today.
  - `parse_rate_starfield_all_meshes`. `/audit-nif` measured 100% / 120,543 today.
- **Real-data probes**:
  - `real_cdb.rs` 3/3 (bounded, ~4 s).
  - A CLAS schema dump and FileName string census of the vanilla base CDB (Python, `-I`, read-only; the extracted blob was deleted).
- **Scratch**: `/tmp/audit/starfield/dim_{1..6}.md`, `guards_crates.txt`, `guards_bin.txt`, `real_cdb.txt`. The three findings match the scratch files one-for-one: D6-01 ← dim_6, D3-01/D3-02 ← dim_3. Dims 1, 2, 4 and 5 are clean.

## Total Findings Summary

| Severity | NEW |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 0 |
| LOW | 2 |
| **Total** | **3** |

| ID | Sev | Title | Status |
|---|---|---|---|
| SF-2026-10-08-D6-01 | HIGH | CDB slot 6 `_height` → canonical `height` → unauthored POM on Starfield materials | NEW |
| SF-2026-10-08-D3-01 | LOW | `PersistentID` columns read by `BTreeMap` alphabetical order; docs say positional | NEW |
| SF-2026-10-08-D3-02 | LOW | Phase-1 / "no `.mat` resolver" text and per-session INFO log survive Phase 2 + #4277; misplaced doc block | NEW |

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-10-08.md`. Label every finding `game:starfield` + `legacy-compat`, and add:
- D6-01: `high` `bug` `nifal` `import-pipeline`;
- D3-01: `low` `tech-debt` `import-pipeline`;
- D3-02: `low` `documentation` `doc-rot`.
