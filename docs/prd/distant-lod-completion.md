# Distant LOD Completion — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (GameKind count, generated depth define, census figures framed as
Phase 0, tree-texture source, CK tree-LOD citation, FO3/FNV tree split,
virtual-geometry and far-field-shadow non-goals). Revision 1 was the first
draft: the remainder of ROADMAP milestone M35 (Terrain LOD), re-scoped
against the code at HEAD, which has already shipped three of the five items
the M35 row lists as open.
Statements about how an original engine, file format or tool behaves cite
external sources (the local UESP / CS / CK / GECK wiki dumps, nif.xml, the
games' shipped INI files); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: Distant LOD on the five titles whose exteriors the engine loads
(Oblivion, Fallout 3, Fallout: New Vegas, Skyrim, Fallout 4): every distant
terrain, object and tree LOD family each title authors is drawn, measured by
the live `lod.coverage` gate; the join between the resident full-detail cells
and the LOD ring leaves no gap; the band distances and quad lattices come from
the games' own settings and descriptors; and the reversed-Z depth mapping that
is implemented but disabled is validated and switched on. Out of scope: LOD
generation (no baker), distant water (WATAL), near-field terrain correctness,
FO4 precombines (M49), near-field SpeedTree geometry, virtual geometry (M53),
and Fallout 76 / Starfield distant LOD.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design section.

| Mechanism | Owning design |
|---|---|
| LOD families, providers, ring reconciliation, VWD rule, per-cell segments | [EXAL §5](../engine/exal.md#5-lod-distance-rendering-the-must-have) |
| Terrain-LOD join (albedo gain, boundary block, skirts, softening) | [EXAL §5.4.2](../engine/exal.md#542-the-terrain-lod-join-2026-10-2627) |
| Inherited LOD key and the `lodsettings` descriptor | [EXAL §5.4.1](../engine/exal.md#541-the-inherited-lod-key-5496-and-the-authored-descriptor) |
| Band ladder, `lod.coverage`, `terrain.seams`, VWD and reversed-Z history | [Exterior readiness plan, EX-10/11](../engine/exterior-readiness-plan.md#ex-1011-2371--near-terrain-correctness-and-distant-lod-bands) |
| Streaming ring scheduling and LOD work budget | [Exterior Grid Streaming §4](../engine/exterior-grid-streaming.md#4-streaming-phase-1-async-pre-parse-worker) |
| Geometry-pool admission tiers for LOD uploads | [Memory Budget, Mesh Registry](../engine/memory-budget.md#mesh-registry) |
| Tree tiers above the distant ring | [EXAL Trees §7](../engine/exal-trees.md#7-the-lod-chain) |
| Depth mapping (reversed-Z) | `ACTIVE_DEPTH_MAPPING` rustdoc in `crates/core/src/ecs/components/camera.rs`; no design document — Phase 0 adds a section to the exterior readiness plan |
| Oblivion and FO3 distant-tree families | None yet — Phase 0 adds them to [EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `DLOD-` prefix.

---

## 1. Product thesis

EXAL names distant LOD its must-have: an exterior without a horizon is a
render demo, not a world
([EXAL §5](../engine/exal.md#5-lod-distance-rendering-the-must-have)). M35
built most of it. Terrain and object LOD draw on all five exterior titles, a
per-game band ladder reads the games' own INI distances, Skyrim's tree LOD
draws, and a live gate proves that no LOD quad overlaps a resident cell.

What is left falls into three groups:

1. **Families that are authored but not drawn.** Most of the trees in
   Oblivion's `.lod` placement lists, and Fallout 3's `.dtl` tree files, are
   never read.
2. **Seams the gate cannot see.** The gate sees terrain and object quads
   only, so the tree and placement rings are invisible to it. It counts
   overlaps but not gaps, so the moat between the resident cells and the
   first whole LOD quad is never measured.
3. **Choices still waiting on evidence.** Quad lattices are inferred where
   the games ship a descriptor. Some distances are inferred where the INI
   names one. Reversed-Z is built but not enabled.

The audience is a player walking out of a town on any of the five titles,
and the developer who needs the gate to fail when the horizon breaks.

The corollary that shapes every requirement: **distant LOD is complete per
authored family, and a family counts as drawn only when the gate reports
it.** "Distant LOD works on FO3" means every FO3 family in §3 has a nonzero
resident count, zero overlaps and zero moat cells on a boundary run, and
nothing else. The engine will not generate LOD that a game did not author,
will not chase the original renderer's look pixel for pixel, and will not
draw LOD for titles whose exteriors it does not load.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **LOD family** | One authored kind of distant asset for one title: a terrain, object or tree format with its naming, container and reader. The unit of the completion promise (§3). |
| **LOD ring** | The set of LOD entities a family keeps resident around the player, reconciled by `streaming_helpers::reconcile_lod_rings`. Each family has its own ring. |
| **Quad** | One authored LOD file covering a square of cells, named by level and SW-corner cell (`<ws>.<level>.<x>.<y>.bto`, `<ws>.level<L>.x<x>.y<y>.nif`). |
| **Level** | Cells per quad edge: 4, 8, 16 or 32 (`LOD_LEVELS`, `lod_bands.rs`). |
| **Band ladder** | `LodBandLadder`: the per-game refine distances that pick a level by distance (`lod_bands.rs`). |
| **Resident full-detail cell** | A cell whose full REFRs and terrain are loaded now (`LodReconcileInput::resident_full_cells`). |
| **Overlap** | A LOD entity drawn over a resident full-detail cell. Counted by `lod.coverage` as `full_detail_overlaps` and `vwd_full_model_overlaps`. |
| **Moat cell** | A non-resident cell next to the resident set (within one finest-level quad edge of it) inside a family's ring, covered by no entity of that family. The gap that whole-quad exclusion leaves. New metric, defined by DLOD-GATE-005. |
| **Segment** | A `BSGeometrySegmentData` triangle range in a `BSSubIndexTriShape` / `BSSegmentedTriShape`; nif.xml describes these as "segmented in a grid (for LOD)" (`nifxml/nif.xml`, `BSGeometrySegmentData`). |
| **VWD** | Visible When Distant, Oblivion's name for it; the Creation Kit calls it "Has Distant LOD". It marks a base object as one to show "in LOD beyond the 5x5 loaded area" (CK wiki, *Static*). At HEAD it is materialised as the `VisibleWhenDistant` marker component. |
| **LOD descriptor** | The per-worldspace settings file under `lodsettings\`: `<ws>.lod` (Skyrim, FO4) or `<ws>.dlodsettings` (FO3, FNV). |
| **Depth mapping** | `DepthMapping::Conventional` or `DepthMapping::Reversed`, selected by `ACTIVE_DEPTH_MAPPING` (`camera.rs`). |
| **Far decade** | The 100 000–400 000 BU band of a `depth.stats` capture, where the LOD ring sits. |

---

## 3. Core model

Every LOD family on every title is in exactly one state:

| State | Meaning | Reported as |
|---|---|---|
| **Drawn** | Read, translated and resident on a boundary run, with zero overlaps and zero moat cells | nonzero `<family>_resident`, `verdict=PASS` |
| **Drawn, gapped** | Drawn, but whole-quad exclusion leaves moat cells at the resident boundary | nonzero moat count |
| **Unconsumed** | Shipped in the vanilla archives, never read | no resident count; listed in §3.1 |
| **Not authored** | The title ships no such family | the family predicate returns `false`, pinned by a test |

### 3.1 Family inventory

The state at HEAD on 2026-11-01. The code column was checked against the
source that day. The **Vanilla data** counts are not checked facts: they come
from a one-off name-table census of the vanilla archives (BSA folder and file
tables, BA2 name tables, and `.lod` group headers matched against
`Oblivion.esm` TREE and STAT records) run from a throwaway script that is not
in the repository. Every such count, here and wherever a requirement or a
decision below repeats one, is a **Phase 0: verify** figure until the census
is checked in as DLOD-GATE-007 and reproduces it. The exceptions are the
counts a code comment already records (the 386 `.btt`, `tree_lod.rs`). This column is
hand-written and goes stale: once DLOD-GATE-001 to -006 land, the boundary
run's `lod-coverage:` line replaces it.

| # | Family | Titles | Vanilla data | State | Today |
|---|---|---|---|---|---|
| F1 | Combined terrain `.btr` + per-quad `_n`/`_msn` | Skyrim, FO4 | `meshes\terrain\<ws>\*.btr` | Drawn | `terrain_lod_btr.rs`; clamp sampling fixed by #4912 (`235a90ba2`) |
| F2 | Legacy terrain DDS quads on LAND-synthesised geometry | Oblivion (32-cell), FO3/FNV (levels 4–32) | `textures\landscape\lod\…` | Drawn | `terrain_lod.rs` + `env_translate::translate_terrain_lod_textures`; the authored `Landscape\LOD\*.nif` meshes are not read (Q2) |
| F3 | Heightmap synth fallback | all five | LAND | Drawn | `terrain_lod.rs` |
| F4 | Combined objects `.bto` | Skyrim, FO4 | `meshes\terrain\<ws>\objects\` | Drawn, gapped | `object_lod.rs` (`ObjectLodScheme::BakedBto`); whole-quad exclusion against resident cells |
| F5 | Fallout `blocks\` combined quads | FO3, FNV | `meshes\landscape\lod\<ws>\blocks\` | Drawn, gapped | `ObjectLodScheme::FalloutLegacyBlocks` (#3321), selected from `LegacyLodQuadIndex` (#5222) |
| F6 | Oblivion `.lod` placements, STAT groups → `_far.nif` | Oblivion | 9 889 `.lod`; 118 STAT bases with a `_far.nif` (901 placements), 4 without (6) | Drawn | `placement_lod.rs`; full-model fallback for a base with no `_far.nif` (Q6) |
| F7 | Oblivion `.lod` placements, TREE groups | Oblivion | 82 TREE bases, 98 661 placements: the large majority of all `.lod` placements | **Unconsumed** | `spawn_placement_lod_cell` (`placement_lod.rs:528`) resolves a group's base through `record_index.cells.statics`, which TREE records also populate (`dispatch_world_placement.rs:64-67`). A TREE's `MODL` is a bare `.spt` name (`references/import.rs:274-278`), so `far_nif_path` returns `None`, the `meshes\`-prefixed full path resolves in no archive, and the group reaches `None => continue` (`placement_lod.rs:548`) |
| F8 | Skyrim tree billboards `.btt` + `<ws>.lst` + `treelod` atlas | Skyrim | 386 level-4 `.btt` (`tree_lod.rs` module docs) | Drawn, gapped; invisible to the gate | `tree_lod.rs` (#4913, `a9384f9c0`); whole-quad exclusion (`footprint_intersects_full_detail`, pinned by `full_detail_cells_veto_the_quads_they_touch`); not counted by `update_lod_coverage` |
| F9 | FO3 distant trees `.dtl` + `treetypes.lst` | FO3 | 75 `.dtl` + 4 `.lst` in `Fallout - Meshes.bsa` (71 at level 8, 4 at level 4), plus 33 `.dtl` + 7 `.lst` across Anchorage, BrokenSteel, PointLookout and ThePitt | **Unconsumed** | no reader: `.dtl` and `treetypes` appear nowhere in the code |
| F10 | `lodsettings\<ws>.lod` descriptor | Skyrim, FO4 | 16 B per worldspace | Drawn (origin only) | `resolve_lod_grid_origin` (#2586); extent and level fields unread |
| F11 | `lodsettings\<ws>.dlodsettings` descriptor | FO3, FNV | 15 (FO3) and 9 (FNV), 24 B each, in `Fallout - Misc.bsa` | **Unconsumed** | no reader; the legacy lattice is chosen by a residue-majority prune (#5387) and a coverage tie-break (#5520), which [EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod) calls "a data-anchored choice, not a verified reproduction" |
| — | Separate tree family | FNV, FO4 | none: FNV has no `lod\<ws>\trees\` folder in any archive; FO4 ships no `.btt` | Not authored | `tree_lod_supported` is `false` for both (`object_lod_scheme_table`) |

Cross-cutting state, the same date:

| Item | Today |
|---|---|
| VWD rule | Ring separation: object LOD only outside the resident set. `LodCoverageStats::vwd_full_model_overlaps` is gated at 0 in `m-exteriors.sh` boundary mode. No active cull (#3307, open) |
| LAND value-plausibility guards | Shipped: `sanitize_land_height`, `vnml_raw_magnitude`, `VNML_DEGENERATE_RAW_MAGNITUDE` (`cell_loader/terrain.rs`), with unit tests; exterior readiness EX-10/11 item 5 is checked |
| `.btr` sampling mode | Shipped: clamp per the authored `texture_clamp_mode` (#4912, closed 2026-10-01) |
| Depth mapping | Implemented behind `ACTIVE_DEPTH_MAPPING`, shipping `DepthMapping::Conventional` (`camera.rs`, pinned by `the_shipped_depth_mapping_is_conventional`). #3308 is closed by `c8ac5328d`; no open issue tracks the flip |
| Coverage gate | `lod-coverage:` reports `terrain_resident` and `object_resident` only (`LodCoverageStats::machine_line`, `crates/core/src/ecs/resources/mod.rs`) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **DLOD-G-001 — Every authored family drawn.** Every family that §3.1
  marks Unconsumed or Drawn, on each of the five titles, reaches Drawn.
  Family coverage (§8) is 100% on every title.
- **DLOD-G-002 — No moat.** Zero moat cells, per family, on every
  profile's boundary run, at every level where the asset gives a per-cell
  lever (§5.3).
- **DLOD-G-003 — The gate sees everything.** The `lod-coverage:` line
  carries every ring, and `m-exteriors.sh` fails when an authored family is
  absent.
- **DLOD-G-004 — Authored layouts over inferred ones.** Quad lattices and
  ring distances come from the games' descriptors and INI keys wherever the
  games ship them; each remaining inference is named in its rustdoc.
- **DLOD-G-005 — Reversed-Z on by default.** It ships as the default with a
  measured gain in the far decade and no gate regression.

### 4.2 Non-goals

- **DLOD-NG-001.** No LOD generation or baking, and no STAT `MNAM` parse.
  The shipped assets already encode the result
  ([EXAL §5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need)).
- **DLOD-NG-002.** Distant water. WATAL owns LOD water
  ([WATAL](../engine/watal.md)).
- **DLOD-NG-003.** Near-field terrain correctness. Seam cracks (#6500),
  Use-Land-Data child landscapes (#6594) and LAND parsing belong to the
  [exterior readiness plan](../engine/exterior-readiness-plan.md) and EXAL.
  The LAND plausibility guards M35 listed are shipped (§3.1).
- **DLOD-NG-004.** FO4 precombined and previs geometry: a near-field
  optimisation owned by M49, not distant LOD
  ([EXAL §2](../engine/exal.md#lod-distance-rendering--terrain--object-lod-live-on-every-legacy-and-creation-title-tree-lod-absent-no-canonical-model)).
- **DLOD-NG-005.** Near-field and mid-distance SpeedTree geometry
  ([EXAL Trees](../engine/exal-trees.md#7-the-lod-chain)). This PRD stops at
  the distant tier.
- **DLOD-NG-006.** Virtual geometry (M53). The
  [Virtual Geometry PRD](virtual-geometry.md) keeps authored distant LOD as
  the far-field authority and does not replace it (VGEO-NG-001). ROADMAP's
  M53 row still says M53 "Makes M35 (.btr terrain LOD) obsolete"; that
  tension is that PRD's decision Q4, not reopened here. This PRD completes
  the authored families either way.
- **DLOD-NG-007.** Fallout 76 and Starfield distant LOD (§7.1).
- **DLOD-NG-008.** Matching the original renderer's LOD look pixel for
  pixel. ROADMAP's anti-scope names per-pixel parity with the original's
  interior look; this applies the same rule to distant LOD
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
  Band distances are taken from the games' settings, not calibrated against
  captures of the original game (Q5).
- **DLOD-NG-009.** In-cell `NiLODNode` / `BSValueNode` LOD switches. These
  are authoring hints, not the open-world authority
  ([EXAL §5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need)).
- **DLOD-NG-010.** Far-field sun shadows. This PRD makes no shadow promise
  for receivers past the sun-shadow fade. LOD blocks within
  `LOD_SHADOW_CASTER_DISTANCE` (`crates/renderer/src/shader_constants.rs:33`,
  `SHADOW_FADE_END + DIRECTIONAL_SHADOW_TRACE_DISTANCE`) are already
  admitted to the TLAS as casters (`byroredux/src/render/static_meshes.rs`),
  so the gap is the fade distance, not missing LOD casters. The
  [Reference Lighting PRD](reference-lighting.md) records the gap as owned
  by neither PRD (RLT-NG-008) and holds the decision (its Q4); a tree ring
  added here inherits the same caster admission and no more.

---

## 5. Requirements by area

### 5.1 Coverage and reporting (`DLOD-GATE`)

- **DLOD-GATE-001.** The `lod-coverage:` line MUST carry `tree_resident=`,
  the number of resident tree-LOD files across every tree family. Today the
  line carries `terrain_resident` and `object_resident` only
  (`LodCoverageStats::machine_line`), and `update_lod_coverage`
  (`streaming_helpers.rs`) reads `state.lod_blocks` and
  `state.object_lod_blocks` only.
- **DLOD-GATE-002.** The `lod-coverage:` line MUST carry
  `placement_resident=`, the number of resident Oblivion placement cells.
  Today placement LOD is deliberately left out of the line
  (`update_lod_coverage` doc comment).
- **DLOD-GATE-003.** `full_detail_overlaps` MUST count tree-LOD cards that
  stand in a resident full-detail cell. Today the tree ring is not part of
  the count.
- **DLOD-GATE-004.** The `lod-coverage:` line MUST carry `tree_churn=`,
  with the same settled-snapshot semantics as `terrain_churn`
  (`ChurnTracker`).
- **DLOD-GATE-005.** The `lod-coverage:` line MUST carry
  `object_moat_cells=` and `tree_moat_cells=`, computed as §2 defines a moat
  cell. Today no gap metric exists, and nothing measures how wide the moat
  that whole-quad exclusion leaves actually is.
- **DLOD-GATE-006.** `m-exteriors.sh` boundary mode MUST hard-fail a profile
  when a family that §3.1 lists for its title has a resident count of 0
  after the traversal settles. Today the script fails on overlaps, churn,
  `held_back` and an incomplete line, but not on an absent family.
- **DLOD-GATE-007.** The vanilla census behind §3.1 MUST be checked in as an
  `#[ignore]` test, `lod_family_census`, that skips without game data and
  asserts the per-title family file counts. Until it passes, every
  **Vanilla data** figure in §3.1, and every count a requirement, scenario or
  decision takes from it, is a Phase 0 figure; where the test's count
  differs, the test wins and this document is revised.
- **DLOD-GATE-008.** Which families each title authors MUST be answered by
  the translate-tier predicates (`terrain_lod_layout`, `object_lod_scheme`,
  `tree_lod_supported`, `placement_lod_supported`, plus one for each new tree
  family). One test, `lod_family_table_covers_every_game`, MUST enumerate
  all six `GameKind` variants across every predicate. There are six, not
  seven: Fallout 3 and New Vegas share `GameKind::Fallout3NV`
  (`crates/plugin/src/esm/reader.rs:202-227`), so no predicate can tell them
  apart and a per-title difference between them comes from the archives
  (DLOD-TREE-009). Today each predicate has its own test
  (`object_lod_scheme_table`, `placement_lod_supported_is_oblivion_only`).

### 5.2 Distant trees (`DLOD-TREE`)

- **DLOD-TREE-001.** On Oblivion, a `.lod` group whose base is a TREE MUST
  produce distant-tree geometry for each placement in it. Today such groups
  are skipped (§3.1 F7). The CS wiki says the distant LOD data stores "the
  positions of distant trees and buildings" (CS wiki, *Update Distant LOD
  Data*).
- **DLOD-TREE-002.** The Oblivion tree ring MUST take its reach from
  `uGridDistantTreeRange`: 15 in the shipped `Oblivion_default.ini`, which
  UESP describes as "the visible distance on trees" (UESP, *Oblivion
  Mod:Oblivion.ini*). **Phase 0: verify** whether the value is a radius or a
  diameter in cells, and whether the closed-city variant
  `uGridDistantTreeRangeCity` (default 4 per the same UESP page; absent from
  the shipped INI) applies in closed-city worldspaces (V1).
- **DLOD-TREE-003.** An Oblivion distant tree MUST be textured from the
  source V2 settles, and the choice MUST be pinned by a test. The candidate is
  `textures\trees\billboards\`, which the 2026-11-01 census counts at 118
  textures (a Phase 0 figure, DLOD-GATE-007). It is not what the near-field
  importer resolves: `TREE_ICON_CANDIDATE_DIRS` (`references/import.rs:196`)
  tries `trees\leaves\` first, and its rustdoc records that `leaves\` holds
  93 of 93 vanilla `TREE.ICON` names and `billboards\` only 10. **Phase 0:
  verify** which texture the original draws a distant tree with and how it
  sizes it (V2).
- **DLOD-TREE-004.** FO3 `.dtl` files and their `treetypes.lst` MUST parse
  bounds-checked, as `parse_btt` does. The `#[ignore]` corpus test
  `dtl_corpus_parses_and_resolves_every_species` MUST parse every `.dtl` file
  DLOD-GATE-007 finds in the FO3 base and DLC archives (108 in the
  2026-11-01 census) and resolve every species. **Phase 0: verify** the
  layout (V3). The one-off census suggested a `.btt`-like header (a `u32`
  group count, then a type index and a count per group), records that do not
  fit the 32-byte `.btt` stride, and 36-byte `.lst` entries; none of this is
  established.
- **DLOD-TREE-005.** The `GameKind::Fallout3NV` tree ring MUST take its
  reach from `[TerrainManager] fTreeLoadDistance`, converted with
  `cells_from_bu` as the Skyrim ring is. The shipped FO3 and FNV
  `VeryHigh.ini` both author 40000 (line 26 of each), so one constant serves
  the shared `GameKind`.
- **DLOD-TREE-006.** Every tree family MUST translate into the one
  representation `tree_lod.rs` draws: one mesh per LOD file, `IsLodTerrain`,
  alpha-tested cards (Q3). Per-game differences MUST stay in the parse and
  translate step; no tree-specific branch may reach the renderer.
- **DLOD-TREE-007.** No distant-tree card MUST be drawn inside a resident
  full-detail cell. This is observable as DLOD-GATE-003 reading 0.
- **DLOD-TREE-008.** A tree-LOD file whose footprint touches resident cells
  SHOULD still draw the trees that stand in its non-resident cells, so
  `tree_moat_cells` reads 0. Today the whole file is vetoed
  (`full_detail_cells_veto_the_quads_they_touch`). No source read for this
  draft says how the original retires a LOD tree on approach. The CK wiki
  records only a custom-worldspace bug, "there are some issues with tree LOD
  where the LOD will not disappear, even if you move close to a tree. The
  cause of this issue isn't known yet" (CK wiki, *Creating a Custom
  Worldspace with LOD*), which does not say whether hiding is per tree or per
  quad. **Phase 0: verify** whether the original hides LOD trees
  individually, and the key that would match a LOD tree to its full tree: the
  `.btt` record's `u32` REFR FormID (`TreeLodInstance::ref_form_id`, parsed
  at `tree_lod.rs:219` and read nowhere else) or the cell of its position
  (V4).
- **DLOD-TREE-009.** FNV and FO4 MUST NOT start a separate tree ring:
  neither ships a tree family (§3.1). For FO4 this is `tree_lod_supported`
  returning `false`. FNV shares `GameKind::Fallout3NV` with FO3, so its
  outcome MUST come from the data, not from a predicate: the ring finds no
  `.dtl` in the loaded archives and spawns nothing, observable as
  `tree_resident=0` with no error on the `fnv boundary` run. FO4's `Ultra.ini`
  does author `fTreeLoadDistance=75000` (line 33), and FNV's `VeryHigh.ini`
  authors 40000, although `lod_bands.rs:29` lists the FO4 key as absent and
  `:116-117` says FO4 "author[s] no such setting". **Phase 0: verify** what
  the key governs on a title with no separate tree family (V10).
- **DLOD-TREE-010.** Each tree card's orientation and alpha cutoff MUST
  carry either an external source or the "interpretation, not the engine's
  draw" note `tree_lod.rs` carries today (`TREE_LOD_ALPHA_CUTOFF` is
  unsourced). **Phase 0: verify** the original's tree-LOD orientation
  (V5). `tree_lod.rs` asserts that the original's billboards face the
  camera, and cites no source for it.

### 5.3 Object-LOD boundary (`DLOD-OBJ`)

- **DLOD-OBJ-001.** `vwd_full_model_overlaps` MUST read 0 on every
  `m-exteriors.sh` boundary run. Today it does
  ([EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod)).
- **DLOD-OBJ-002.** A level-4 `.bto` quad that straddles resident cells
  SHOULD draw the segments of its non-resident cells and none of its
  resident cells, so `object_moat_cells` reads 0 at level 4 on Skyrim (Q1).
  Today a straddling quad is not drawn at all. EXAL's 2026-08-29 census
  found one segment per cell at level 4 in `Skyrim - Meshes1.bsa`
  (`crates/nif/examples/bto_segment_census.rs`).
- **DLOD-OBJ-003.** Level-8, -16 and -32 quads MUST NOT be drawn over a
  resident cell. They ship one segment per sub-mesh, so the lever there is
  the whole quad. Today this holds (`LodBandSelection::exclude_within`,
  `refine_within`, `lod_bands.rs`).
- **DLOD-OBJ-004.** FO4 level-4 `.bto` quads SHOULD get the treatment of
  DLOD-OBJ-002. **Phase 0: verify** that FO4's segment tables partition one
  segment per cell; the census covered Skyrim only (V6).
- **DLOD-OBJ-005.** FO3/FNV `blocks\` quads SHOULD get the treatment of
  DLOD-OBJ-002 if their `BSSegmentedTriShape` segments partition per cell.
  nif.xml defines the block's segment count as the "Number of segments in
  the square grid". **Phase 0: verify** the partition on the corpus (V7).
- **DLOD-OBJ-006.** A per-cell draw MUST NOT rely on per-REFR identity or on
  the VWD flag. The bake carries neither
  ([EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod),
  2026-08-26 correction), and the cell is the lever streaming already uses.
- **DLOD-OBJ-007.** On Oblivion, a VWD base with no `_far.nif` MUST keep
  drawing its full model as the distant mesh (Q6). Today this is the
  fallback in `placement_lod.rs` (`full_model_path`).

### 5.4 Distances and descriptors (`DLOD-BAND`)

- **DLOD-BAND-001.** Every per-game LOD distance constant MUST name its INI
  section, key and shipped value in its rustdoc, and a `cargo test` MUST pin
  the value. Today the Skyrim, FO4 and FO3 ladders do (`lod_bands.rs`).
  `PLACEMENT_LOD_RADIUS_CELLS` (16, `placement_lod.rs`) names no key, and
  the `lod_bands.rs` table misstates FO4's `fTreeLoadDistance` as absent.
- **DLOD-BAND-002.** The Oblivion placement ring MUST take its reach from
  `uGridDistantCount`: 25 in the shipped `Oblivion_default.ini`, which UESP
  says "controls viewable distance for LOD buildings". Today the ring
  reaches 16 cells (`PLACEMENT_LOD_RADIUS_CELLS`, `placement_lod.rs:84`).
  **Phase 0: verify** whether the value is a radius or a diameter, and
  whether `uGridDistantCountCity` (default 4 per UESP) applies in
  closed-city worldspaces (V1).
- **DLOD-BAND-003.** The FO3/FNV `.dlodsettings` descriptor MUST be read
  once per streaming state on the LOD source worldspace, as
  `resolve_lod_grid_origin` reads `<ws>.lod`. **Phase 0: verify** the layout
  (V8). The one-off census read `wasteland.dlodsettings` as 24 bytes that
  decode plausibly as `4`, `32`, `128` (`u32`), then `-64, -64` and `63, 63`
  (`i16` pairs), then `4` (`u32`); the field meanings are unestablished.
- **DLOD-BAND-004.** If V8 shows the descriptor's SW cell is the quad
  lattice origin, legacy quad selection MUST use it in place of the
  residue-majority prune (#5387) and the coverage tie-break (#5520). The
  test `dlodsettings_origin_tiles_every_authored_quad` MUST pass over every
  vanilla FO3 and FNV descriptor DLOD-GATE-007 finds (24 in the 2026-11-01
  census).
- **DLOD-BAND-005.** The worldspace "Distant LOD Multiplier" (CK wiki,
  *World Menu*; FO4 CK wiki, *WorldSpace*) MAY scale the object ring.
  **Phase 0: verify** which WRLD sub-record holds it, whether any vanilla
  worldspace authors a value other than 1, and what it scales (V9). Today
  no `WorldspaceRecord` field (`crates/plugin/src/esm/cell/worldspace.rs`)
  carries it.
- **DLOD-BAND-006.** Band selection MUST keep its documented reading of the
  INI thresholds (Chebyshev distance to the quad centre, `fSplitDistanceMult`
  not modelled; `lod_bands.rs` module docs) unless Q5 is decided otherwise.

### 5.5 Distant terrain residue (`DLOD-TERR`)

- **DLOD-TERR-001.** The Oblivion row of `terrain_lod_albedo_gain` MUST be
  measured with the A/B method its rustdoc records for FNV and FO4: the same
  pose at a small and a large radius, the same screen regions, a control
  region. The result goes in the rustdoc. Today the row is 1.0, unmeasured
  (`env_translate/lod.rs`).
- **DLOD-TERR-002.** The Skyrim row (0.36, measured on one Whiterun scene)
  SHOULD be re-measured on two more scenes under different weather or time
  of day. The rustdoc SHOULD record the spread.
- **DLOD-TERR-003.** Legacy distant-terrain geometry MUST stay synthesised
  from LAND (Q2). Today it is (`terrain_lod.rs` module docs).

### 5.6 Depth precision (`DLOD-DEPTH`)

- **DLOD-DEPTH-001.** Before the flip, `depth.stats` MUST be captured at
  pinned `--camera-pos` / `--camera-forward` poses on three scenes: the
  recorded FNV satellite-dish pose, Skyrim Tamriel and FO4 Commonwealth. The
  baseline table in the `ACTIVE_DEPTH_MAPPING` rustdoc MUST grow to all
  three. Today it records the FNV scene only (`camera.rs`).
- **DLOD-DEPTH-002.** The flip MUST be one commit that changes
  `ACTIVE_DEPTH_MAPPING` (`camera.rs:179`) to `DepthMapping::Reversed` and
  carries the regenerated `include/shader_constants.glsl` and every
  recompiled `.spv` that includes `include/depth_convention.glsl`.
  `BYRO_REVERSED_Z` is not hand-edited: `crates/renderer/build.rs` generates
  it from the constant (`shader_constants_data.rs:17`), and
  `the_shader_depth_convention_matches_the_engine_constant`
  (`shader_contract_tests.rs:6225`) asserts both that the generated header
  matches and that `depth_convention.glsl` defines no copy. That test MUST
  pass, and `the_shipped_depth_mapping_is_conventional` (`camera.rs:1347`)
  MUST be replaced by its reversed twin.
- **DLOD-DEPTH-003.** After the flip, the far decade's distinct depth codes
  MUST exceed the conventional baseline at the same pose on each of the
  three scenes.
- **DLOD-DEPTH-004.** After the flip, `m-exteriors.sh all static`,
  `all boundary`, `all cycle` and `all water` MUST return the same
  per-profile verdicts as a pre-flip control run on the same machine.
- **DLOD-DEPTH-005.** DLOD-DEPTH-004 MUST be run under both `--upscaler taa`
  and the default FSR path. FSR3's `depth_inverted` flag is one of the
  converted sites (#3308).
- **DLOD-DEPTH-006.** Bench-of-record frame times after the flip MUST fall
  within the noise of a same-machine control
  ([bench methodology](../../ROADMAP.md#how-to-run-a-bench)).

### 5.7 Budget (`DLOD-PERF`)

- **DLOD-PERF-001.** New tree rings MUST upload through the `Prop` admission
  tier ([Memory Budget](../engine/memory-budget.md#mesh-registry)), as the
  Skyrim tree ring does. `held_back` MUST read 0 after settle on every
  boundary profile.
- **DLOD-PERF-002.** Every new ring MUST reconcile under its own
  `LodWorkBudget` and the shared frame deadline in `reconcile_lod_rings`, as
  the four rings do today (`streaming_helpers.rs`).
- **DLOD-PERF-003.** On a pinned boundary pose, `main_render` time SHOULD
  stay within same-machine control noise after the Oblivion and FO3 tree
  rings land.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Full models within the 5×5 loaded area; LOD beyond it (CK wiki, *Static*; `uGridsToLoad=5` in the shipped `Oblivion_default.ini`) | Full detail within `DEFAULT_EXTERIOR_RADIUS` (12 cells, `scene.rs`); LOD rings start beyond it | Tools and INI tweaks that raise `uGridsToLoad` | The LOD rings start farther out and the geometry pool is shared → DLOD-PERF-001 (`held_back=0`) |
| 2 | Tree LOD billboards; their orientation is unsourced | Static crossed cards per tree (`tree_lod.rs`) | — | A visible X shape on a close approach → DLOD-TREE-010, V5, Q3 |
| 3 | Radial INI distances with `fSplitDistanceMult` (shipped `Ultra.ini` / `VeryHigh.ini`) | Chebyshev distance to the quad centre; split multiplier not modelled | — | Bands switch at slightly different distances → DLOD-BAND-006, Q5 |
| 4 | A VWD object with no `_far.nif` is not visible from a distance (CS wiki, *Visible When Distant Tutorial*) | The full model is drawn as the distant mesh | `_far.nif` packs made only to surface existing VWD flags | Cost of full meshes at range on modded data → DLOD-OBJ-007, Q6, V12 |
| 5 | Legacy terrain LOD meshes authored per quad (GECK wiki, *World LOD*: "Generate Terrain Meshes") | Geometry synthesised from LAND; only the authored textures are used | — | A mod that ships LOD meshes without matching LAND draws Redux's ground instead → DLOD-TERR-003, Q2 |
| 6 | The baked LOD diffuse as authored | Skyrim's baked ground scaled by 0.36 (`terrain_lod_albedo_gain`) | Mods that darken LOD textures | Calibrated on one scene → DLOD-TERR-002 |
| 7 | Whole-quad pop-in at the loaded-area edge (whatever the original does there, unverified) | Per-cell segment suppression at level 4 (Q1) | — | Index sub-range draws are a render-visible change → DLOD-OBJ-002, live-gated by DLOD-GATE-005 |

The FO3 boundary profile's seam gate is red today on MegatonWorld (#6500,
out of scope per DLOD-NG-003). Acceptance below reads the `lod-coverage:`
line, which that failure does not affect.

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Families F2, F3, F6, F7. F7 (trees) and the `uGridDistantCount` reach are the open work. Exteriors pass the `m-exteriors.sh` gates ([compatibility matrix](../../ROADMAP.md#compatibility-matrix)). Wave 2 |
| Fallout 3 | yes | F2, F3, F5, F9, F11. The `.dtl` trees and the `.dlodsettings` lattice are open. Wave 2 |
| Fallout: New Vegas | yes | F2, F3, F5, F11. No tree family is authored; FNV shares `GameKind::Fallout3NV` and the FO3 translation, and gets no tree ring because its archives hold no `.dtl` (DLOD-TREE-009), not through a per-title branch. Wave 1 for the gate and descriptor work |
| Skyrim (LE / SE / AE) | yes | F1, F3, F4, F8, F10. Acceptance runs on SE. LE and AE share `GameKind::Skyrim` and the translation; **Phase 0: verify** that LE's `Skyrim - Meshes.bsa` names the same families (V11). Wave 1 |
| Fallout 4 | yes | F1, F3, F4, F10. Trees are part of the `.bto` (**Phase 0: verify**, V13). Wave 1 |
| Fallout 76 | no | No cell loader ([Game Compatibility, FO76](../engine/game-compatibility.md#fallout-76)). The `BakedBto` arm of `object_lod_scheme` stays, with no acceptance promised; `m-exteriors.sh` names FO76 a policy skip |
| Starfield | n/a | Ships no `.bto` / `.btr` (`object_lod_scheme_table`), and only the Cydonia interior is supported |

Wave 1 (gate, moat, depth) runs on the three Tier-1 titles with
bench-of-record scenes. Wave 2 adds the tree families that need new readers.
Reversed-Z is renderer-wide and is accepted on all five.

### 7.2 Acceptance

All scenarios need the vanilla game data on the machine. Nothing is checked
in. Engine runs use `--bench-hold` with `BYRO_DEBUG_SERVER=1` and
`byro-dbg` on port 9876 ([smoke-test index](../smoke-tests/README.md)).

| # | Scenario | Data | Passes when |
|---|---|---|---|
| A1 | `docs/smoke-tests/m-exteriors.sh skyrim boundary` | Skyrim SE | `lod-coverage:` shows nonzero `terrain_resident`, `object_resident` and `tree_resident`; `overlaps`, `full_detail_overlaps`, `vwd_full_model_overlaps`, `tree_churn`, `held_back`, `object_moat_cells` and `tree_moat_cells` all 0 |
| A2 | `docs/smoke-tests/m-exteriors.sh fo4 boundary` | FO4 | as A1 without `tree_resident`; `object_moat_cells=0` if V6 holds |
| A3 | `docs/smoke-tests/m-exteriors.sh fnv boundary` | FNV | as A2 for the `blocks\` ring |
| A4 | `docs/smoke-tests/m-exteriors.sh oblivion boundary` | Oblivion | nonzero `placement_resident` and `tree_resident`; overlaps 0; `held_back=0` |
| A5 | `cargo run --release -- --game fo3 --wrld Wasteland --grid 0,0 --radius 3 --bench-frames 300 --bench-hold`, then `lod.coverage` over `byro-dbg` | FO3 | nonzero `tree_resident`; overlaps 0. (The `fo3` boundary profile sits in MegatonWorld, which has no `.dtl` of its own) |
| A6 | `cargo test -p byroredux --bin byroredux -- --ignored dtl_corpus_parses_and_resolves_every_species` | FO3 base and DLC | every `.dtl` and `.lst` file DLOD-GATE-007 counts parses; every species resolves |
| A7 | `cargo test -p byroredux --bin byroredux -- --ignored dlodsettings_origin_tiles_every_authored_quad` | FO3, FNV | every descriptor DLOD-GATE-007 counts decodes; every authored `blocks\` and terrain quad sits on its descriptor's lattice |
| A8 | `cargo test -p byroredux --bin byroredux lod_family_table_covers_every_game` | none | all six `GameKind` variants are classified by every family predicate |
| A9 | Three pinned poses (DLOD-DEPTH-001); at each, `depth.stats` twice (arm, then read), before and after the flip | FNV, Skyrim SE, FO4 | far-decade distinct codes after > before on all three |
| A10 | `m-exteriors.sh all static|boundary|cycle|water`, before and after the flip, under `--upscaler taa` and FSR | all five | identical per-profile verdicts |
| A11 | Bench-of-record refresh with a same-machine control | bench scenes | frame times within control noise |

A boundary-run scenario reads the `lod-coverage:` line only. Its other
gates (seams, image health) keep their own owners.

---

## 8. Metrics

The numbers live in [ROADMAP.md](../../ROADMAP.md): in the M35 row of the
[milestone tiers](../../ROADMAP.md#milestone-tiers) while the milestone is
open, and in the
[compatibility matrix](../../ROADMAP.md#compatibility-matrix)'s "Verified
content" column once it closes. Frame times go to the bench-of-record.

| Metric | Definition | Granularity |
|---|---|---|
| Family coverage | families Drawn ÷ families authored (§3.1) | per title |
| Tree-LOD parse rate | tree-LOD files that parse ÷ files shipped | per title, per archive |
| Moat cells | `object_moat_cells`, `tree_moat_cells` after settle | per title, per boundary profile |
| Overlap and churn | sum of the overlap and churn fields after settle (a gate, expected 0) | per boundary profile |
| Far-decade depth codes | distinct codes in the 100 000–400 000 BU decade | per pinned scene, before and after the flip |
| Frame-time delta | after ÷ before, against a same-machine control | per bench scene |

---

## 9. Delivery sequence

Depends on M32 (landscape) and M40 (streaming), both closed. Phase 5 is
independent of Phases 2–4 and may run in parallel with them.

**Phase 0 — Verify, census and decide (no engine code).** Close V1–V13
(below) from external sources or vanilla-data censuses. Get Q1–Q6 decided.
Write the missing design text: the Oblivion and FO3 tree families in EXAL
§5.2, and a reversed-Z section in the exterior readiness plan. File a
tracking issue for the reversed-Z flip (none is open). Check in the
DLOD-GATE-007 census and revise every Phase 0 figure it contradicts. Correct
the stale M35 row in ROADMAP. Exit: `lod_family_census` passes on the
owner's machine, 13 of 13 verify items closed or filed as issues,
6 of 6 decisions carry **Decided**, and the M35 row's open list names only
work this PRD tracks.

| V | Question | Settled by |
|---|---|---|
| V1 | Are `uGridDistantTreeRange` / `uGridDistantCount` radii or diameters in cells, and do the `…City` variants (default 4, UESP) apply in closed-city worldspaces? | A second written source (UESP talk pages, the CS wiki); else the owner rules on the risk |
| V2 | Which texture and size the original uses for a distant Oblivion tree (`trees\billboards\` or the `TREE.ICON` leaves texture) | CS-wiki / UESP texture documentation; a census pairing the `trees\billboards\` DDS names with the `.lod` TREE bases |
| V3 | `.dtl` record layout and `treetypes.lst` entry layout | A byte census over every shipped `.dtl` and `.lst` file, checked in as DLOD-TREE-004's test |
| V4 | Does the original hide LOD trees one at a time, and by which key (REFR FormID or cell)? | A census matching `.btt` `ref_form_id` values to `Skyrim.esm` REFRs and their cells |
| V5 | Does the original's tree LOD face the camera? | A published source (CK wiki, xLODGen / DynDOLOD documentation); else the static cross stays an interpretation |
| V6 | Do FO4 level-4 `.bto` segments partition one per cell? | `bto_segment_census.rs` run over `Fallout4 - Meshes.ba2` |
| V7 | Do FO3/FNV `blocks\` segments partition one per cell? | The same census over `Fallout - Meshes.bsa` |
| V8 | `.dlodsettings` field meanings | A census over every vanilla descriptor against the authored quad names (`LegacyLodQuadIndex`) |
| V9 | The WRLD sub-record for "Distant LOD Multiplier", its vanilla values, and what it scales | xEdit's WRLD definitions (`wbDefinitionsTES5.pas`, `wbDefinitionsFO4.pas`) and a master census |
| V10 | What `fTreeLoadDistance` governs on FO4 and FNV, which author it but ship no separate tree family | FO4 CK wiki, GECK wiki or a published INI reference |
| V11 | Does Skyrim LE ship the same family names as SE? | A name-table census of `Skyrim - Meshes.bsa` (LE) |
| V12 | Is a VWD object with no `_far.nif` invisible at distance in the original? | A second source beside the CS wiki tutorial (TES4LODGen / TES4qLOD documentation) |
| V13 | Are FO4 trees part of the `.bto` bake? | FO4 CK wiki or xLODGen documentation, plus a census of `.bto` sub-mesh texture paths |

**Phase 1 — The gate sees every ring.** Ships DLOD-GATE-001 to -008 and
DLOD-BAND-001, measured on today's families. Exit: all five boundary
profiles print the extended line; Skyrim's `tree_resident` is nonzero; the
first moat counts are published in the M35 row; A8 passes.

**Phase 2 — Distant trees for Oblivion and FO3.** Ships DLOD-TREE-001 to
-007, -009 and -010, DLOD-BAND-002 and DLOD-PERF-001 to -003. Exit: A4, A5
and A6 pass; tree-LOD parse rate is 100% on FO3; family coverage is 100% on
Oblivion and FO3.

**Phase 3 — No moat.** Ships DLOD-OBJ-002 to -006 and DLOD-TREE-008, as Q1
decides. Exit: A1 to A3 pass with both moat counts at 0 wherever V6 and V7
confirmed a per-cell lever; `vwd_full_model_overlaps` stays 0.

**Phase 4 — Authored lattices and calibration.** Ships DLOD-BAND-003 to
-006 and DLOD-TERR-001 to -003. Exit: A7 passes; the Oblivion albedo row is
measured and recorded; FO3/FNV moat and overlap counts are unchanged or
better than the Phase 1 figures.

**Phase 5 — Reversed-Z.** Ships DLOD-DEPTH-001 to -006. Exit: A9, A10 and
A11 pass, and the three-scene before/after table is published.

---

## 10. Decisions required

1. **Q1 — How to close the object-LOD moat at the resident boundary.**
   Recommended: per-cell segment suppression at level 4 (DLOD-OBJ-002), with
   the VWD radius left coupled to the full-detail radius. It closes the gap
   with the lever the bake already provides and needs no per-object
   identity. The cost is index sub-range draws and a live validation pass.
   Alternative A: #3307 as filed, giving VWD REFRs a spawn radius beyond
   `radius_unload`. It reintroduces the #1866 overlap risk, and it does not
   close the moat for non-VWD content. Alternative B: defer. M35 closes on
   ring separation and the moat stays, now measured by DLOD-GATE-005.
2. **Q2 — Legacy distant-terrain geometry.** Recommended: keep synthesising
   from LAND (DLOD-TERR-003). The boundary block needs synth for its holes
   and skirts anyway, and both sources come from the same LAND. Alternative:
   read the authored `Landscape\LOD\*.nif` meshes. That matches mods which
   ship LOD meshes, at the cost of a second geometry path with its own seam
   and skirt work.
3. **Q3 — How Oblivion and FO3 distant trees are drawn.** Recommended: the
   `tree_lod.rs` representation, one crossed-card mesh per LOD file, so
   there is one draw shape and one gate for every tree family
   (DLOD-TREE-006). Alternative: one yaw-facing `Billboard` entity per tree,
   reusing the near-field path. It matches the near-field look, but the
   entity count grows with every distant tree placed.
4. **Q4 — Reversed-Z.** Recommended: flip once A9 to A11 pass (Phase 5),
   which turns the measured far-decade gain into the default. Alternative:
   keep `Conventional` and drop the item from M35. Depth stays the precision
   floor between LOD terrain and the LOD objects standing on it.
5. **Q5 — Band calibration.** Recommended: accept the INI-derived ladder and
   its documented reading as final (DLOD-BAND-006), consistent with the
   anti-scope on matching the original's look. Alternative: calibrate the
   distances against captures of the original game. It needs the original
   running beside Redux and a capture method; that is per-pixel parity work.
6. **Q6 — Oblivion VWD objects with no `_far.nif`.** Recommended: keep
   drawing the full model (DLOD-OBJ-007). A VWD flag then always means
   "visible at distance", and on vanilla data the 2026-11-01 census puts
   it at 4 bases and 6 placements (§3.1, a Phase 0 figure until
   DLOD-GATE-007). Alternative: match the original as the CS wiki
   describes it and skip them. Behaviour becomes faithful for mods, but the
   modder's flag then does nothing without a `_far.nif`.

---

## 11. Document map

| Need | Go to |
|---|---|
| LOD families, providers, VWD rule, segment census | [EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod) |
| Terrain-LOD join and albedo gain | [EXAL §5.4.2](../engine/exal.md#542-the-terrain-lod-join-2026-10-2627) |
| Descriptor and inherited LOD key | [EXAL §5.4.1](../engine/exal.md#541-the-inherited-lod-key-5496-and-the-authored-descriptor) |
| What runtime LOD does not need | [EXAL §5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need) |
| EX-10/11 history (ladder, gate, VWD, reversed-Z) | [Exterior readiness plan](../engine/exterior-readiness-plan.md#ex-1011-2371--near-terrain-correctness-and-distant-lod-bands) |
| LOD scheduling in streaming | [Exterior Grid Streaming](../engine/exterior-grid-streaming.md#4-streaming-phase-1-async-pre-parse-worker) |
| Pool admission for LOD uploads | [Memory Budget](../engine/memory-budget.md#mesh-registry) |
| Tree tiers above the distant ring | [EXAL Trees](../engine/exal-trees.md#7-the-lod-chain) |
| Boundary gate and its fields | [`m-exteriors.sh`](../smoke-tests/m-exteriors.sh), [smoke-test index](../smoke-tests/README.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| Bench methodology | [ROADMAP, how to run a bench](../../ROADMAP.md#how-to-run-a-bench) |
| Anti-scope and keep list | [ROADMAP anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope), [keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |
| Virtual geometry and its stance on authored LOD | [Virtual Geometry PRD](virtual-geometry.md) (VGEO-NG-001, Q4) |
| Far-field sun shadows (unowned) | [Reference Lighting PRD](reference-lighting.md) (RLT-NG-008, Q4) |

External sources, all local dumps under `/mnt/data/src/reference/` unless
marked otherwise:

| Source | Used for |
|---|---|
| UESP, *Oblivion Mod:Oblivion.ini* (`uesp-wiki`) | `uGridDistantTreeRange`, `uGridDistantCount`, `bDisplayLODTrees` |
| CS wiki: *Update Distant LOD Data*, *Visible When Distant Tutorial*, *TES4LODGen*, *TES4qLOD* (`cs-uesp-wiki`) | `.lod` holds tree and building positions; `_far.nif` requirement |
| GECK wiki, *World LOD* (`geck-uesp-wiki`) | FO3/FNV `blocks` and `trees` LOD folders, `LODSettings`, generated terrain meshes |
| CK wiki: *Static*, *Creating a Custom Worldspace with LOD*, *World Menu* (`ck-uesp-wiki`); FO4 CK wiki, *WorldSpace* (`falloutck-uesp-wiki`) | Has Distant LOD and the 5×5 loaded area; tree LOD hiding; Distant LOD Multiplier |
| nif.xml, `BSGeometrySegmentData`, `BSSegmentedTriShape` (`nifxml/nif.xml`) | LOD segments "in a grid" |
| Shipped INIs: Skyrim SE `Ultra.ini`, FO4 `Ultra.ini`, FO3 and FNV `VeryHigh.ini`, `Oblivion_default.ini` (game installs) | Band, tree and object distances |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: six `GameKind` variants not seven (GATE-008, A8); FO3/FNV tree split made data-driven (TREE-005, -009); generated `BYRO_REVERSED_Z` (DEPTH-002); census counts framed as Phase 0 figures (§3.1, GATE-007, TREE-003/-004, BAND-003/-004, A6, A7, Q6); tree texture source vs `TREE_ICON_CANDIDATE_DIRS` (TREE-003); CK tree-LOD citation narrowed (TREE-008); closed-city INI keys (V1); M53 and far-field-shadow non-goals linked to sibling PRDs (NG-006, NG-010) | claude: revise |
