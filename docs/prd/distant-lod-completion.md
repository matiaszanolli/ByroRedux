# Distant LOD Completion — Product Requirements

**Status**: ACTIVE (2026-11-05). Revision 6 records the Phase 0 census results (§9.1). Revision 5 files the tracking issues. Revision 4 recorded the owner's §10 decisions, every one the recommendation. Revision 3 folded in review round 2:
every HEAD claim re-checked on 2026-11-03, Skyrim's tree-LOD shape and the
Distant LOD Multiplier sub-record sourced, the far-field-shadow non-goal
aligned with the Reference Lighting PRD's decided Q4, descriptors given
their own state, and one new decision (Q7). Revision 2 folded in review
round 1. Revision 1 was the first draft: the remainder of ROADMAP milestone
M35 (Terrain LOD), re-scoped against the code at HEAD, which has already
shipped three of the five items the M35 row lists as open.
Statements about how an original engine, file format or tool behaves cite
external sources (the local UESP / CS / CK / GECK wiki dumps, nif.xml,
xEdit's record definitions, DynDOLOD's published help, the games' shipped
INI files); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Tracking**: #6633 (parent). Phases: #6636 (0), #6637 (1), #6638 (2), #6639 (3), #6640 (4), #6642 (5).

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
| Oblivion and FO3 distant-tree families; per-tree LOD retirement (DLOD-TREE-008) | None yet — Phase 0 adds them to [EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod) |

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
| **Full-detail radius** | `radius_unload`, the Chebyshev radius within which a full-detail cell may be resident (`LodReconcileInput::full_detail_radius`). It is `--radius` + 1 (`streaming/mod.rs:590`). The default `--radius` is `DEFAULT_EXTERIOR_RADIUS`, 12 cells (`scene.rs:39`); every `m-exteriors.sh` profile runs `--radius 1`. |
| **Reach** | The Chebyshev distance in cells from the player's cell to a ring's outer edge. A ring draws only between the full-detail radius and its reach (§5.4, Q7). |
| **Overlap** | A LOD entity drawn over a resident full-detail cell. Counted by `lod.coverage` as `full_detail_overlaps` and `vwd_full_model_overlaps`. |
| **Moat cell** | A non-resident cell next to the resident set (within one finest-level quad edge of it) inside a family's ring, covered by no entity of that family. The gap that whole-quad exclusion leaves. New metric, defined by DLOD-GATE-005. |
| **Segment** | A `BSGeometrySegmentData` triangle range in a `BSSubIndexTriShape` / `BSSegmentedTriShape`; nif.xml describes these as "segmented in a grid (for LOD)" (`nifxml/nif.xml`, `BSGeometrySegmentData`). |
| **VWD** | Visible When Distant, Oblivion's name for it; the Creation Kit calls it "Has Distant LOD". It marks a base object as one to show "in LOD beyond the 5x5 loaded area" (CK wiki, *Static*). At HEAD it is materialised as the `VisibleWhenDistant` marker component. |
| **LOD descriptor** | The per-worldspace settings file under `lodsettings\`: `<ws>.lod` (Skyrim, FO4) or `<ws>.dlodsettings` (FO3, FNV). |
| **Depth mapping** | `DepthMapping::Conventional` or `DepthMapping::Reversed`, selected by `ACTIVE_DEPTH_MAPPING` (`camera.rs`). |
| **Far decade** | The 100 000–400 000 BU band of a `depth.stats` capture, where the LOD ring sits. |

---

## 3. Core model

Every LOD family on every title is in exactly one state. Drawn and Drawn,
gapped apply to the ring families (F1–F9), which put geometry in a ring;
Read applies to the descriptors (F10, F11), which put none.

| State | Meaning | Reported as |
|---|---|---|
| **Drawn** | Parsed, translated and resident on a boundary run, with zero overlaps and zero moat cells | nonzero `<family>_resident`, `verdict=PASS` |
| **Drawn, gapped** | Drawn, but whole-quad exclusion leaves moat cells at the resident boundary | nonzero moat count |
| **Read** | A descriptor whose fields each have a consumer, or are named unconsumed with the reason in the reader's rustdoc | the reader's `cargo test` passes |
| **Unconsumed** | Shipped in the vanilla archives, never read | no resident count; listed in §3.1 |
| **Not authored** | The title ships no such family | the family predicate returns `false`, pinned by a test; where one `GameKind` spans two titles (FO3 and FNV), the archives hold no file of the family and the ring reports `0` without failing (DLOD-TREE-009) |

### 3.1 Family inventory

The state at HEAD on 2026-11-03. The code column was re-checked against
the source that day. The **Vanilla data** counts are not checked facts: they come
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
| F6 | Oblivion `.lod` placements, STAT groups → `_far.nif` | Oblivion | 9 889 `.lod`; 118 STAT bases with a `_far.nif` (901 placements), 8 without (29) | Drawn; invisible to the gate | `placement_lod.rs`; full-model fallback for a base with no `_far.nif` (Q6); left out of `update_lod_coverage` by its doc comment (`streaming_helpers.rs:331-336`) |
| F7 | Oblivion `.lod` placements, TREE groups | Oblivion | 82 TREE bases, 98 661 placements: the large majority of all `.lod` placements | **Unconsumed** | `spawn_placement_lod_cell` (`placement_lod.rs:528`) resolves a group's base through `record_index.cells.statics`, which TREE records also populate (`dispatch_world_placement.rs:64-67`). A TREE's `MODL` is a bare `.spt` name (`references/import.rs:337-341`), so `far_nif_path` returns `None`, the `meshes\`-prefixed full path resolves in no archive, and the group reaches `None => continue` (`placement_lod.rs:548`) |
| F8 | Skyrim tree billboards `.btt` + `<ws>.lst` + `treelod` atlas | Skyrim | 386 level-4 `.btt` (`tree_lod.rs` module docs) | Drawn, gapped; invisible to the gate | `tree_lod.rs` (#4913, `a9384f9c0`); whole-quad exclusion (`footprint_intersects_full_detail`, pinned by `full_detail_cells_veto_the_quads_they_touch`); not counted by `update_lod_coverage` |
| F9 | FO3 distant trees `.dtl` + `treetypes.lst` | FO3 | 75 `.dtl` + 4 `.lst` in `Fallout - Meshes.bsa` (71 at level 8, 4 at level 4), plus 33 `.dtl` + 7 `.lst` across Anchorage, BrokenSteel, PointLookout and ThePitt | **Unconsumed** | no reader: `.dtl` and `treetypes` appear nowhere in the code |
| F10 | `lodsettings\<ws>.lod` descriptor | Skyrim, FO4 | 16 B per worldspace | Read | `parse_lod_settings_origin` (#2586, `lod_support.rs:138`) reads the SW cell, pinned by `lod_settings_descriptor_gives_the_sw_cell`; its rustdoc names the extent and the two level fields and says why nothing consumes them |
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

- **DLOD-G-001 — Every authored family drawn.** Every ring family that
  §3.1 marks Unconsumed or Drawn, on each of the five titles, reaches Drawn,
  and every descriptor reaches Read. Family coverage (§8) is 100% on every
  title.
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
  [Reference Lighting PRD](reference-lighting.md) decided it (its Q4,
  2026-11-02): on-screen sun shadow beyond the fade is that PRD's
  screen-space term (RLT-CON-004), and traced sun shadows past
  `SHADOW_FADE_END` and anything off-screen stay out of scope
  (RLT-NG-008). Not reopened here. A tree ring added here inherits the same
  caster admission and no more.

---

## 5. Requirements by area

### 5.1 Coverage and reporting (`DLOD-GATE`)

- **DLOD-GATE-001.** The `lod-coverage:` line MUST carry `tree_resident=`,
  the number of resident tree-LOD units across every tree family: one per
  `.btt` or `.dtl` file, and one per Oblivion `.lod` cell with at least one
  TREE group drawn. Today the
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
  when one of `terrain_resident`, `object_resident`, `placement_resident` or
  `tree_resident` reads 0 after the traversal settles while the
  DLOD-GATE-007 census finds that ring's family in the profile's worldspace.
  Today the script fails on overlaps, churn, `held_back` and an incomplete
  line (`m-exteriors.sh:841-852`), but not on an absent family. The
  worldspace qualifier matters: the `fo3` boundary profile sits in
  MegatonWorld, which the 2026-11-01 census found without a `.dtl` of its
  own (a Phase 0 figure).
- **DLOD-GATE-007.** The vanilla census behind §3.1 MUST be checked in as an
  `#[ignore]` test, `lod_family_census`, that skips without game data and
  asserts the family file counts per title and per worldspace. Its archive
  half extends the family matching `crates/bsa/examples/probe_lod_corpus.rs`
  already does (`.btr`, `.bto`, `.btt`, `landscape\lod` terrain and
  `blocks\`, `distantlod\`, `lodsettings\*.lod`) with the families it does
  not count today (`.dtl`, `treetypes.lst`, `.dlodsettings`) rather than
  duplicating it. Until the test passes, every **Vanilla data** figure in
  §3.1, and every count a requirement, scenario or decision takes from it,
  is a Phase 0 figure; where the test's count differs, the test wins and
  this document is revised.
- **DLOD-GATE-008.** Which families each title authors MUST be answered by
  the per-game family predicates (`terrain_lod_layout`, `object_lod_scheme`,
  `tree_lod_supported`, `placement_lod_supported`, plus one for each new tree
  family). One test, `lod_family_table_covers_every_game`, MUST enumerate
  all six `GameKind` variants across every predicate. There are six, not
  seven: Fallout 3 and New Vegas share `GameKind::Fallout3NV`
  (`crates/plugin/src/esm/reader.rs:202-227`), so no predicate can tell them
  apart and a per-title difference between them comes from the archives
  (DLOD-TREE-009). Today the four predicates are already pinned over all
  six variants, in three tests: `object_lod_scheme_table`
  (`object_lod.rs:1342`, object and tree), `placement_lod_supported_is_oblivion_only`
  (`placement_lod.rs:911`) and `combined_and_legacy_lod_capabilities_are_disjoint`
  (`lod_support.rs:672`, `terrain_lod_layout` through its two wrappers).
  The new test replaces those three tables with one and adds the new tree
  predicates to it.

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
  the shipped INI) applies in closed-city worldspaces (V1). Read as a
  diameter it is 7 cells, inside the default full-detail radius
  (DLOD-BAND-007, Q7).
- **DLOD-TREE-003.** An Oblivion distant tree MUST be textured from the
  source V2 settles, and the choice MUST be pinned by a test. The candidate is
  `textures\trees\billboards\`, which the 2026-11-01 census counts at 118
  textures (a Phase 0 figure, DLOD-GATE-007). It is not what the near-field
  importer resolves: `TREE_ICON_CANDIDATE_DIRS` (`references/import.rs:259`)
  tries `trees\leaves\` first, and its rustdoc (`:243-252`) records that
  `leaves\` holds 93 of 93 vanilla `TREE.ICON` names and `billboards\` only
  10. **Phase 0:
  verify** which texture the original draws a distant tree with and how it
  sizes it (V2).
- **DLOD-TREE-004.** FO3 `.dtl` files and their `treetypes.lst` MUST parse
  bounds-checked, as `parse_btt` does. The `#[ignore]` corpus test
  `dtl_corpus_parses_and_resolves_every_species` MUST parse every `.dtl` file
  DLOD-GATE-007 finds in the FO3 base and DLC archives (108, confirmed) and
  resolve every species. The layout is established; the trailing bytes of 38
  files are never referenced by a declared chunk, so the reader trusts the
  headers and ignores them (V3, §9.1, #6678): a `u32` chunk count, then chunks of
  `u32` type, `u32` count and 32-byte records; `treetypes.lst` entries are 32
  bytes (a `u32` index and seven `f32`), not 36.
- **DLOD-TREE-005.** The `GameKind::Fallout3NV` tree ring MUST take its
  reach from `[TerrainManager] fTreeLoadDistance`, converted with
  `cells_from_bu` as the Skyrim ring is. The shipped FO3 and FNV
  `VeryHigh.ini` both author 40000 (line 26 of each), so one constant serves
  the shared `GameKind`. No source read for this draft says what the key
  bounds; **Phase 0: verify** that it is the reach of the `.dtl` ring (V10).
  40000 BU is 10 cells, inside the default full-detail radius, so at the
  default `--radius` this ring draws nothing unless Q7 takes its
  alternative (DLOD-BAND-007).
- **DLOD-TREE-006.** Every tree family MUST translate into the one
  representation `tree_lod.rs` draws: one mesh per LOD file, `IsLodTerrain`,
  alpha-tested cards (Q3). Per-game differences MUST stay in the parse and
  translate step; no tree-specific branch may reach the renderer.
- **DLOD-TREE-007.** A distant-tree card MUST NOT be drawn inside a resident
  full-detail cell. This is observable as DLOD-GATE-003 reading 0.
- **DLOD-TREE-008.** A tree-LOD file whose footprint touches resident cells
  SHOULD still draw the trees that stand in its non-resident cells, so
  `tree_moat_cells` reads 0. Today the whole file is vetoed
  (`full_detail_cells_veto_the_quads_they_touch`). No source read for this
  draft states how the original retires a LOD tree on approach. The CK wiki
  records only a custom-worldspace bug, "there are some issues with tree LOD
  where the LOD will not disappear, even if you move close to a tree. The
  cause of this issue isn't known yet" (CK wiki, *Creating a Custom
  Worldspace with LOD*). DynDOLOD's *Tree LOD* help lists among the system's
  limitations that "Tree LOD does not unload for references added by ESL
  flagged plugins", which points at a per-reference key without stating
  one. **Phase 0: verify** whether the original hides LOD trees
  individually, and the key that would match a LOD tree to its full tree: the
  `.btt` record's `u32` REFR FormID (`TreeLodInstance::ref_form_id`, parsed
  at `tree_lod.rs:219` and read by no production code) or the cell of its
  position (V4).
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
- **DLOD-TREE-010.** Each tree family's card shape and alpha cutoff MUST
  carry, in the rustdoc of the code that builds them, either an external
  source or an "interpretation, not the engine's draw" note. For Skyrim the
  shape is sourced: DynDOLOD's *Tree LOD* help says of the game's own tree
  LOD that "The tree LOD billboard texture is shown on 2 double sided planes
  intersecting each other at a 90 degree angle", upright with "No x or y
  rotation angles", which is the crossed pair `tree_lod.rs` draws. Today
  `tree_lod.rs:37-38` says the opposite ("The engine's tree LOD billboards
  face the camera"), unsourced, and `TREE_LOD_ALPHA_CUTOFF` is marked
  unsourced. **Phase 0: verify** the Oblivion and FO3 tree-LOD shape (V5).

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
  DLOD-OBJ-002. Verified (V6, §9.1): FO4's level-4 sub-meshes partition their
  triangles one segment per cell, 713 of 713, as Skyrim's do.
- **DLOD-OBJ-005.** FO3/FNV `blocks\` quads MUST keep the whole-quad treatment: they have no per-cell lever (V7,
  §9.1): each is one `NiTriShape` with no `BSSegmentedTriShape` or sub-index
  block, in all 422 FO3 and 355 FNV quads read. They keep the whole-quad
  treatment of DLOD-OBJ-003: Phase 3 cannot close their moat per cell.
- **DLOD-OBJ-006.** A per-cell draw MUST NOT rely on per-REFR identity or on
  the VWD flag. The bake carries neither
  ([EXAL §5.2](../engine/exal.md#52-the-lodprovider-trait-gamevariant-for-lod),
  2026-08-26 correction), and the cell is the lever streaming already uses.
- **DLOD-OBJ-007.** On Oblivion, a VWD base with no `_far.nif` MUST keep
  drawing its full model as the distant mesh (Q6). Today this is the
  fallback in `placement_lod.rs` (`full_model_path`).

### 5.4 Distances and descriptors (`DLOD-BAND`)

- **DLOD-BAND-001.** Every per-game LOD distance constant MUST say in its
  rustdoc either its INI section, key and shipped value, or that it is not
  INI-derived and why, and a `cargo test` MUST pin the value. Today the
  Skyrim and FO4 ladders and Skyrim's tree reach do, in the `lod_bands.rs`
  module table and rustdoc, pinned by
  `ladders_match_vanilla_terrain_manager_distances` and `tree_lod_tests.rs:238`.
  The FO3/FNV ladder names its keys in a `//` comment, not rustdoc
  (`lod_bands.rs:129-140`): its refine distances are
  `fBlockLoadDistanceLow` 50000 BU stepped by `fSplitDistanceMult` 1.5, and
  its outer extent `FALLOUT_LEGACY_MAX_CELLS` (64) is the archives' quad
  extent, not the authored `fBlockLoadDistance` 125000 BU (about 31 cells);
  it is pinned by `fallout_legacy_terrain_uses_all_four_authored_bands`.
  `PLACEMENT_LOD_RADIUS_CELLS` (16, `placement_lod.rs:84`) names no key, and
  `lod_bands.rs:29` and `:116-117` misstate FO4's `fTreeLoadDistance` as
  absent (FO4 `Ultra.ini` line 33 authors 75000).
- **DLOD-BAND-002.** The Oblivion placement ring MUST take its reach from
  `uGridDistantCount`: 25 in the shipped `Oblivion_default.ini`, which UESP
  says "Controls viewable distance for LOD buildings, ruins, etc." Today the
  ring reaches 16 cells (`PLACEMENT_LOD_RADIUS_CELLS`, `placement_lod.rs:84`).
  **Phase 0: verify** whether the value is a radius or a diameter, and
  whether `uGridDistantCountCity` (default 4 per UESP) applies in
  closed-city worldspaces (V1). Read as a diameter it is 12 cells, inside
  the default full-detail radius (DLOD-BAND-007, Q7).
- **DLOD-BAND-003.** The FO3/FNV `.dlodsettings` descriptor MUST be read
  once per streaming state on the LOD source worldspace, as
  `resolve_lod_grid_origin` reads `<ws>.lod`. The layout is established
  (§9.1, V8): `u32 4`, `u32` top level, `u32` 4 x top level, `i16` SW x, y,
  `i16` NE x, y, `u32` 4 or 8.
- **DLOD-BAND-004.** V8 shows the descriptor's SW cell is the quad lattice
  origin for 46 of the 53 vanilla descriptors and not for seven FO3
  worldspaces (§9.1), so it is not a universal replacement. Legacy quad
  selection MUST use the SW cell as the lattice origin where every authored
  quad tiles on it, and MUST keep the residue-majority prune (#5387) and the
  coverage tie-break (#5520) for a descriptor that does not (or has none).
  The test `dlodsettings_origin_tiles_every_authored_quad` MUST assert that
  split over every vanilla FO3 and FNV descriptor (24 base-game, 53 with
  DLC): all 25 FNV descriptors and 21 FO3 ones tile, the seven named in §9.1
  do not.
- **DLOD-BAND-005.** The worldspace "Distant LOD Multiplier" (CK wiki,
  *World Menu*; FO4 CK wiki, *WorldSpace*) MAY scale the object ring. xEdit
  defines it as the WRLD `NAMA` float on Skyrim and FO4
  (`wbFloat(NAMA, 'Distant LOD Multiplier')`, `wbDefinitionsTES5.pas:10687`,
  `wbDefinitionsFO4.pas:13152`); neither wiki page describes what it does.
  **Phase 0: verify** whether any vanilla worldspace authors a value other
  than 1, and what it scales (V9). Today no `WorldspaceRecord` field
  (`crates/plugin/src/esm/cell/worldspace.rs:146`) carries it.
- **DLOD-BAND-006.** Band selection MUST keep its documented reading of the
  INI thresholds unless Q5 is decided otherwise: Chebyshev distance to the
  quad centre (`lod_bands.rs` module docs); on Skyrim and FO4 the
  `fBlockLevel<i>Distance` values with `fSplitDistanceMult` not modelled; on
  FO3/FNV `fSplitDistanceMult` generating the successive boundaries from
  `fBlockLoadDistanceLow` (`lod_bands.rs:129-140`). The original's metric and
  reference point, and where it applies `fSplitDistanceMult`, are
  unverified (V14).
- **DLOD-BAND-007.** A ring whose reach ends inside the full-detail radius
  MUST spawn nothing and MUST NOT fail the coverage verdict; its resident
  count reads 0, unless Q7 takes its alternative. Observable on a default
  `--radius` run of FO3 Wasteland once DLOD-TREE-005 lands
  (`tree_resident=0`, `verdict=PASS`).

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
  pinned `--camera-pos` / `--camera-forward` poses on three scenes: the FNV
  satellite-dish scene, Skyrim Tamriel and FO4 Commonwealth. The measured
  baseline table MUST grow to all three, each row with its pose
  coordinates. Today the table sits in `DEFAULT_RENDER_DISTANCE`'s rustdoc
  (`camera.rs:116-131`, which `ACTIVE_DEPTH_MAPPING`'s rustdoc cites) and
  records the FNV scene only, by description ("camera on the Mojave
  satellite-dish rise") rather than by pose.
- **DLOD-DEPTH-002.** The flip MUST be one commit that changes
  `ACTIVE_DEPTH_MAPPING` (`camera.rs:179`) to `DepthMapping::Reversed` and
  carries the regenerated `include/shader_constants.glsl` and every
  recompiled `.spv` it changes; `scripts/check-shader-artifacts.sh`, which
  recompiles every shader and requires byte-identical SPIR-V, MUST pass on
  that commit. `BYRO_REVERSED_Z` is not hand-edited:
  `crates/renderer/build.rs` generates it from the constant
  (`shader_constants_data.rs:17`), and
  `the_shader_depth_convention_matches_the_engine_constant`
  (`shader_contract_tests.rs:6342`) asserts both that the generated header
  matches and that `depth_convention.glsl` defines no copy. That test MUST
  pass, and `the_shipped_depth_mapping_is_conventional` (`camera.rs:1347`)
  MUST be replaced by its reversed twin. The "Flipping it" rustdoc on
  `ACTIVE_DEPTH_MAPPING` (`camera.rs:163-169`) still describes a hand edit
  of `depth_convention.glsl`; the flip commit corrects it.
- **DLOD-DEPTH-003.** After the flip, the far decade's distinct depth codes
  MUST exceed the conventional baseline at the same pose on each of the
  three scenes.
- **DLOD-DEPTH-004.** After the flip, `m-exteriors.sh all static`,
  `all boundary`, `all cycle` and `all water` MUST return the same
  per-profile verdicts as a pre-flip control run on the same machine.
- **DLOD-DEPTH-005.** DLOD-DEPTH-004 MUST be run under both `--upscaler taa`
  and the default `fsr3` upscaler (`cli_args.rs:171`). FSR3's
  `depth_inverted` flag is one of the converted sites (#3308). Today
  `m-exteriors.sh` passes `--upscaler taa` in every mode
  (`m-exteriors.sh:322`), so the FSR half cannot be run; Phase 5 adds an
  upscaler override to the script.
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
  the four rings do today (`streaming_helpers.rs:215-257`).
- **DLOD-PERF-003.** On a pinned boundary pose, `main_render` time SHOULD
  stay within same-machine control noise after the Oblivion and FO3 tree
  rings land.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Full models within the 5×5 loaded area; LOD beyond it (CK wiki, *Static*; `uGridsToLoad=5` in the shipped `Oblivion_default.ini`) | Full detail within `DEFAULT_EXTERIOR_RADIUS` (12 cells, `scene.rs:39`); LOD rings start beyond it | Tools and INI tweaks that raise `uGridsToLoad` | The LOD rings start farther out and the geometry pool is shared → DLOD-PERF-001 (`held_back=0`). An authored reach shorter than the full-detail radius leaves its ring empty at the default radius → DLOD-BAND-007, Q7 |
| 2 | Skyrim: each LOD tree on "2 double sided planes intersecting each other at a 90 degree angle", upright (DynDOLOD, *Tree LOD*). Oblivion, FO3: unverified | The same crossed cards for every tree family (`tree_lod.rs`, DLOD-TREE-006) | — | Oblivion or FO3 may draw distant trees another way → DLOD-TREE-010, V5, Q3 |
| 3 | `[TerrainManager]` distances in BU with `fSplitDistanceMult` (shipped `Ultra.ini` / `VeryHigh.ini`); the metric and reference point are unverified | Chebyshev distance to the quad centre; the split multiplier generates FO3/FNV's boundaries and is not modelled on Skyrim/FO4 | — | Bands switch at slightly different distances → DLOD-BAND-006, V14, Q5 |
| 4 | A VWD object with no `_far.nif` is not visible from a distance (CS wiki, *Visible When Distant Tutorial*) | The full model is drawn as the distant mesh | `_far.nif` packs made only to surface existing VWD flags | Cost of full meshes at range on modded data → DLOD-OBJ-007, Q6, V12 |
| 5 | Legacy terrain LOD meshes authored per quad, generated from the land: GECK wiki, *World LOD* ("Generate Terrain Meshes: Creates terrain meshes for the world from the local land geometry"); CS wiki, *Landscape LOD Tutorial* (Heightmap Editor, *Generate LOD*) | Geometry synthesised from LAND; only the authored textures are used | — | A mod that ships LOD meshes without matching LAND draws Redux's ground instead → DLOD-TERR-003, Q2 |
| 6 | The baked LOD diffuse as authored | Skyrim's baked ground scaled by 0.36 (`terrain_lod_albedo_gain`) | Mods that darken LOD textures | Calibrated on one scene → DLOD-TERR-002 |
| 7 | Whole-quad pop-in at the loaded-area edge (whatever the original does there, unverified) | Per-cell segment suppression at level 4 (Q1) | — | Index sub-range draws are a render-visible change → DLOD-OBJ-002, live-gated by DLOD-GATE-005 |
| 8 | FO3/FNV author `fBlockLoadDistance=125000` BU (about 31 cells) in `VeryHigh.ini`; what it bounds is unverified | The legacy ring reaches 64 cells, the archives' quad extent (`FALLOUT_LEGACY_MAX_CELLS`) | — | If V14 shows the key bounds the ring, Redux draws FO3/FNV LOD about twice as far, at Prop-tier cost → DLOD-BAND-001, DLOD-PERF-001, Q5 |

The FO3 boundary profile's seam gate is red today on MegatonWorld (#6500,
out of scope per DLOD-NG-003). Acceptance below reads the `lod-coverage:`
line, which that failure does not affect.

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Families F2, F3, F6, F7. F7 (trees) and the `uGridDistantCount` reach are the open work. Exterior state: [compatibility matrix](../../ROADMAP.md#compatibility-matrix). Wave 2 |
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
| A3 | `docs/smoke-tests/m-exteriors.sh fnv boundary` | FNV | as A2 for the `blocks\` ring, and `tree_resident=0` with the verdict unaffected (DLOD-TREE-009) |
| A4 | `docs/smoke-tests/m-exteriors.sh oblivion boundary` | Oblivion | nonzero `placement_resident` and `tree_resident`; overlaps 0; `held_back=0` |
| A5 | `cargo run --release -- --game fo3 --wrld Wasteland --grid 0,0 --radius 3 --bench-frames 300 --bench-hold`, then `lod.coverage` over `byro-dbg` | FO3 | nonzero `tree_resident`; overlaps 0. The `fo3` boundary profile sits in MegatonWorld, which the census found without a `.dtl` (a Phase 0 figure); `--radius 3` keeps the 10-cell tree reach outside the full-detail radius (DLOD-BAND-007) |
| A6 | `cargo test -p byroredux --bin byroredux -- --ignored dtl_corpus_parses_and_resolves_every_species` | FO3 base and DLC | every `.dtl` and `.lst` file DLOD-GATE-007 counts parses; every species resolves |
| A7 | `cargo test -p byroredux --bin byroredux -- --ignored dlodsettings_origin_tiles_every_authored_quad` | FO3, FNV | every descriptor DLOD-GATE-007 counts decodes; every authored `blocks\` and terrain quad sits on its descriptor's lattice |
| A8 | `cargo test -p byroredux --bin byroredux lod_family_table_covers_every_game` | none | all six `GameKind` variants are classified by every family predicate |
| A9 | Three pinned poses (DLOD-DEPTH-001); at each, `depth.stats` twice (arm, then read), before and after the flip | FNV, Skyrim SE, FO4 | far-decade distinct codes after > before on all three |
| A10 | `m-exteriors.sh` with `all static`, `all boundary`, `all cycle` and `all water`, before and after the flip, once with the script's `--upscaler taa` and once with the `fsr3` override DLOD-DEPTH-005 adds | all five | identical per-profile verdicts |
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
| Family coverage | (ring families Drawn + descriptors Read) ÷ families authored (§3.1) | per title |
| Tree-LOD parse rate | tree-LOD files that parse ÷ files shipped | per title, per archive |
| Moat cells | `object_moat_cells`, `tree_moat_cells` after settle | per title, per boundary profile |
| Overlap and churn | sum of the overlap and churn fields after settle (a gate, expected 0) | per boundary profile |
| Far-decade depth codes | distinct codes in the 100 000–400 000 BU decade | per pinned scene, before and after the flip |
| Frame-time delta | after ÷ before, against a same-machine control | per bench scene |

---

## 9. Delivery sequence

Depends on M32 (landscape) and M40 (streaming), both closed. Phase 5 is
independent of Phases 2–4 and may run in parallel with them.

**Phase 0 — Verify, census and decide (no engine code).** Close V1–V14
(below) from external sources or vanilla-data censuses. Get Q1–Q7 decided.
Write the missing design text: the Oblivion and FO3 tree families and
per-tree retirement in EXAL §5.2, and a reversed-Z section in the exterior
readiness plan. File a tracking issue for the reversed-Z flip (none is
open). Check in the DLOD-GATE-007 census and revise every Phase 0 figure it
contradicts. Correct the stale M35 row in ROADMAP, and the Status
paragraph's distant-LOD line (`ROADMAP.md:44`), which still credits
`_far.nif` LOD to FO3/FNV. Exit: `lod_family_census` passes on the owner's
machine, 14 of 14 verify items closed or filed as issues, 7 of 7 decisions
carry **Decided**, and the M35 row's open list names only work this PRD
tracks.

| V | Question | Settled by |
|---|---|---|
| V1 | Are `uGridDistantTreeRange` / `uGridDistantCount` radii or diameters in cells, and do the `…City` variants (default 4, UESP) apply in closed-city worldspaces? | A second written source (UESP talk pages, the CS wiki); else the owner rules on the risk |
| V2 | Which texture and size the original uses for a distant Oblivion tree (`trees\billboards\` or the `TREE.ICON` leaves texture) | CS-wiki / UESP texture documentation; a census pairing the `trees\billboards\` DDS names with the `.lod` TREE bases |
| V3 | `.dtl` record layout and `treetypes.lst` entry layout | A byte census over every shipped `.dtl` and `.lst` file, checked in as DLOD-TREE-004's test |
| V4 | Does the original hide LOD trees one at a time, and by which key (REFR FormID or cell)? | A census matching `.btt` `ref_form_id` values to `Skyrim.esm` REFRs and their cells; DynDOLOD's *Tree LOD* help (no unload for ESL-flagged references) points at a per-reference key without stating it |
| V5 | Do Oblivion and FO3 draw distant trees as Skyrim does, on two upright planes crossed at 90° (DynDOLOD, *Tree LOD*, which covers Skyrim only)? | CS / GECK wiki, TES4LODGen / TES4qLOD documentation; else the crossed cards stay an interpretation for those two titles |
| V6 | Do FO4 level-4 `.bto` segments partition one per cell? | `bto_segment_census.rs` run over `Fallout4 - Meshes.ba2` |
| V7 | Do FO3/FNV `blocks\` segments partition one per cell? | The same census over `Fallout - Meshes.bsa` |
| V8 | `.dlodsettings` field meanings | A census over every vanilla descriptor against the authored quad names (`LegacyLodQuadIndex`) |
| V9 | The vanilla values of WRLD `NAMA` "Distant LOD Multiplier" (sub-record settled by xEdit, DLOD-BAND-005) and what it scales | A `NAMA` census over `Skyrim.esm`, `Fallout4.esm` and their DLC masters; a published description of the field |
| V10 | What `fTreeLoadDistance` governs: the `.dtl` ring on FO3, and anything on FO4 and FNV, which author it but ship no separate tree family | FO4 CK wiki, GECK wiki or a published INI reference |
| V11 | Does Skyrim LE ship the same family names as SE? | A name-table census of `Skyrim - Meshes.bsa` (LE) |
| V12 | Is a VWD object with no `_far.nif` invisible at distance in the original? | A second source beside the CS wiki tutorial (TES4LODGen / TES4qLOD documentation) |
| V13 | Are FO4 trees part of the `.bto` bake? | FO4 CK wiki or xLODGen documentation, plus a census of `.bto` sub-mesh texture paths |
| V14 | How the original measures the `[TerrainManager]` distances (metric, reference point on a quad), where it applies `fSplitDistanceMult`, and whether FO3/FNV `fBlockLoadDistance` bounds the LOD ring | A published source (CK / GECK wiki, an INI reference); else the documented reading stands under Q5 |

### 9.1 Phase 0 results (2026-11-05)

The censuses are `#[ignore]` tests run on the owner's machine (`crates/plugin/tests/lod_family_census.rs`,
`byroredux/src/cell_loader/placement_lod.rs` and `tree_lod_tests.rs`, `crates/nif/examples/bto_segment_census.rs`);
the written sources are the local wiki dumps and the StepMods pages cited. Every §3.1 *Vanilla data* figure and
every count a requirement repeats from it reproduced, except one: F6's STAT bases without a `_far.nif` are 8 with
29 placements, not 4 with 6 (corrected in §3.1 and Q6). The 118 `trees\billboards\` DDS, F7's 82 TREE bases and
98 661 placements, F6's 118 with a `_far.nif` and 901, the 108 FO3 `.dtl`, the 24 base-game descriptors (15 FO3,
9 FNV; 53 with DLC), the 386 `.btt` and the 75 + 33 `.dtl` / 4 + 7 `.lst` split all match. MegatonWorld has no
`.dtl` and no `.dlodsettings`.

| V | Answer | Source |
|---|---|---|
| V1 | **Partly settled; the owner rules on the rest.** The `…City` variants do apply in closed-city worldspaces: UESP, *Oblivion Mod:Oblivion.ini*: "Does the same thing as uGridDistantTreeRange when you are inside a closed city worldspace" (`uGridDistantCountCity` likewise, default 4). Radius vs diameter has no second source; the only wording is UESP's "15 cells out the world will appear to drop off suddenly", which reads as a radius. | UESP page only, so Q7's ruling stands as the risk owner |
| V2 | The original's distant tree texture is `textures\trees\billboards\<.spt stem>.dds`: all 82 TREE bases the `.lod` groups instance pair one-to-one with a billboard by `.spt` stem (0 by `ICON`), out of 118 billboards. Its size is not documented. | `oblivion_tree_billboard_census` |
| V3 | **Settled (#6678).** `treetypes.lst` is a `u32` count and 32-byte entries (a `u32` index 0, 1, 2, … and seven `f32`; 10 of 11 exact, `wasteland` declares 8 and holds 9). `.dtl` is a `u32` chunk count then chunks of `u32` type, `u32` record count (at most 114) and 32-byte records `{x, y, z, scale, f32, base FormID, chunk index, 0xFFFFFF00}`; 70 of 108 are exact, 38 carry undeclared trailing bytes that no declared chunk references (every declared record sits in the chunk its index word names, and in every world the highest tree type a declared chunk uses is below the `.lst` header count; `wasteland` 7 against 8), so the reader trusts the headers. Not the 36-byte `.lst` entries or the `.btt`-unlike records the one-off census suggested. | `fallout3_tree_file_layouts`; #6678 |
| V4 | A `.btt` record's `ref_form_id` is the FormID of the placed TREE reference it bakes: 43 333 distinct ids; 41 538 resolve in `Skyrim.esm`, all to a TREE base, at the baked position (within 1 unit) and in its cell; 1 709 are DLC references, 3 sit in a persistent cell, 83 are absent from the parsed references (cause not examined). Whether the original hides LOD trees one at a time is not documented; the key is available either way. | `tree_lod_ref_form_id_census` |
| V5 | **Not settled by a source: the crossed cards stay an interpretation for Oblivion and FO3.** UESP's TREE record page calls `BNAM` "Billboard dimensions" (the near tree), and its TES4LODGen page is a stub with no description of the LOD tree shape. DLOD-TREE-010's rustdoc note applies. | UESP *Mod File Format/TREE*, *TES4LODGen* |
| V6 | FO4 level-4 `.bto` quads partition their triangles one segment per cell: 713 of 713 sub-meshes (Skyrim 1 856 of 1 856), at most 16 segments; levels 8, 16 and 32 have exactly 1. | `bto_segment_census` |
| V7 | **No.** FO3/FNV `blocks\` quads (FO3 422, FNV 355 read) are one `NiTriShape` under a `BSMultiBoundNode` each; none holds a segmented or sub-index block, so there is no per-cell lever (DLOD-OBJ-005's premise fails) and the whole quad stays the unit. | `bto_segment_census --blocks` |
| V8 | The 24-byte layout is `u32 4`, `u32` top level, `u32` 4 x top level, `i16` SW cell x, y, `i16` NE cell x, y, `u32` 4 or 8. Over all 53 descriptors the first is 4 (53), the third is 4 x the second (51), the second is the highest authored level (46), the last the lowest (50), and the SW cell is an authored quad's corner (48). **The SW cell is not a universal lattice origin**: every authored quad is on its lattice for 46 of 53; 7 FO3 worldspaces (`dcworld01`, `dcworld06`, `dcworld12`, `dcworld17`, `dlc02baileyscrossroads`, `paradisefalls`, `washmontop`) are not. FNV: all 25 are. The NE cell is the world's cell extent, not the quads'. | `dlodsettings_descriptors_against_the_authored_quads` |
| V9 | Every vanilla WRLD of `Skyrim.esm`, its DLC and `Fallout4.esm` + DLC authors `NAMA`, all 1.0 except WhiterunWorld, WindhelmWorld, MarkarthWorld and AlftandWorld at 0.25. What it scales is not published (the CK and FO4 CK wiki pages list it with no description; UESP calls it "unknown float"). | `worldspace_distant_lod_multiplier_census` |
| V10 | On FO4 `fTreeLoadDistance` is "believed to be unused by the game"; it "used to set the maximum distance that tree LOD was loaded in Skyrim". What it bounds on FO3 / FNV is not documented, so DLOD-TREE-005's reading (the `.dtl` ring's reach) stays an interpretation. | StepMods, *Fallout4Prefs INI / TerrainManager* |
| V11 | Yes. Skyrim LE's base archives hold 5 618 LOD names in the same families; all but one (a `labyrinthianmazeworld` `.bto`) exist under the same name in SE. SE adds 5 459 names for the DLC worldspaces, which LE ships in its DLC archives (not read). | `skyrim_le_ships_the_same_lod_family_names_as_se` |
| V12 | A missing or bad `_far.NIF` makes the distant object black, not invisible (UESP, *PCS Library / All distant (lod) objects turn black*); the CS wiki tutorial says the game "will automatically seek out" the `_far.nif`. Neither describes a missing file exactly; Q6's full-model fallback stands. | UESP *PCS Library*; CS wiki *Visible When Distant Tutorial* |
| V13 | FO4 `.bto` sub-meshes name only `textures\terrain\<ws>\` atlases (980 sub-meshes: commonwealth 922, sanctuaryhillsworld 48, diamondcity 10); none names a tree texture. Two FO4 worldspaces (`diamondcity`, `sanctuaryhillsworld`) ship a tree `.lst` with no `.btt`. No CK or xLODGen page was found to confirm what the bake holds. | `bto_segment_census --textures` |
| V14 | **Not settled by a source: the documented reading stands under Q5.** The published INI guides name `fBlockLevel<N>Distance` as the "maximum distance" of each object LOD level and `fSplitDistanceMult` the "distance multiplier for landscape LOD levels" (default 0.75), without a metric, reference point or units; `fBlockLoadDistance` is not explained. | StepMods, *Fallout4Prefs INI / TerrainManager* |

**Phase 1 — The gate sees every ring.** Ships DLOD-GATE-001 to -008 and
DLOD-BAND-001, measured on today's families. Exit: all five boundary
profiles print the extended line; Skyrim's `tree_resident` is nonzero; the
first moat counts are published in the M35 row; A8 passes.

**Phase 2 — Distant trees for Oblivion and FO3.** Ships DLOD-TREE-001 to
-007, -009 and -010, DLOD-BAND-002 and -007 as Q7 decides, and
DLOD-PERF-001 to -003. Exit: A3, A4, A5 and A6 pass; tree-LOD parse rate is
100% on FO3; family coverage is 100% on Oblivion and FO3.

**Phase 3 — No moat.** Ships DLOD-OBJ-002 to -006 and DLOD-TREE-008, as Q1
decides. Exit: A1 to A3 pass with both moat counts at 0 wherever V6 and V7
confirmed a per-cell lever; `vwd_full_model_overlaps` stays 0.

**Phase 4 — Authored lattices and calibration.** Ships DLOD-BAND-003 to
-006 and DLOD-TERR-001 to -003. Exit: A7 passes; the Oblivion albedo row is
measured and recorded; FO3/FNV moat and overlap counts are unchanged or
better than the Phase 1 figures.

**Phase 5 — Reversed-Z.** Ships DLOD-DEPTH-001 to -006, including the
`m-exteriors.sh` upscaler override DLOD-DEPTH-005 needs. Exit: A9, A10 and
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
   **Decided** (2026-11-03): the recommendation — per-cell segment
   suppression at level 4 (DLOD-OBJ-002), with the VWD radius left coupled
   to the full-detail radius.
2. **Q2 — Legacy distant-terrain geometry (Oblivion, FO3, FNV).**
   Recommended: keep synthesising it from LAND (DLOD-TERR-003). The boundary
   block needs synth for its holes and skirts anyway, and the original tools
   generated the authored meshes from the land too (GECK wiki, *World LOD*:
   "Creates terrain meshes for the world from the local land geometry"; CS
   wiki, *Landscape LOD Tutorial*), so on vanilla data both describe the
   same ground. Consequence: a mod that ships LOD meshes without matching
   LAND shows Redux's ground, not the mod's. Alternative: read the authored
   `Landscape\LOD\*.nif` meshes. Consequence: such mods show their own
   distant ground, at the cost of a second geometry path with its own seam
   and skirt work.
   **Decided** (2026-11-03): the recommendation — keep synthesising
   Oblivion, FO3 and FNV distant terrain from LAND (DLOD-TERR-003).
3. **Q3 — How Oblivion and FO3 distant trees are drawn.** Recommended: the
   `tree_lod.rs` representation, one mesh of crossed upright cards per LOD
   file (DLOD-TREE-006). It is the shape Skyrim's own tree LOD draws
   (DynDOLOD, *Tree LOD*: "2 double sided planes intersecting each other at
   a 90 degree angle"), and it gives one draw shape and one gate for every
   tree family. Consequence: if V5 finds Oblivion or FO3 drew distant trees
   another way, those two titles look different at range. Alternative: one
   yaw-facing `Billboard` entity per tree, reusing the near-field path.
   Consequence: it matches the near-field look, but the entity count grows
   with every distant tree placed.
   **Decided** (2026-11-03): the recommendation — Oblivion and FO3 distant
   trees use the `tree_lod.rs` representation, one mesh of crossed upright
   cards per LOD file (DLOD-TREE-006); V5 may still show those titles drew
   them another way.
4. **Q4 — Reversed-Z.** Recommended: flip once A9 to A11 pass (Phase 5),
   which turns the measured far-decade gain into the default. Alternative:
   keep `Conventional` and drop the item from M35. Depth stays the precision
   floor between LOD terrain and the LOD objects standing on it.
   **Decided** (2026-11-03): the recommendation — flip to reversed-Z once
   A9 to A11 pass (Phase 5).
5. **Q5 — Band calibration.** Recommended: accept the INI-derived ladder and
   its documented reading as final (DLOD-BAND-006), consistent with the
   anti-scope on matching the original's look. The reading is: Chebyshev
   distance to the quad centre; `fSplitDistanceMult` generating FO3/FNV's
   band boundaries and not modelled on Skyrim/FO4; FO3/FNV's ring reaching
   the archives' 64-cell quad extent. Consequence: bands may switch at
   slightly different distances than the original's, and if V14 shows that
   FO3/FNV `fBlockLoadDistance` (about 31 cells) bounds the ring, Redux
   draws that LOD about twice as far. Alternative: calibrate the distances
   against captures of the original game. Consequence: it needs the
   original running beside Redux and a capture method; that is per-pixel
   parity work.
   **Decided** (2026-11-03): the recommendation — the INI-derived band
   ladder and its documented reading are final (DLOD-BAND-006).
6. **Q6 — Oblivion VWD objects with no `_far.nif`.** The CS wiki says such
   an object "will not be visible in the game from distant" (*Visible When
   Distant Tutorial*; V12: UESP's *PCS Library* says a missing `_far.NIF` turns the distant object black, not invisible). Recommended: keep drawing
   the full model (DLOD-OBJ-007). Consequence: a VWD flag always means
   "visible at distance", at the cost of full meshes at range; on vanilla
   data `oblivion_lod_census` (DLOD-GATE-007) counts 8 bases and 29
   placements (§3.1; the 2026-11-01 hand census said 4 and 6). Alternative: match the original and
   skip them. Consequence: behaviour becomes faithful for mods, but the
   modder's flag then does nothing without a `_far.nif`.
   **Decided** (2026-11-03): the recommendation — keep drawing the full
   model of an Oblivion VWD object with no `_far.nif` (DLOD-OBJ-007).
7. **Q7 — Rings whose authored reach ends inside the full-detail radius.**
   Redux keeps full detail out to 12 cells by default
   (`DEFAULT_EXTERIOR_RADIUS`); the original kept a 5×5 loaded area
   (`uGridsToLoad=5`). Some authored reaches are shorter than 12 cells:
   FO3's `fTreeLoadDistance` (40000 BU, 10 cells), and Oblivion's
   `uGridDistantCount` (25) and `uGridDistantTreeRange` (15) if V1 reads
   them as diameters (12 and 7 cells). Recommended: the authored reach stays
   the ring's outer bound (DLOD-BAND-007). Consequence: at the default
   radius such a ring draws nothing, because full models already stand
   farther out than the original's LOD did; boundary runs use `--radius 1`,
   so the gate still measures every ring; and if V1 finds diameters,
   Oblivion's building horizon at the default radius shrinks from today's
   16 cells to the 12 the full models cover. Alternative: every ring reaches
   at least 4 cells past `--radius`, the margin today's Oblivion placement
   ring keeps (16 cells against the default 12). Consequence: some LOD
   always stands past the full-detail edge at any radius, so the horizon
   exceeds the original's wherever its INI value is shorter, and that INI
   value no longer bounds the ring (an exception to DLOD-G-004).
   **Decided** (2026-11-03): the recommendation — the authored reach stays
   a ring's outer bound (DLOD-BAND-007), so at the default radius such a
   ring draws nothing.

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
| Far-field sun shadows (decided there) | [Reference Lighting PRD](reference-lighting.md) (RLT-NG-008, RLT-CON-004, Q4) |
| Vanilla LOD family census tool | `crates/bsa/examples/probe_lod_corpus.rs` (DLOD-GATE-007 extends it) |

External sources, all local dumps under `/mnt/data/src/reference/` unless
marked otherwise:

| Source | Used for |
|---|---|
| UESP, *Oblivion Mod:Oblivion.ini* (`uesp-wiki`) | `uGridDistantTreeRange`, `uGridDistantCount` and their `…City` variants |
| CS wiki: *Update Distant LOD Data*, *Visible When Distant Tutorial*, *TES4LODGen*, *TES4qLOD*, *Landscape LOD Tutorial* (`cs-uesp-wiki`) | `.lod` holds tree and building positions; `_far.nif` requirement; Oblivion terrain LOD meshes generated from the heightmap |
| GECK wiki, *World LOD* (`geck-uesp-wiki`) | FO3/FNV `blocks` and `trees` LOD folders, `LODSettings`, terrain meshes generated "from the local land geometry" |
| CK wiki: *Static*, *Creating a Custom Worldspace with LOD*, *World Menu* (`ck-uesp-wiki`); FO4 CK wiki, *WorldSpace* (`falloutck-uesp-wiki`) | Has Distant LOD and the 5×5 loaded area; tree LOD hiding; Distant LOD Multiplier |
| xEdit WRLD definitions, `wbDefinitionsTES5.pas:10687`, `wbDefinitionsFO4.pas:13152` (local copies under `/mnt/data/tmp/`) | `NAMA` "Distant LOD Multiplier" |
| DynDOLOD help, *Tree LOD* (<https://dyndolod.info/Help/Tree-LOD>, online) | Skyrim's tree LOD on two crossed double-sided upright planes; no unload for ESL-flagged references |
| nif.xml, `BSGeometrySegmentData`, `BSSegmentedTriShape` (`nifxml/nif.xml`) | LOD segments "in a grid" |
| Shipped INIs: Skyrim SE `Ultra.ini`, FO4 `Ultra.ini`, FO3 and FNV `VeryHigh.ini`, `Oblivion_default.ini` (game installs) | Band, tree and object distances |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: six `GameKind` variants not seven (GATE-008, A8); FO3/FNV tree split made data-driven (TREE-005, -009); generated `BYRO_REVERSED_Z` (DEPTH-002); census counts framed as Phase 0 figures (§3.1, GATE-007, TREE-003/-004, BAND-003/-004, A6, A7, Q6); tree texture source vs `TREE_ICON_CANDIDATE_DIRS` (TREE-003); CK tree-LOD citation narrowed (TREE-008); closed-city INI keys (V1); M53 and far-field-shadow non-goals linked to sibling PRDs (NG-006, NG-010) | claude: revise |
| 3 | 2026-11-03 | Review round 2 folded in: HEAD re-checked on 2026-11-03 (stale line anchors in F7, TREE-003, DEPTH-002; family-predicate tests already cover all six variants, GATE-008; FO3/FNV ladder models `fSplitDistanceMult` and clamps at 64 cells, BAND-001, BAND-006, §6 rows 3 and 8; depth baseline lives under `DEFAULT_RENDER_DISTANCE` and names no pose, DEPTH-001; `m-exteriors.sh` pins `--upscaler taa`, DEPTH-005, A10); Skyrim tree-LOD shape sourced from DynDOLOD (TREE-010, V5, §6 row 2, Q3) and per-reference unload hint (TREE-008, V4); `NAMA` sourced from xEdit (BAND-005, V9); `fTreeLoadDistance` meaning made a verify item (TREE-005, V10); new V14; Read state for descriptors (§3, F10, G-001, metrics); F6 marked invisible to the gate; GATE-006 scoped per worldspace; GATE-007 extends `probe_lod_corpus`; NG-010 cites the Reference Lighting PRD's decided Q4; Q2 and Q6 sourced and given explicit consequences; new BAND-007 and Q7 (rings inside the full-detail radius) | claude: revise |
| 4 | 2026-11-03 | Owner decisions Q1–Q7 recorded, every one the recommendation | owner: approved |
| 5 | 2026-11-03 | → ACTIVE: parent #6633 and phase issues #6636, #6637, #6638, #6639, #6640, #6642 filed; the Phase 0 verify items are acceptance criteria of #6636 | — |
| 6 | 2026-11-05 | Phase 0 census figures reproduced (§9.1): F6's STAT bases without a `_far.nif` are 8 / 29, not 4 / 6; V1–V14 answered or filed (#6678); DLOD-BAND-004's premise (the `.dlodsettings` SW cell as the lattice origin) holds for 46 of 53 descriptors, DLOD-OBJ-005's (segmented `blocks\` quads) for none | — |
