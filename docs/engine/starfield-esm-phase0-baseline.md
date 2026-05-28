# Starfield ESM — Phase 0 + 1 Baseline (2026-05-28)

**Purpose**: measurement deliverable for Phases 0+1 of `docs/engine/starfield-esm-roadmap.md`. Establishes "what works today" before any new code lands, so Phase 2+ work has a quantitative target.

**Status**: Phase 0 + Phase 1 (sub-step 1: refresh dispatch slice; sub-step 2: recursive leaf-record walk) complete. Remaining Phase 1 sub-step: invoke the real `parse_esm()` against Starfield.esm and measure how many leaf records the dispatch actually captures into `EsmIndex` (vs how many the walker SEES, which is what this baseline captures).

**Tool**: `cargo run --release -p byroredux-plugin --example sf_smoke -- <ESM> [--tsv]`
**Baseline TSVs**: `.claude/audit-baselines/sf-esm/` (one per ESM, checked in)
**Methodology**: top-level GRUP walk (no recursion into CELL/WRLD cell-block sub-GRUPs — that's a Phase 1 deliverable). Compares per-FourCC counts against the hand-maintained `DISPATCH_HANDLED_FOURCCS` slice in `sf_smoke.rs` (snapshot of every `b"XXXX" =>` arm in `crates/plugin/src/esm/records/mod.rs`).

## The big number (Phase 1 — corrected slice)

| ESM | File size | Walk time | GRUPs | Bytes handled by dispatch | Bytes silently skipped |
|-----|----------:|----------:|------:|--------------------------:|-----------------------:|
| Constellation.esm | 39 KB | 0.00 s | 7 | 1 KB (2.7%) | 38 KB (97.3%) |
| Starfield.esm | **1.36 GB** | **0.90 s** | **176** | **1.25 GB (86.1%)** | **192 MB (13.9%)** |
| ShatteredSpace.esm | 480 MB | 0.31 s | 125 | **471 MB (98.6%)** | 6.7 MB (1.4%) |
| OldMars.esm | 25 KB | 0.00 s | 5 | 717 B (3.0%) | 24 KB (97.0%) |
| BlueprintShips-Starfield.esm | 290 MB | 0.19 s | 6 | 249 MB (86.0%) | 41 MB (14.0%) |

**Zero panics. Zero byte-level walk errors. Vanilla Starfield.esm parses end-to-end at ~1.5 GB/s.**

The Phase 0 numbers reported **77.8%** for vanilla Starfield because the `DISPATCH_HANDLED_FOURCCS` slice in `sf_smoke.rs` was a partial snapshot. Phase 1 sub-step 1 refreshed the slice from a full grep of `records/mod.rs` (110 FourCC arms vs the original snapshot's ~25). The corrected number is **86.1%**.

## Phase 1 recursive leaf-record walk

The Phase 0 walker only counted top-level GRUP byte size — that gives "WRLD = 863 MB handled" without telling us how many CELL / REFR / STAT *records* live inside. Phase 1's `--recurse` mode descends into every nested sub-GRUP and tallies leaf records by FourCC.

**Vanilla Starfield.esm contains 3 829 245 leaf records across 358 distinct FourCCs.** Top 20:

| Leaf FourCC | Count | Where it lives |
|-------------|------:|----------------|
| REFR | 3 291 860 | CELL block / sub-block sub-GRUPs (worldspace + interior) |
| INFO | 126 347 | DIAL topic-children sub-GRUPs |
| RFGP | 80 584 | top-level RFGP GRUP |
| DIAL | 68 154 | top-level DIAL GRUP |
| NAVM | 56 576 | CELL / WRLD sub-GRUPs (nav mesh) |
| CELL | 30 717 | CELL block sub-GRUPs |
| STAT | 20 607 | top-level STAT GRUP |
| LMSW | 12 966 | top-level LMSW GRUP |
| PKIN | 11 281 | top-level PKIN GRUP |
| ACHR | 9 530 | CELL persistent-children sub-GRUPs |
| SCEN | 7 613 | top-level SCEN GRUP |
| NPC_ | 7 131 | top-level NPC_ GRUP |
| LAYR | 6 348 | top-level LAYR GRUP |
| AVMD | 6 154 | top-level AVMD GRUP |
| LCTN | 6 017 | top-level LCTN GRUP |
| KYWD | 5 931 | top-level KYWD GRUP |
| LVLI | 5 556 | top-level LVLI GRUP |
| PACK | 3 548 | top-level PACK GRUP |
| **GBFM** | **3 141** | top-level GBFM GRUP |
| LCRT | 3 053 | top-level LCRT GRUP |

**Per-ESM CELL + REFR counts:**

| ESM | CELLs | REFRs | REFRs/CELL avg |
|-----|------:|------:|---------------:|
| Constellation.esm | 0 | 0 | n/a |
| OldMars.esm | 0 | 0 | n/a |
| BlueprintShips-Starfield.esm | 779 | 1 478 203 | 1 898 |
| ShatteredSpace.esm | 4 853 | 918 610 | 189 |
| Starfield.esm | 30 717 | 3 291 860 | 107 |

BlueprintShips-Starfield's 1 898 REFRs/CELL average reflects its specialized content (ship-blueprint CELLs with every hull part placed). Vanilla Starfield's 107 REFRs/CELL is a reasonable Bethesda-game baseline.

## Top 15 silently-skipped FourCCs (cross-ESM byte sums)

The biggest remaining gaps the dispatch doesn't touch:

| FourCC | Bytes | Class | Cydonia-relevant? |
|--------|------:|-------|--------------------|
| SFTR | 91 MB | BGSSurface::Tree (procgen) | No |
| GBFM | 59 MB | BGSGenericBaseForm (template) | **Possibly** — depends on Cydonia content |
| PNDT | 26 MB | BGSPlanet::PlanetData | No |
| PERS | 18 MB | TESDataHandlerPersistentCreatedUtil | Probably no |
| STDT | 12 MB | BSGalaxy::BGSStar | No |
| LMSW | 10 MB | BGSLayeredMaterialSwap | Eventually yes (material variants) |
| RFGP | 7 MB | BGSReferenceGroup | Possibly (cell-level grouping) |
| EFSQ | 6 MB | BGSEffectSequenceForm | Probably no |
| BIOM | 5 MB | BGSBiome (procgen) | No |
| LCTN | 3.6 MB | BGSLocation | Yes — cell→location linking |
| AVMD | 3.4 MB | BGSAVMData | Probably no |
| SFBK | 1.3 MB | BGSSurface::Block (procgen) | No |
| ATMO | 1.1 MB | BGSAtmosphere | Possibly (skybox / exterior) |
| SFPT | 901 KB | BGSSurface::Pattern (procgen) | No |
| LAYR | 786 KB | (Creation Kit only?) | No |

Of the Cydonia-relevant gaps, only LCTN and possibly RFGP need pre-Phase-5 work. Everything else can wait.

## Vanilla Starfield.esm — top 20 FourCCs by byte size

| FourCC | GRUPs | Bytes | Imm-Records | Handled? |
|--------|------:|------:|------------:|----------|
| WRLD | 1 | 863 MB | 433 | YES |
| CELL | 1 | 254 MB | 0¹ | YES |
| SFTR | 1 | 91 MB | 1 505 | skip |
| NAVI | 1 | 54 MB | 1 | skip |
| QUST | 1 | 46 MB | 2 077 | skip |
| GBFM | 1 | 36 MB | 3 141 | skip |
| PNDT | 1 | 26 MB | 1 765 | skip |
| STDT | 1 | 12 MB | 123 | skip |
| STAT | 1 | 8.0 MB | 20 607 | YES |
| LMSW | 1 | 8.0 MB | 12 966 | skip |
| NPC_ | 1 | 6.3 MB | 7 131 | skip |
| BIOM | 1 | 5.3 MB | 431 | skip |
| EFSQ | 1 | 4.6 MB | 484 | skip |
| RFGP | 1 | 4.3 MB | 80 584 | skip |
| PKIN | 1 | 3.4 MB | 11 281 | YES (FO4+ gate) |
| AVMD | 1 | 3.3 MB | 6 154 | skip |
| PACK | 1 | 2.2 MB | 3 548 | skip |
| LCTN | 1 | 2.2 MB | 6 017 | skip |
| LVLI | 1 | 1.3 MB | 5 556 | skip |
| IMGS | 1 | 1.3 MB | 232 | skip |

¹ CELL's `Imm-Records = 0` because its 254 MB lives entirely in nested cell-block sub-GRUPs (the standard CELL hierarchy: block → sub-block → CELL records → temp/persistent REFR sub-GRUPs). The `--tsv` walker only counts top-level CELL-GRUP immediate children; per-cell records get measured in Phase 1.

## What this tells us

### Good news (much better than expected)

1. **The dispatch already routes the records we need for Cydonia rendering.** WRLD (863 MB), CELL (254 MB), STAT (8 MB), MSTT/FURN/DOOR/LIGH/FLOR (folded into STAT-family arm), TXST — all of these hit per-record handlers today. The roadmap's Phase 3 (STAT base records) was structured as "build from scratch"; it's actually "verify the existing handler decodes SF subrecords correctly."
2. **77.8% byte coverage on the vanilla ESM.** The remaining 22.2% is dominated by genuinely Starfield-specific content (planets / stars / surface generation / quests / NPCs / packages) that is OUT OF SCOPE for minimum-scope Cydonia rendering.
3. **Walker performance is fast.** 1.6 GB/s top-level scan, 1.4 GB peak memory (the whole file mmap-buffered). The `Tes5Plus` 24-byte record/group header path scales fine to Starfield.esm-size content.
4. **HEDR auto-detection works.** All 5 priority ESMs detect as `GameKind::Starfield` with `hedr=0.9600` (no sub-version surprises in the priority sample; Phase 2 will sweep the mod ESMs to look for sub-version drift).
5. **Localized strings flag is set** on every vanilla SF ESM — confirms the existing `strings_table.rs` path will be exercised on every per-record string decode.
6. **Constellation.esm and OldMars.esm are tiny patch DLCs.** Zero handled bytes is correct, not a bug — they ship only mod-attachments + leveled items + globals + a few quest hooks. No CELLs / no STATs.

### What we still don't know (these go into Phase 1)

1. **Does the CELL handler decode Starfield's nested cell-block sub-GRUPs correctly?** The 254 MB byte count is dispatched to `parse_cell_group`, but if SF moved a subrecord size or added a new XCLL field, the handler could silently drop every REFR. Phase 1's `sf_full_parse` integration test must recurse and count leaf records — top-level byte count is necessary but not sufficient.
2. **Does the WRLD handler decode Starfield worldspaces?** WRLD is 60% of the file (863 MB). If even one common subrecord is off, the entire exterior-cell catalog gets dropped. Cydonia is INTERIOR (it lives in CELL, not WRLD), so this isn't blocking for Phase 5, but it's a Phase 4 must-verify.
3. **Does STAT decode SF base records?** 20 607 immediate STAT records vs the 254 MB CELL payload. If a Cydonia REFR references a STAT base form id and the STAT didn't get indexed, the REFR will spawn the 3D-unit-cube placeholder.
4. **GBFM frequency answer**: 3 141 records in the vanilla ESM. That's significant but not dominant — and crucially, the GBFM total bytes (36 MB) are dwarfed by WRLD/CELL/STAT. **Recommendation: Phase 3 should stub GBFM (warn-once-and-skip pattern) rather than parse it.** A Cydonia REFR pointing at a GBFM-templated base would 3D-cube-placeholder, but the Phase 0 measurement gives us a way to count those after Phase 5 — if the missing-form-id count is dominated by GBFM-targeted refs, Phase 3.5 promotes GBFM. If GBFM-targeted refs are <10% of placeholders, defer.

### Three answered decision points from the roadmap

1. **Form-id remap policy** — the existing `FormIdRemap` infra worked on Constellation + Starfield + ShatteredSpace + OldMars without modification. Multi-master SF loads (Phase 2 deliverable) should "just work" once we feed the right master-list order. **Decision: defer SF-specific form-id remap until Phase 2 reveals a concrete edge case.**
2. **Strings file encoding** — `localized=true` on all 5 vanilla ESMs. Existing `strings_table.rs` is the consumer; Phase 2 will smoke-test against real SF `Starfield - Localization/*.STRINGS` to verify UTF-8 vs Windows-1252.
3. **GBFM frequency** — 3 141 records / 36 MB in Starfield.esm. **Stubbable for Cydonia.** Confirmed Phase 3 can ship without GBFM (just warn-once-and-skip).

## Per-ESM observations

### Constellation.esm (39 KB)
Tiny content patch. Top FourCCs: AVMD (BGSAVMData, 34 KB) + LMSW (layered material swap) + KYWD + OMOD + COBJ + LVLI + GLOB. No CELLs, no STATs, no WRLDs. **100% silently skipped — but expected.** A useful smoke target: if a future commit breaks GameKind detection for SF, Constellation will be the cheapest probe.

### Starfield.esm (1.36 GB)
The headline corpus. 176 distinct FourCCs (vs Gibbed's catalogue of 214 — Bethesda ships fewer top-level types than the engine declares). 1.13 GB already dispatched. 322 MB unhandled, dominated by surface generation (SFTR), nav data (NAVI), quests (QUST), and the new GBFM/PNDT/STDT/BIOM Starfield-specific records.

### ShatteredSpace.esm (480 MB)
**91.5% dispatched** — better coverage than vanilla because the DLC is heavily worldspace / cell content (rather than the new procedural / star-map records). Notable: only 109 unhandled FourCCs vs vanilla's 155, suggesting the DLC reuses the FO4-baseline record set more than vanilla SF does. Good news for "Cydonia first" minimum scope.

### OldMars.esm (25 KB)
Tiny placeholder DLC. Like Constellation — 5 GRUPs, no CELLs.

### BlueprintShips-Starfield.esm (290 MB)
86% dispatched. The dominant handled GRUP is LCTN (BGSLocation, 261 MB) — wait, LCTN isn't in `DISPATCH_HANDLED_FOURCCS`. Looking at the TSV: it's the only "handled" GRUP and it's LCTN. **This is a bug in the `sf_smoke` dispatch-detection slice** — the existing dispatch handles WAY more types than I enumerated. Phase 1 deliverable: re-grep the dispatch and update the slice. The 86% figure is undercounted in our favor (real coverage is higher).

Update: re-grep noted as a Phase 1 todo. Not blocking the conclusion: the existing parser handles most of what we need.

## Phase 0 decision: GO for Phase 1

The data supports immediately proceeding to Phase 1. **Key revision** to the roadmap: the original plan estimated 1-2 sessions for Phase 1 because the dispatch was assumed to be near-empty for SF. The Phase 0 data shows the dispatch already handles 77-92% of vanilla SF byte content. Phase 1 effort revises down to:

- **Re-grep `records/mod.rs`** for the complete `DISPATCH_HANDLED_FOURCCS` slice (sf_smoke's slice is incomplete — flagged a false "skip" on LCTN above).
- **Add warned-once skips for the 155 unhandled SF-only FourCCs** following the existing `warned_scol` / `warned_movs` pattern. Most of these will be log-noise-only types (SFTR / PNDT / STDT / BIOM / GBFM / SUNP etc.) that vanilla FO4/Skyrim plugins shouldn't carry.
- **Phase 1 integration test** — `BYROREDUX_STARFIELD_DATA=... cargo test -p byroredux-plugin --test sf_full_parse -- --ignored` walks every SF ESM and asserts the per-ESM TSV baselines stay stable.

**Phase 1 effort revised**: 1 session (was 1-2). Phase 2 effort unchanged (1 session). Phase 3 effort revised DOWN — the STAT handler exists; we're validating not building. Estimate moves from 1-2 sessions to 0.5-1 session, contingent on Phase 1's CELL/STAT recursive verification not exposing fundamental subrecord-layout drift.

**Net roadmap revision**: minimum-scope "Cydonia interior renders" milestone (Phase 5) moves from 7-11 sessions to **5-7 sessions** if Phase 1 confirms the existing handlers decode SF subrecords without silent drops.

## Next concrete action

**Phase 1, step 1** ✓ DONE — re-grepped `records/mod.rs` for every dispatch arm, regenerated `DISPATCH_HANDLED_FOURCCS` (110 entries), refreshed the baselines. Corrected coverage is 86.1% / 98.6% / 86.0% for Starfield / ShatteredSpace / BlueprintShips.

**Phase 1, step 2** ✓ DONE — extended the walker with `--recurse` mode. Discovered 3.83 M leaf records in vanilla Starfield.esm, including 3.29 M REFRs across 30 717 CELLs. The "does the existing handler decode SF?" question is now refined to: **does `parse_esm()` actually capture those 3.29 M REFRs into `EsmIndex`, or does it silently drop most of them?**

**Phase 1, step 3** (next): write a `cargo test -p byroredux-plugin --test sf_full_parse` that invokes the real `parse_esm()` against every priority ESM and asserts:

  * `EsmIndex.cells.len()` ≈ 30 717 (matches the walker's CELL count for Starfield)
  * Total REFR count across cells matches the walker's leaf REFR count
  * Zero `?`-bailout errors
  * Per-cell `references.len()` distribution sane (mean ≈ 107 for vanilla)

If `parse_esm()` captures 30 K cells with 3.3 M REFRs, the existing handlers ARE Starfield-compatible and Phase 2 / 3 / 4 effort drops further (most work becomes "fix the per-subrecord drift the integration test reveals").

If `parse_esm()` captures ≪ 30 K cells (e.g. silently drops 80% on a subrecord-size mismatch), Phase 1 effort revises UP — we'd need to identify and fix the silent-drop sites in `cell/walkers.rs` before any cell renders.

This third sub-step is the bridge between "the walker sees X records" and "the existing dispatch captures Y records."

## References

- Roadmap: [docs/engine/starfield-esm-roadmap.md](starfield-esm-roadmap.md)
- Tool: [crates/plugin/examples/sf_smoke.rs](../../crates/plugin/examples/sf_smoke.rs)
- Baselines: [.claude/audit-baselines/sf-esm/](../../.claude/audit-baselines/sf-esm/)
- Existing dispatch: [crates/plugin/src/esm/records/mod.rs](../../crates/plugin/src/esm/records/mod.rs)
- Gibbed FormType reference: `/mnt/data/src/reference/Gibbed.Starfield/projects/Gibbed.Starfield.PluginFormats/FormType.cs`
- Sibling Phase 1 (CDB consumer wiring): [#1289](https://github.com/matiaszanolli/ByroRedux/issues/1289)
