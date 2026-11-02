# Procedural Exterior Cells — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (the authored-cell lookup is ~40 read sites, not one branch; distant
terrain does not cover generated land for free; placement identity aligned
with the LMC-ID profile model and the M50 output plugin; worldspace record
for a new domain; bake through WED-OUT-008, format-neutral; existing `terrain.seams` / `cell.owners` / `lod.coverage`
observables reused; Starfield citation narrowed). Written against the code
at HEAD on 2026-11-01; the decisions it needs from the project owner are
collected in §10 and nothing below is built yet. Statements about how the
original engines, their tools or published techniques behave cite external
sources (the local Construction Set / GECK / Creation Kit wiki dumps under
`/mnt/data/src/reference/`, UESP's file-format pages, the Starfield wiki, the
Gibbed.Starfield record table, published papers); a claim that could not be
verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: M64. The engine generates exterior cells (heightmap, landscape
texture splat, water, scattered base objects) at runtime from a seed, a set
of biome rules and noise, at grid coordinates where no plugin authored a
cell. Generated cells stream, collide, carry ground cover and persist
through saves exactly as authored cells do, because they enter the engine
as the same parsed cell data the exterior loader already consumes; they
draw distant terrain through the synthesised finest band only (PGEN-NG-004).
A rule set is content in an ordinary mod package: its identity, activation
and save behaviour are the plugin layer's, not a second scheme. The promise is for players of the supported titles who want more
explorable land, and for modders who want to author a biome instead of
hand-placing a worldspace. Out of scope: replacing or editing any authored
cell; generated interiors, settlements, quests, actors or navmesh;
reproducing Starfield's own planet generator; distant object LOD for
generated land.

**Related**: this document states the *promise*; the designs below own the
*how*. The generator itself has no design yet: Phase 0 writes it as an EXAL
sub-document, in the same relation to EXAL as the ground-cover design.

| Mechanism | Owning design |
|---|---|
| Exterior streaming, radius, worker, apply budget, cell unload | [Exterior Grid Streaming](../engine/exterior-grid-streaming.md) |
| Terrain, splat, water, distant LOD, the translate boundary | [EXAL](../engine/exal.md) |
| Ground cover over any terrain (engine-authored density field) | [EXAL — Ground Cover](../engine/exal-groundcover.md) |
| Engine-generated, versioned, disk-cached terrain data (precedent) | [EXAL — Terrain height maps](../engine/exal-terrain-height.md) |
| Form ID identity, plugin identity, manifests | [Plugin Loading](../engine/plugin-loading.md) |
| Package identity, activation by mod profile, profile fingerprint, save-side profile changes | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id) (cited, never redefined) |
| Writing cells into a plugin (the bake) | [In-Engine World Editor PRD](world-editor.md#56-output-plugin-wed-out) (M50) |
| Save capture, reload, FormID-keyed deltas | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| State of references that stream out and back | [Stream-Boundary State Continuity](../engine/stream-boundary-state-continuity.md) |
| RT absolute-space precision ceiling | [Shader Pipeline — coordinate spaces](../engine/shader-pipeline.md#absolute-world-space-rt-path--and-its-f32-ceiling) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| Generator, rule-set format, seams, bake | Phase 0 writes the missing design (§9) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `PGEN-` prefix.

---

## 1. Product thesis

The ROADMAP row states the intent: generate exterior cells from heightmap,
biome rules and noise, giving an effectively unlimited worldspace that
**complements rather than replaces** vanilla exteriors, and does better than
Starfield's procedural planets "because cells are first-class ECS state, not
save-file blobs" ([ROADMAP, Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon)).
Tier 9 was held until M40 (streaming) and M45 (save/load) proved the state
model; both have shipped ([Completed Milestones](../../ROADMAP.md#completed-milestones)).

Procedural generation is not foreign to the lineage. The Construction Set's
Region Editor already "procedurally create[s] content including … landscape
textures, roads, forests, mountains and grasslands", conditioning each
generated object on density, slope, height and radius, and the GECK and
Creation Kit kept the same tool (`cs-uesp-wiki/(main)/Regions.wiki`,
`geck-uesp-wiki/(main)/Regions.wiki`, `ck-uesp-wiki/(main)/Region.wiki`).
But there it is an *editor-time bake*: "Generate Now" writes references into
the plugin, and once the plugin is saved "the generated objects can no
longer be obliterated" (same pages). What the player gets is whatever the
author baked. The worldspace grid itself "extends infinitely in all
directions" (`cs-uesp-wiki/(main)/Exterior cells.wiki`), yet only authored
cells exist on it.

ByroRedux moves generation to runtime. Three statements define the product:

1. **Authored content always wins.** A generated cell exists only where no
   plugin authored one, inside a domain a rule set declared. Nothing
   authored is edited, overridden or re-textured.
2. **Generated cells are ordinary cells.** The generator outputs the same
   parsed cell data the loader already consumes; streaming, collision,
   ground cover, distant terrain, saves and the debug tools see no
   difference except a provenance tag.
3. **A generated world is a function, not a file.** The same seed, rule set
   and generator version give the same cells, bit for bit, in any order, on
   any run. A save stores those three inputs and the player's changes, never
   the generated content.

The corollary that shapes everything below: **generation happens before the
translate boundary, never after it.** The generator is one more producer of
parser-tier cell data (`CellData` with `LandscapeData` and `PlacedRef`s), so
no renderer, physics, ground-cover or LOD code gains a "was this generated"
branch, the same doctrine EXAL applies to per-game data
([EXAL §1](../engine/exal.md#1-the-three-tier-model)). The engine will not
generate content that only the generator's own render path could draw.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Authored cell** | An exterior cell a plugin defines: an entry in `EsmCellIndex.exterior_cells` for the worldspace and grid coordinate. |
| **Generated cell** | A `CellData` the generator produced for a grid coordinate with no authored cell, inside a domain. |
| **Grid coordinate** | `(gx, gy)`, the `i32` exterior-cell coordinate `world_pos_to_grid` returns; one cell is 4096 units square, 33×33 height vertices 128 units apart (`cs-uesp-wiki/(main)/Exterior cells.wiki`; UESP `Oblivion Mod/Mod File Format/LAND.wiki`). |
| **Domain** | The set of grid coordinates in one worldspace where a rule set may generate. Bounded (§3.2). |
| **Rule set** | One unit of authored generation data: identity, seed, domain, biomes, the base records they use. Its file format is §10 Q2. |
| **Biome** | A named set of rules inside a rule set: height character, landscape texture (`LTEX`) layers, water, object rules, ground-cover palette hints. |
| **Biome field** | The per-coordinate weights that pick and blend biomes, evaluated from noise. |
| **Object rule** | A biome's rule for scattering one base record (`STAT`, `TREE`, `FLOR`, …): density, spacing, slope and height gates, size variance, conform-to-slope. The vocabulary is the Region Editor's Objects tab (`cs-uesp-wiki/(main)/Regions.wiki`). |
| **Generator** | The pure CPU function from (seed, rule set, generator version, grid coordinate) to `CellData`. |
| **Generator version** | An integer bumped whenever the generator's output changes for the same inputs, as `crates/terrain-height`'s `GENERATOR_VERSION` is ([Terrain height maps](../engine/exal-terrain-height.md#disk-cache)). |
| **Cell hash** | A 64-bit hash of a generated `CellData`'s heights, splat, water and placements, used by the golden test. |
| **Delta** | A saved change to a reference, keyed by its `FormIdPair` and overlaid on reload ([Save/Load §6](../engine/save-load-roundtrip.md#6-live-load-apply-m451)). |
| **Seam** | The shared edge row or column of two neighbouring cells. LAND's bottom row and left column copy the southern and western neighbours (UESP `LAND.wiki`, VHGT). |
| **Bake** | Writing generated cells into a Redux-native output plugin so they load as authored cells ([M50 output plugin](world-editor.md#56-output-plugin-wed-out)). The output plugin's `PluginId` is its manifest UUID, not a hash of its bytes ([WED §3.4](world-editor.md#34-which-identity-the-output-plugin-follows)). |
| **Package** | The unit the mod profile activates and fingerprints, with one `PluginId` ([LMC-ID-002, LMC-ID-007, LMC-ID-012](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)). A rule set ships in a package; this PRD adds no second identity. |

---

## 3. Core model

### 3.1 Cell provenance

Every grid coordinate of an exterior worldspace has exactly one provenance,
decided before the cell loads:

| Class | Name | When | Reported as |
|---|---|---|---|
| **P1** | Authored | A plugin defines the cell | `authored <plugin>` |
| **P2** | Generated | No authored cell; coordinate inside a domain | `generated <rule set> v<version> hash=<cell hash>` |
| **P3** | Seam-bound | Generated, and a neighbour is authored (only under §10 Q1 option B) | `generated … seam=<edges>` |
| **P4** | Baked | A generated cell written into a plugin; loads as P1 | `authored <plugin> (baked from <rule set> v<version>)` |
| **P5** | Void | No authored cell, outside every domain | `void` (today's behaviour for every unauthored coordinate) |

### 3.2 What the engine has at HEAD

The inventory below is the state at HEAD on 2026-11-01. It is hand-written;
the provenance report (PGEN-DX-001) replaces the P-class rows once Phase 1
lands.

| # | Capability the generator leans on | Today |
|---|---|---|
| 1 | Streaming asks for every coordinate in the radius | `compute_streaming_deltas` (`byroredux/src/streaming/mod.rs:897`) inserts every `(px+dx, py+dy)` in the Chebyshev radius, authored or not |
| 2 | An unauthored coordinate loads nothing | The lookup happens twice per cell: the pre-parse worker returns an empty payload when the worldspace map has no entry for `(gx, gy)` (`byroredux/src/streaming/pre_parse.rs:789-810`), and `ExteriorCellApplyJob` re-reads the index on the main thread and returns `Ok(None)` (`byroredux/src/cell_loader/exterior/apply_job.rs:91-97`); every unauthored coordinate is P5 |
| 2a | The authored index is read in many places | `exterior_cells` is read at about 40 non-test sites in 17 files under `byroredux/src` (`rg -n exterior_cells byroredux/src --glob '!*tests*'`), including the worker and apply job (row 2), the LOD ring (row 5), water, climate, worldspace selection, reference state, the bench camera and script lookup |
| 3 | Terrain consumes parsed `LandscapeData` | `spawn_terrain_mesh(…, land: &esm::cell::LandscapeData)` (`byroredux/src/cell_loader/terrain.rs:1231`); `LandscapeData` is decoded `f32` heights plus optional raw VNML normal bytes, vertex colours and four quadrants of `LTEX` layers (`crates/plugin/src/esm/cell/landscape.rs:62-77`); absent normals render flat-up (`terrain.rs:1067-1076`) |
| 4 | Terrain collision comes from the render mesh | `spawn_terrain_mesh` calls `spawn_land_collider` (`terrain.rs:1298`), which calls `spawn_trimesh_collider_ghost` (`terrain.rs:995-1002`), the single collider path for all static geometry (`byroredux/src/cell_loader/spawn.rs`) |
| 5 | Distant terrain is partly synthesised from parsed heights | The finest band (`LOD_BLOCK_CELLS = 4`, `terrain_lod.rs:67`) is synthesised from cells in `index.exterior_cells` (`byroredux/src/cell_loader/terrain_lod.rs:552`). On FO3, FNV, Skyrim and FO4 the coarser bands exist only where the game baked a `.btr` or authored quad (`terrain_lod.rs:314-322`, `:760-762`; test `coarse_terrain_bands_require_a_baked_btr`, `:1683`); Oblivion keeps one synthesised ring. The ring is clipped to the WRLD's usable cell bounds (`byroredux/src/cell_loader/lod_support.rs:232-247`) |
| 6 | Ground cover needs no per-cell data | The density field is evaluated on the GPU from terrain splat, slope, water distance, curvature and noise ([Ground Cover §3](../engine/exal-groundcover.md#3-the-density-field)) |
| 7 | The parsed index is shared read-only during a session | `ExteriorWorldContext::record_index` is an `Arc<EsmIndex>` (`byroredux/src/cell_loader/exterior.rs:82`), shared with the streaming worker; no code path inserts a cell into it after parse |
| 7a | An exterior session needs a worldspace record | The exterior context, the cell-root stamp, the default water height and type, the climate and the LOD source key all resolve through `record_index.cells.worldspaces` (`exterior.rs:115-124`, `:187-202`, `:238-241`; `byroredux/src/env_translate/climate.rs:36-60`); a worldspace no plugin defines has no context to stream in |
| 8 | A placement's identity is a `FormIdPair` built from a load-order slot | the spawn site takes `placement_form_id_pair: Option<FormIdPair>`, `None` on the precombine path (`spawn.rs:707`). The pair is built from `PlacedRef.form_id: u32` (`crates/plugin/src/esm/cell/placed_ref.rs:44`) by `plugin_for_form_id` (`byroredux/src/cell_loader/load_order.rs:368`), which finds the plugin owning the ID's global slot; an ID with no slot falls back to the placeholder `"Engine.esm"` (`byroredux/src/cell_loader/references/synth_child.rs:796-803`). The portable decode `LoadOrderIdentity::pair` masks the local ID to the slot's object-ID width (`crates/scripting/src/load_order.rs:35-44`) |
| 8a | Form-ID lookups go through the runtime handle | `World::find_by_form_id` takes the interned `FormId`, not a pair (`crates/core/src/ecs/world.rs:445`); `prid` selects by `EntityId` (`byroredux/src/commands/view.rs:808-829`); `mesh.info` prints an entity's `REFR FormID` line from `FormIdPool::resolve` (`byroredux/src/commands/assets.rs:734-757`) |
| 9 | An exterior save stores where to reload, not what was there | `CurrentExteriorContext` holds worldspace key, ESM path, masters, grid and radii (`byroredux/src/cell_loader/transition.rs:158`); reload streams the radius from the plugins and overlays `FormIdPair` deltas ([Save/Load §6](../engine/save-load-roundtrip.md#6-live-load-apply-m451)) |
| 10 | Absolute-space RT has a precision ceiling | `RT_ABSOLUTE_PRECISION_CEILING = 1_048_576.0` (2^20 units) in `byroredux/src/cell_loader/references/mod.rs:164`, i.e. 256 cells from the origin per axis at 4096 units per cell (`EXTERIOR_CELL_UNITS`, `crates/core/src/math/coord.rs:41`) |
| 11 | Engine-generated data is versioned and cached | `GENERATOR_VERSION` (`crates/terrain-height/src/lib.rs:28`) is hashed into the cache key (`byroredux/src/asset_provider/terrain_height.rs:138`) |
| 12 | Region object rules are parsed by form only | `RegionDataPayload::Objects(Vec<u32>)` keeps the `RDOT` form IDs and drops the rest of each row (`crates/plugin/src/esm/records/misc/world.rs`) |
| 13 | Redux-native plugin manifests have no runtime consumer | `PluginManifest::from_toml` (`crates/plugin/src/manifest.rs:38`) is referenced from no file under `byroredux/src`; the engine loads plugins only as ESM/ESP files |
| 14 | Starfield planet records are not dispatched | `BIOM` and `PNDT` are absent from `DISPATCH_HANDLED_FOURCCS` (`crates/plugin/src/esm/records/parse.rs:49`) |
| 15 | Live terrain diagnostics exist | `terrain.seams` reports shared-edge height agreement between resident cells and `lod.coverage` reports `settled` / `held_back` / `verdict` (`byroredux/src/commands/world_info.rs:805-841`, machine lines in `crates/core/src/ecs/resources/mod.rs:1227-1241`, `:1478-1490`); `cell.owners <gx> <gy>` reports one resident cell's owners (`world_info.rs:866-880`) |
| 16 | Streaming telemetry is aggregate | `byroredux/src/streaming/telemetry.rs` keeps per-phase latency summaries over a 128-sample ring (`telemetry.rs:1-30`); it has no per-cell line |

Rows 3, 4 and 6 are why the product is feasible without a second render
path: the terrain, collider and ground-cover consumers already read the
parser-tier types the generator will produce. Rows 2, 2a and 7 are the
structural gap: generated cells need a second, session-local source beside
the immutable index, readable from the worker and the main thread, and the
design must classify each `exterior_cells` read site as "must see generated
cells" or "authored only". Row 7a is the second gap for a new worldspace
(§10 Q1). Row 5 means distant terrain over generated land is a feed to
build, not a free consequence (PGEN-LOD-001). Row 10 is the one product
limit: see §6 and §10 Q3.

---

## 4. Goals and non-goals

### 4.1 Goals

- **PGEN-G-001 — Land where there was void.** In a domain, every
  coordinate without an authored cell loads a generated cell with terrain,
  collision, water and ground cover.
- **PGEN-G-002 — Nothing authored changes.** Every authored cell in a
  worldspace with a rule set loads with the same cell hash as without it.
- **PGEN-G-003 — Reproducible worlds.** Same seed, rule set and generator
  version give the same cell hash on every run, in every visiting order.
- **PGEN-G-004 — Streaming parity.** Crossing generated cells costs no more
  readiness time than crossing authored cells of comparable reference
  count, measured on the existing grid-cross bench.
- **PGEN-G-005 — Saves carry inputs and changes only.** A save in a
  generated worldspace holds no generated geometry; its size grows with the
  player's changes, not with the area visited.
- **PGEN-G-006 — Modder-authored biomes.** A biome is authored as data in a
  rule set, with no engine code and no game-specific branch.

### 4.2 Non-goals

- **PGEN-NG-001.** Editing, re-texturing or adding references to authored
  cells. The ROADMAP row says "complements rather than replaces"; a mod
  that wants to change an authored cell is a plugin, not a rule set.
- **PGEN-NG-002.** Generated interiors, settlements, dungeons, quests and
  points of interest. Starfield imposes authored worldspaces on a planet as
  an "Overlay", "replacing a grid section of the terrain in game at
  particular planetary coordinates" (Starfield Wiki,
  [Starfield Creation Kit: Cell](https://starfieldwiki.net/wiki/Starfield_Mod:Starfield_Creation_Kit:Cell));
  the analogous promise for this engine is a later PRD once seams (§10 Q1)
  are proven.
- **PGEN-NG-003.** Actors and navmesh in generated cells. No `NAVM` is
  generated, so AI packages have nowhere to path; spawning NPCs there would
  promise behaviour the engine cannot deliver.
- **PGEN-NG-004.** Distant object LOD, and coarse distant-terrain bands,
  for generated land. Object LOD is a baked per-game asset
  ([EXAL §5](../engine/exal.md#5-lod-distance-rendering-the-must-have))
  with no generated equivalent. Distant terrain over generated land is the
  finest synthesised band only, fed by PGEN-LOD-001: the coarser bands on
  FO3, FNV, Skyrim and FO4 exist only as baked assets (§3.2 row 5), which a
  generated domain does not have.
- **PGEN-NG-005.** Reproducing Starfield's planet generator. Starfield
  ships planet and biome record types (`BIOM` = `BGSBiome`, `PNDT` =
  `BGSPlanet::PlanetData`, `PCMT`/`PCBN`/`PCCN` = the Planet Content
  Manager tree, `SFBK`/`SFPT`/`SFTR` = `BGSSurface` block / pattern / tree,
  per `Gibbed.Starfield/projects/Gibbed.Starfield.PluginFormats/FormType.cs`
  under `/mnt/data/src/reference/`), none decoded today (§3.2 row 14).
  Rendering vanilla planets from them is Starfield exterior bring-up, not
  this milestone (§10 Q7).
- **PGEN-NG-006.** A GPU-evaluated generator. Ground cover is evaluated on
  the GPU because it is drawn, never saved or collided with
  ([Ground Cover §3](../engine/exal-groundcover.md#3-the-density-field)); generated
  cells feed physics, saves and identity, so their authority is CPU-side.
- **PGEN-NG-007.** A hosted world service, shared seeds fetched from a
  server, or telemetry of generated worlds. Excluded by the anti-scope
  ([ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).

---

## 5. Requirements by area

### 5.1 Domain and provenance (`PGEN-DOM`)

- **PGEN-DOM-001.** At a coordinate where a plugin authored a cell, the
  generator MUST NOT be consulted; the cell MUST load with the same cell
  hash with and without the rule set active. Today every coordinate is
  either authored or void (§3.2 row 2).
- **PGEN-DOM-002.** A rule set MUST declare its domain as a closed set of
  grid coordinates in one named worldspace; a coordinate outside every
  domain MUST stay P5 and load nothing, exactly as today's `Ok(None)` arm.
- **PGEN-DOM-003.** A domain MUST lie within `RT_ABSOLUTE_PRECISION_CEILING`
  of the origin on every axis (256 cells at HEAD, §3.2 row 10). A rule set
  whose domain exceeds it MUST be refused at boot with a report naming the
  ceiling, unless §10 Q3 is decided otherwise.
- **PGEN-DOM-004.** Where several rule sets claim one coordinate, the
  winner MUST be chosen by the same rule the plugin layer applies to record
  conflicts between packages, with no rule-set-specific order. At HEAD that
  is dependency depth, then the lowest `PluginId`
  (`DependencyResolver::resolve_winner`, `crates/plugin/src/resolver.rs:55-58`;
  [Plugin Loading](../engine/plugin-loading.md#dependencyresolver--conflict-resolution));
  [LMC-ID-003](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)
  replaces the `PluginId` tie-break with profile order, and rule sets follow
  whichever rule is in force. The loser MUST be named in the boot report.
- **PGEN-DOM-005.** Which kind of worldspace a domain may target (a new
  worldspace, the unauthored rim of an existing one, or both) MUST follow
  §10 Q1; under option A the boot MUST refuse a domain that contains, or is
  edge-adjacent to, a coordinate with an authored cell, and name that cell.
- **PGEN-DOM-006.** The domain's worldspace MUST be present in
  `record_index.cells.worldspaces` before streaming starts (§3.2 row 7a):
  either a `WRLD` record a loaded plugin defines, or a parser-tier
  `WorldspaceRecord` the rule set's package supplies, per §10 Q1 and Q2. A
  domain whose worldspace resolves to neither MUST be refused at boot with a
  report naming it.
- **PGEN-DOM-007.** A rule set MUST be active only when its package is
  enabled in the mod profile, the only activation input
  ([LMC-ID-002](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id));
  its identity MUST be that package's `PluginId`, and it MUST appear in the
  profile fingerprint and ownership records like any other package
  (LMC-ID-007, LMC-ID-012). A rule-set-specific identity, registry or
  activation list MUST NOT exist.

### 5.2 Generator (`PGEN-GEN`)

- **PGEN-GEN-001.** The generator MUST be a pure function of (seed, rule
  set content hash, generator version, grid coordinate): no wall clock, no
  thread count, no `HashMap` iteration order, no global mutable state.
- **PGEN-GEN-002.** Generating coordinate `c` MUST give the same cell hash
  whichever other coordinates were generated before it and in what order.
- **PGEN-GEN-003.** A checked-in golden table of cell hashes for a fixture
  rule set and a fixed coordinate list MUST match on every CI run; a
  change to any hash MUST come with a generator-version bump in the same
  commit.
- **PGEN-GEN-004.** Each cell's output MUST depend only on inputs inside a
  bounded neighbourhood of its coordinate, declared in the design, so any
  cell (including one in the distant-LOD ring) can be produced alone.
  Non-local operators such as whole-map hydraulic erosion (Musgrave, Kolb,
  Mace, "The synthesis and rendering of eroded fractal terrains", SIGGRAPH
  1989, [doi:10.1145/74334.74337](https://doi.org/10.1145/74334.74337);
  Génevaux et al., "Terrain generation using procedural models based on
  hydrology", ACM TOG 2013,
  [doi:10.1145/2461912.2461996](https://doi.org/10.1145/2461912.2461996))
  are admitted only as §10 Q5 decides.
- **PGEN-GEN-005.** The generator MUST output parser-tier types: a
  `CellData` with `grid`, `landscape: Some(LandscapeData)`, water height
  and `references: Vec<PlacedRef>`. Renderer, physics, ground-cover and
  LOD modules MUST NOT read the provenance tag; a source-scan test MUST fail if
  the tag's type is named outside the modules the design lists.
- **PGEN-GEN-006.** The generator MUST run on the streaming pre-parse
  worker (`cell_pre_parse_worker`, [Streaming §4](../engine/exterior-grid-streaming.md#4-streaming-phase-1-async-pre-parse-worker)),
  never on the main thread.
- **PGEN-GEN-007.** Generated cells MUST live in a session-local store
  beside the immutable `EsmIndex` (§3.2 row 7), looked up by the same
  `(worldspace, gx, gy)` key from both the pre-parse worker and the
  main-thread apply job (§3.2 row 2), and MUST be dropped when the cell
  unloads. The design MUST list every `exterior_cells` read site (§3.2
  row 2a) with its classification, and a test MUST fail when a new read
  site appears unclassified.
- **PGEN-GEN-008.** Noise primitives SHOULD be the published gradient and
  cellular bases (Perlin, "Improving noise", ACM TOG 2002,
  [doi:10.1145/566654.566636](https://doi.org/10.1145/566654.566636);
  Worley, "A cellular texture basis function", SIGGRAPH 1996,
  [doi:10.1145/237170.237267](https://doi.org/10.1145/237170.237267)), each
  evaluated in integer-hashed lattice space so a coordinate's value does
  not depend on float accumulation across cells.
- **PGEN-GEN-009.** Cell hashes MUST be equal across two release builds of
  the same commit on the same target triple. Cross-platform equality is a
  **Phase 0: verify** item: whether `f32` noise evaluation in Rust is
  bit-identical across the M70 targets, settled by running the golden
  table on each.

### 5.3 Terrain (`PGEN-TER`)

- **PGEN-TER-001.** A generated `LandscapeData` MUST carry 33×33 heights at
  128-unit spacing over a 4096-unit cell, the LAND grid
  (`cs-uesp-wiki/(main)/Exterior cells.wiki`; UESP `LAND.wiki`).
- **PGEN-TER-002.** Two neighbouring generated cells MUST have bit-equal
  heights on their shared seam row or column, because LAND copies the
  neighbour's edge (UESP `LAND.wiki`, VHGT). Offline: a test over every seam
  of the golden coordinate list. Live: `terrain.seams` (§3.2 row 15) MUST
  report `pairs_dirty=0` over the resident fixture domain.
- **PGEN-TER-003.** Under §10 Q1 option B, a generated cell adjacent to an
  authored cell MUST copy the authored seam heights exactly and blend into
  its own field over a fall-off distance the rule set declares; the
  maximum seam discontinuity is metric M-PGEN-5, read from `terrain.seams`.
- **PGEN-TER-004.** A generated `LandscapeData` MUST populate `normals`
  (absent normals render flat-up, §3.2 row 3) as VNML signed bytes derived
  from the generated heights; each decoded normal MUST match the
  central-difference normal of the heights to within one VNML quantisation
  step per component, so lighting and the ground-cover slope gate read the
  slope the collider has.
- **PGEN-TER-005.** Each quadrant's base and blended `LTEX` layers MUST
  come from the biome field, with per-vertex alphas clamped to 0.0–1.0, the
  invariant the parser already enforces for authored layers
  (`TerrainTextureLayer::alpha`, `crates/plugin/src/esm/cell/landscape.rs`).
- **PGEN-TER-006.** A generated cell's water height MUST come from the rule
  set's worldspace default, through the existing explicit / inherited
  water-height resolution (`resolved_exterior_water_height`,
  `byroredux/src/cell_loader/exterior.rs`), not a generator-specific path.
- **PGEN-TER-007.** Every generated cell of the fixture domain MUST get a
  terrain collider through the existing collider synthesis (§3.2 row 4): a
  downward physics ray at each cell's centre MUST hit it within one unit of
  the generated height.

### 5.4 Biome rules (`PGEN-BIO`)

- **PGEN-BIO-001.** The rule-set format (§10 Q2) MUST be machine-readable,
  carry a format version, and be validated at boot with one report line per
  rule set: biomes, object rules, unresolved references.
- **PGEN-BIO-002.** A rule set MUST reference base records (`LTEX`, `STAT`,
  `TREE`, `FLOR`, `GRAS`, `WATR`, `CLMT`) by stable identity, resolved at
  the EXAL translate boundary; a reference that does not resolve in the
  active load order MUST disable only the rule that uses it and MUST be
  reported.
- **PGEN-BIO-003.** Each object rule MUST support the Region Editor's
  placement controls: density, minimum and maximum slope, minimum and
  maximum height, radius, size variance, sink and conform-to-slope
  (`cs-uesp-wiki/(main)/Regions.wiki`, Objects and Objects (more) tabs).
  Their numeric scales are ours to define and MUST be documented in the
  design; matching the original tool's internal formula is not promised,
  because no source states it.
- **PGEN-BIO-004.** Biome transitions MUST blend over a declared fall-off
  distance `F` in world units, the role the Region Editor's "Edge Fall-off"
  plays for regions (same page, which describes it as a linear taper in
  density); between adjacent height vertices (128 units apart) a biome's
  weight MUST change by at most `128 / F`.
- **PGEN-BIO-005.** Per-game differences MUST be confined to resolving the
  rule set's palette against the active game's records at the translate
  boundary, as ground cover does for its species palette
  ([Ground Cover §7](../engine/exal-groundcover.md#7-per-game-translation--palette-only)).
- **PGEN-BIO-006.** A biome MAY carry ground-cover palette hints; ground
  cover over generated terrain MUST come from the existing density field
  with no generator-owned grass path.

### 5.5 Placed objects (`PGEN-OBJ`)

- **PGEN-OBJ-001.** Object placement SHOULD use Poisson-disk sampling
  (Bridson, "Fast Poisson disk sampling in arbitrary dimensions", SIGGRAPH
  2007 sketches, [doi:10.1145/1278780.1278807](https://doi.org/10.1145/1278780.1278807))
  seeded per cell; two placements of one rule MUST NOT be closer than the
  rule's radius, including across a seam.
- **PGEN-OBJ-002.** Generated `PlacedRef`s MUST spawn through the same
  budgeted reference loader as authored REFRs
  ([Streaming §4](../engine/exterior-grid-streaming.md#4-streaming-phase-1-async-pre-parse-worker)),
  so the per-frame `STREAMING_APPLY_BUDGET` deadline
  (`byroredux/src/app_step.rs:33`) covers them.
- **PGEN-OBJ-003.** A generated placement whose base record is interactive
  (a pickable `MISC` or `INGR`, a `CONT`, anything activatable) MUST carry a
  `FormIdPair` (PGEN-ID-001).
- **PGEN-OBJ-004.** A generated placement of a non-interactive static MAY
  carry no placement identity, as precombine geometry carries none
  (`spawn.rs:707`); such placements MUST NOT accept deltas.
- **PGEN-OBJ-005.** Every rule-set base record's textures MUST resolve:
  `tex.missing` MUST report zero entries attributed to rule-set records on
  the fixture domain.

### 5.6 Identity (`PGEN-ID`)

- **PGEN-ID-001.** A generated placement's identity MUST be a `FormIdPair`
  whose `PluginId` is the rule set's package identity (PGEN-DOM-007) and
  whose `LocalFormId` is derived deterministically from (grid coordinate,
  rule index, placement index), so the same object has the same identity in
  every session and load order
  ([Plugin Loading — Form ID System](../engine/plugin-loading.md#form-id-system)).
  This is the existing stable-identity layer, not a new scheme: no `0xFF`
  save-generated IDs, no legacy slot, no second key type.
- **PGEN-ID-002.** A generated `FormIdPair` MUST NOT equal any pair a loaded
  plugin defines, except a bake's own records for cells the domain then
  excludes (PGEN-BAKE-003); a collision test over the fixture domain MUST
  pass. For a
  Redux-native package, whose local IDs its author assigns
  (`crates/core/src/form_id.rs:117-122`), the design MUST reserve the
  generated local-ID range and state its bit layout and per-cell placement
  ceiling; a cell whose rules would exceed the ceiling MUST report the
  overflow and drop the excess placements.
- **PGEN-ID-003.** While its cell is resident, a generated pair MUST intern
  through `FormIdPool`, `World::find_by_form_id` MUST return the placement
  entity for the interned `FormId`, and `mesh.info <entity>` MUST print its
  `REFR FormID` line with the rule-set package's plugin prefix (§3.2 row 8a).
- **PGEN-ID-004.** A generated pair MUST reach the spawn site as a pair,
  never as a `PlacedRef.form_id: u32`: a rule-set package has no global
  slot, so `plugin_for_form_id` would attribute it to the `"Engine.esm"`
  placeholder (§3.2 row 8). The design MUST name the field or side table
  that carries the pair from the generator to `placement_form_id_pair`.

### 5.7 Persistence (`PGEN-SAV`)

- **PGEN-SAV-001.** An exterior save in a generated domain MUST add
  exactly two inputs beside today's `CurrentExteriorContext` fields (§3.2
  row 9): the effective seed and the generator version. The rule set's
  identity and content hash MUST come from the profile fingerprint and the
  ordered package list with content hashes the save already stores
  ([LMC-ID-007, LMC-SAV-001](legacy-mod-compatibility.md#510-saves-and-profile-changes-lmc-sav)),
  not from a rule-set-specific field. The save MUST NOT contain generated
  heights, splat or unmodified placements.
- **PGEN-SAV-002.** Load MUST regenerate the saved radius and overlay
  deltas through the existing M45.1 sequence (`build_form_id_remap` →
  `apply_deltas`); a generated item placement the player picked up (a
  `MISC` or `INGR` from an object rule) MUST stay gone after F9 and after a
  process restart, through the same pickup tombstone authored items use
  (`cell_loader::reference_state::restore_resident`).
- **PGEN-SAV-003.** A save whose generator version differs from the
  running one MUST be handled per §10 Q4. A save whose rule-set package
  changed content, or is absent, MUST follow the profile-change rules
  already stated for every package (regenerate from current data and
  overlay saved deltas; never refuse solely for an absent package;
  [LMC-SAV-002, LMC-SAV-003](legacy-mod-compatibility.md#510-saves-and-profile-changes-lmc-sav)).
  In every case the load MUST NOT apply a delta to an identity the running
  generator does not produce, and every unmatched delta MUST be listed in
  the load report.
- **PGEN-SAV-004.** A generated placement that streams out and back in
  MUST keep its state through the FormID-keyed parking the streaming
  design defines ([Stream-Boundary §3](../engine/stream-boundary-state-continuity.md#3-formid-keyed-state-not-entityid-keyed-state)).
- **PGEN-SAV-005.** The new save fields MUST bump `FORMAT_MAJOR`
  (`crates/save/src/snapshot.rs:608`) under the save design's versioning
  rules.

### 5.8 Distant terrain (`PGEN-LOD`)

- **PGEN-LOD-001.** The distant terrain ring over generated coordinates
  MUST come from the existing finest-band heightmap synthesis in
  `terrain_lod.rs`, fed by the generator's coarse samples through the
  session-local store (PGEN-GEN-007), since the ring reads only
  `index.exterior_cells` at HEAD (§3.2 row 5); no generator-specific LOD
  mesh path. The ring's reach and block count over a generated domain MUST
  be reported by the LOD ring's existing log line (`terrain_lod.rs:845`).
- **PGEN-LOD-002.** The generator MUST answer a coarse height query for a
  coordinate without generating its placements, so the LOD ring costs no
  object scatter.
- **PGEN-LOD-003.** `lod.coverage` (§3.2 row 15) MUST report `settled=1`,
  `held_back=0` and `verdict=PASS` on the fixture domain after a boundary
  crossing.

### 5.9 Streaming cost (`PGEN-STR`)

- **PGEN-STR-001.** Each generated cell MUST log one line with
  coordinate, provenance class, cell hash, generation time and placement
  count, and generation time MUST be recorded as its own phase in the
  existing streaming latency summaries (`byroredux/src/streaming/telemetry.rs`,
  §3.2 row 16) so `p50`/`p95` come from the same ring as the other
  phases.
- **PGEN-STR-002.** Grid-cross readiness on the fixture domain MUST NOT
  exceed the authored control's by more than the margin Phase 0 sets,
  measured under ROADMAP's bench rules (same machine, pinned camera;
  [How to run a bench](../../ROADMAP.md#how-to-run-a-bench)).
- **PGEN-STR-003.** Generation time per cell at p99 MUST stay under the
  budget Phase 0 sets from a measured authored-cell pre-parse baseline.

### 5.10 Bake (`PGEN-BAKE`)

- **PGEN-BAKE-001.** Baking MUST write the selected generated cells
  through the M50 output-plugin writer called without an editor session
  ([WED-OUT-008](world-editor.md#56-output-plugin-wed-out)), into a
  Redux-native plugin whose manifest lists the rule-set package as a
  dependency (WED-OUT-001); the baked cells then load as P4, and the rule
  set's domain MUST exclude them from then on (PGEN-DOM-001).
- **PGEN-BAKE-002.** The bake is format-neutral and follows the output
  format M50's §10 Q1 chooses. A Redux-native output (Q1's recommendation)
  stores the `f32` heights and MUST reload them bit-equal. Only where the
  output also stores heights as VHGT (Q1's legacy-export alternatives,
  WED-TER-007), baked heights
  MUST round-trip through VHGT, whose per-vertex gradient step is one
  signed byte at 8 units with a range of −1024 to +1016 per step
  (UESP `LAND.wiki`); the baked cell's maximum height error MUST be at most
  4 units, and a gradient VHGT cannot encode MUST be reported, not
  silently clamped.
- **PGEN-BAKE-003.** Baked placements MUST keep their generated
  `FormIdPair`s, which WED-OUT-008 writes unchanged, so deltas saved before
  the bake apply after it with no remap. The output plugin keeps its own
  manifest `PluginId`; the baked records carry the rule-set package's
  `PluginId` the way an override carries its master's, so no two packages
  share one identity
  ([LMC-ID-010](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)),
  and WED-REF-003's next-unused-ID allocation does not apply (it covers
  editor placements only). The consequences are §10 Q10.

### 5.11 Diagnostics (`PGEN-DX`)

- **PGEN-DX-001.** For a resident cell, `cell.owners <gx> <gy>` (§3.2
  row 15) MUST gain a `provenance` line with the class (§3.1) and, for
  P2/P3, rule set, generator version and cell hash. For any coordinate,
  resident or not, `pgen.rules <gx> <gy>` (PGEN-DX-002) MUST print the
  class it would load as.
- **PGEN-DX-002.** A console command (`pgen.rules`, name fixed by the
  design) MUST print every active rule set with its package, domain bounds,
  biome count and the PGEN-BIO-002 unresolved list.
- **PGEN-DX-003.** A headless dump tool SHOULD write a generated cell's
  heights and splat as images from a rule set and coordinate, with no
  Vulkan device, so rule authors can iterate without the engine.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Region generation is an editor-time bake into the plugin; it cannot be undone after save (`cs-uesp-wiki/(main)/Regions.wiki`) | Generation at runtime from data (PGEN-GEN-001) | The bake-then-hand-fix loop for filler terrain | Output drifts with generator changes → PGEN-GEN-003, PGEN-SAV-003 |
| 2 | The grid "extends infinitely" but only authored cells exist (`cs-uesp-wiki/(main)/Exterior cells.wiki`) | Domain coordinates are filled (PGEN-DOM-002) | Ocean or void padding around custom worldspaces | Players reach land no quest expects → domain is opt-in per rule set, PGEN-DOM-005 |
| 3 | The worldspace is as large as the author makes it | A domain is capped at 256 cells from the origin at HEAD (PGEN-DOM-003) | — | ROADMAP's "effectively unlimited" is not met as written → §10 Q3 |
| 4 | LAND heights are VHGT gradients in 8-unit steps (UESP `LAND.wiki`) | Generated heights are `f32`, unquantised | — | A bake loses precision → PGEN-BAKE-002 |
| 5 | A region's objects are conditioned by its Objects tab (same Regions page) | Same vocabulary, our own scales (PGEN-BIO-003) | — | Authors expect CS numbers to mean the same → design documents scales; no parity promised |
| 6 | Region border flags fence the player in (`ck-uesp-wiki/(main)/Region.wiki`, Border Region) | `RegnRecord` carries no border field at HEAD (`crates/plugin/src/esm/records/misc/world.rs:794-821`), so nothing fences a domain | — | Under Q1 option B, generated rims meet worlds whose borders the engine does not enforce, and the LOD ring is clipped to the WRLD's cell bounds (§3.2 row 5) → recorded in the design; option A avoids both |
| 7 | Starfield imposes authored worldspaces on a planet as overlays that replace "a grid section of the terrain" (Starfield Wiki, Creation Kit: Cell page); its biomes have "unique generation parameters" per planet ([Starfield Wiki: Planets](https://starfieldwiki.net/wiki/Starfield:Planets)) | Generated cells are ordinary ECS cells with saved deltas | — | Neither page says how a planet surface is produced or what a save stores for it → **Phase 0: verify**: that Starfield generates planet terrain at runtime, and what its save stores for a visited tile, settled by a primary source (Bethesda documentation or a save-format reference); ROADMAP's "save-file blobs" comparison is dropped if neither is found |
| 8 | Distant terrain on FO3 / FNV / Skyrim / FO4 is a baked quadtree reaching far past the streamed ring | Generated land gets only finest-band synthesised blocks (PGEN-LOD-001, PGEN-NG-004) | — | More, smaller distant blocks over the same reach, and no coarse band → the block count and reach are logged (PGEN-LOD-001) and timed under PGEN-STR-002 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The generator is game-agnostic; a game is in scope when its exteriors load
and a rule set can resolve a palette from its records
([Game Compatibility](../engine/game-compatibility.md)).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. Exteriors load and stream; pre-Skyrim `TREE` renders as SpeedTree billboard placeholders, so tree-heavy biomes look worse until the tree design lands |
| Fallout 3 | yes | Wave 2. Same parser and tree path as FNV |
| Fallout: New Vegas | yes | Wave 1. Reference title; its `TREE`s are SpeedTree too and render as placeholders (`crates/spt`, Oblivion 4.x / FO3+FNV 5.x), which desert biomes with few trees tolerate |
| Skyrim (LE / SE / AE) | yes | Wave 1. Trees are NIF meshes, rich `LTEX` and `GRAS` sets; SE first, LE and AE share the record set |
| Fallout 4 | yes | Wave 2. Exteriors load; precombines do not apply to generated cells |
| Fallout 76 | no | Exterior support is open scope: the exterior gate names it a policy skip, with object `.bto` LOD wired and terrain, water and fixtures not (`docs/smoke-tests/m-exteriors.sh:1177-1185`); no mod support ([Game Compatibility](../engine/game-compatibility.md)) |
| Starfield | no | Exteriors are a policy skip in the exterior gate (`docs/smoke-tests/m-exteriors.sh`); its own planet generator is PGEN-NG-005 |

### 7.2 Acceptance

Game data is never checked in. The fixture rule set references base
records by stable identity only and is engine-authored, so it is checked in
under `docs/smoke-tests/fixtures/`.

| # | Scenario | Driver | Data | Pass rule |
|---|---|---|---|---|
| A1 | Golden cell hashes | new `cargo test` `generated_cell_hash_matches_golden` in the new generator crate | none (fixture palette stubbed) | every hash equals the table (PGEN-GEN-003) |
| A2 | Order independence | new `cargo test` `generated_cell_is_order_independent` | none | two shuffled visiting orders give equal hashes (PGEN-GEN-002) |
| A3 | Seams | new `cargo test` `generated_seams_are_bit_equal` | none | zero unequal seam vertices (PGEN-TER-002) |
| A4 | Authored wins | new `cargo test` `authored_cell_ignores_rule_set` | none (synthetic `EsmCellIndex`) | authored cell hash unchanged (PGEN-DOM-001) |
| A4a | Read sites classified | new `cargo test` `exterior_cell_read_sites_are_classified` | none (source scan) | every `exterior_cells` read site under `byroredux/src` is in the design's list (PGEN-GEN-007) |
| A5 | Identity | new `cargo test` `generated_form_id_pairs_are_stable_and_unique` | none | pairs equal across runs and across two profile orders; zero collisions with the package's authored locals (PGEN-ID-001/002) |
| A6 | Static load | new script `docs/smoke-tests/m64-procedural.sh <game> static`, indexed in [docs/smoke-tests/README.md](../smoke-tests/README.md) | `Skyrim.esm` / `FalloutNV.esm` + their mesh and texture archives | boots `--bench-hold`, `byro-dbg` `cell.owners 0 0` prints `provenance` P2 (PGEN-DX-001); entity floor met; `tex.missing` has no rule-set entries; PNG health as `m-exteriors.sh` |
| A7 | Boundary crossing | same script, `boundary` mode | same | three-cell `grid-cross` completes; `lod.coverage` `verdict=PASS` (PGEN-LOD-003); `terrain.seams` `pairs_dirty=0` (PGEN-TER-002); readiness within the Phase 0 margin (PGEN-STR-002) |
| A8 | Save round trip | same script, `save` mode | same | pick up a generated item, F5, F9 and `--load` restart: it stays gone; save holds no generated geometry column (PGEN-SAV-001/002) |
| A9 | Version mismatch | new `cargo test` `mismatched_generator_version_parks_deltas` | none | every delta reported unmatched, none applied (PGEN-SAV-003) |

---

## 8. Metrics

Numbers live in ROADMAP: the M64 row in the Tier 9 table until the
milestone closes, and the bench-of-record table for timing rows.

| Metric | Definition | Granularity |
|---|---|---|
| M-PGEN-1 Determinism | golden cells matching ÷ golden cells | per generator version, per target triple |
| M-PGEN-2 Generation time | wall time per generated cell, p50 and p99, from PGEN-STR-001 lines | per game, per rule set |
| M-PGEN-3 Crossing readiness | grid-cross readiness on the fixture domain ÷ authored control | per game, per bench refresh |
| M-PGEN-4 Save growth | save bytes in a generated domain − save bytes of the same route with no deltas | per scenario A8 run |
| M-PGEN-5 Seam error | `terrain.seams` `height_mismatch_vertices` on authored / generated seams | per rule set (option B only) |
| M-PGEN-6 Rule resolution | resolved rule-set references ÷ all references | per game, per rule set |

---

## 9. Delivery sequence

Dependencies: M40 and M45 have shipped; M50
([In-Engine World Editor PRD](world-editor.md)) gates only the bake phase
(§10 Q6); the package identity, activation and save-side profile rules come
from the [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md), so
PGEN-DOM-007 and PGEN-SAV-001/003 land against whatever part of LMC-ID and
LMC-SAV has shipped by then.

**Phase 0 — Design, fixture and decisions (no engine code).** Write the
EXAL sub-document for the generator (inputs, neighbourhood bound, noise
stack, rule-set schema, session-local cell store, the classified list of
`exterior_cells` read sites, the worldspace-record source, the generated
local-ID layout and the pair-carrying field, seam rules), registered in
`docs/engine/index.md`. Check in the fixture rule set and the golden
coordinate list. Resolve §10. Close the **Phase 0: verify** items:
cross-target float determinism (PGEN-GEN-009) and Starfield's planet
generation and save behaviour (§6 row 7). Measure the authored-cell pre-parse baseline
and set the PGEN-STR-002 margin and the PGEN-STR-003 budget. Exit: the
design and fixture are checked in, every verify item is closed or filed as
an issue, and every §10 item carries **Decided**.

**Phase 1 — Generator and terrain.** The generator crate (pure CPU, no ECS
or GPU, as `crates/terrain-height` is), heights, normals, splat and water;
the session-local store, consulted by the pre-parse worker, the apply job
and every read site the design classifies as "must see generated cells";
the worldspace record. Satisfies PGEN-DOM-001…007, PGEN-GEN-001…009,
PGEN-TER-001…007, PGEN-DX-001/002. Exit: A1–A4a pass in CI; A6 passes on
the Wave 1 titles (§10 Q9).

**Phase 2 — Biomes and placed objects.** Biome field, object rules, Poisson
scatter, identity, ground-cover hints. Satisfies PGEN-BIO-001…006,
PGEN-OBJ-001…005, PGEN-ID-001…003. Exit: A5 passes; A6 reports a placement
floor set in Phase 0 and zero rule-set `tex.missing` entries; M-PGEN-6
published for Wave 1.

**Phase 3 — Streaming, distant terrain and saves.** Coarse height query,
LOD ring feed, telemetry, save fields and the mismatch policy. Satisfies
PGEN-LOD-001…003, PGEN-STR-001…003, PGEN-SAV-001…005. Exit: A7, A8 and A9
pass on Wave 1; M-PGEN-1 to M-PGEN-4 published in ROADMAP.

**Phase 4 — Seams with authored land (only if §10 Q1 chooses B).** Seam
copy and fall-off blending against authored edges (PGEN-TER-003). Exit:
M-PGEN-5 is zero on the authored seam row over the Wave 1 fixture.

**Phase 5 — Bake (depends on M50).** PGEN-BAKE-001…003. Exit: a baked
fixture cell loads as P4 with its heights within PGEN-BAKE-002's bound,
and a pre-bake save of the fixture loads with the outcome §10 Q10 chose
(under the recommendation: every pre-bake delta applied, none unmatched).

**Phase 6 — Wave 2.** Oblivion, FO3, FO4 palettes. Exit: A6–A8 pass on each
Wave 2 title; M-PGEN-6 published per title.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — What a generated domain attaches to.** Recommended: **A**, a new
   worldspace the rule set's package supplies, reached by a door or the
   CLI; no seam with authored land, no vanilla border or quest assumption
   at risk. Consequence: the package must supply the worldspace record the
   exterior session needs (§3.2 row 7a, PGEN-DOM-006): either an ESP with a
   `WRLD` (climate, default water, bounds) and no cells, or a worldspace
   block in the rule set that the loader turns into a parser-tier
   `WorldspaceRecord`, which Q2 settles.
   Alternative **B**: also the unauthored rim of an existing worldspace,
   extending past vanilla borders; adds Phase 4 and the seam metric, and
   exposes land that border regions were authored to fence off (§6 row 6).
   Alternative **C**: biomes painted *over* vanilla areas; contradicts
   "complements rather than replaces" and PGEN-NG-001, so not recommended.
2. **Q2 — Rule-set format and authorship.** Recommended: a Redux-native
   text file (TOML, the format `plugin.toml` and the boot request use) with
   a `PluginId` from a manifest, authored by modders; it gives
   `PluginManifest` its first runtime consumer (§3.2 row 13). Alternative:
   legacy `REGN` records in an ESP, reusing `RDOT` object rows; ties the
   format to polygons painted in one worldspace and to an `RDOT` row layout
   the parser does not decode (§3.2 row 12); a legacy ESP's local IDs are
   24-bit (12-bit if light-flagged), which caps the generated local-ID
   layout (PGEN-ID-002,
   [Plugin Loading](../engine/plugin-loading.md#legacy-form-ids-decoded-at-parse-time)).
   Alternative: a sandboxed WASM
   generator ([Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md));
   maximum freedom, but determinism (PGEN-GEN-001/009) becomes the mod's
   promise rather than the engine's.
3. **Q3 — "Effectively unlimited" versus the RT ceiling.** Recommended:
   ship bounded domains within 256 cells of the origin per axis (§3.2 row
   10; at most 512 cells, about 30 km, on a side at 58.5 m per cell,
   `cs-uesp-wiki/(main)/Exterior cells.wiki`) and record the ROADMAP row's
   "unlimited" as a later promise. Alternative: make absolute-space origin
   rebasing for TLAS, ray origins, skinned BLAS and physics part of M64;
   meets the row as written but adds renderer and physics work outside
   EXAL and its own triggers.
4. **Q4 — Loading a save made by another generator version.** Only the
   generator version is open here: a changed or absent rule-set package is
   already covered by the profile-change rules every package follows
   ([LMC-SAV-002, LMC-SAV-003](legacy-mod-compatibility.md#510-saves-and-profile-changes-lmc-sav)),
   and this PRD does not reopen them. Recommended:
   load, regenerate with the running version, apply only deltas whose
   identity still exists, park and report the rest (PGEN-SAV-003).
   Alternative: keep every released generator version loadable, exact
   restores at a maintenance cost that grows with each version.
   Alternative: refuse the load with a report; simplest, but a generator
   fix strands saves, against the spirit of LMC-SAV-002's "MUST NOT refuse
   a load solely for an absent plugin".
5. **Q5 — Non-local terrain operators.** Recommended: only operators with a
   declared bounded neighbourhood in v1 (noise, ridge and terrace shaping,
   tile-local erosion with an overlap margin). Alternative: a coarse
   whole-domain pass (hydrology, river networks, per Génevaux et al. 2013)
   computed once per rule set and cached on disk keyed by rule-set hash and
   generator version, like the terrain-height cache; better landforms, a
   first-boot cost and a cache to invalidate.
6. **Q6 — The M50 dependency.** Recommended: only Phase 5 (bake) depends
   on M50; rule sets are authored as text and previewed with PGEN-DX-003.
   Alternative: keep ROADMAP's dependency for the whole milestone; nothing
   ships until the editor
   ([In-Engine World Editor PRD](world-editor.md), PROPOSED) does. Either
   way the M64 row's "Depends on" cell in ROADMAP changes only after the
   owner decides.
7. **Q7 — Starfield's own planets.** Recommended: out of this milestone
   (PGEN-NG-005); decoding `BIOM` / `PNDT` and the planet data files
   belongs to Starfield exterior bring-up. Alternative: make Starfield
   planet surfaces the first consumer; the natural fit, but it blocks on
   record and file formats nobody here has decoded.
8. **Q8 — Seed ownership.** Recommended: the seed belongs to the rule set,
   so every player of a mod walks the same world (reproducible bug reports,
   compatible with M60 co-op), with a per-profile override. Alternative: a
   new seed per playthrough, stored in the save; more replay value, but no
   two players' reports describe the same land.
9. **Q9 — First wave.** Recommended: Skyrim SE and FNV (§7.1).
   Alternative: Skyrim SE alone, whose NIF trees make forest biomes look
   right first.
10. **Q10 — Bake identity.** Recommended: baked placements keep their
   generated pairs (PGEN-BAKE-003, written by WED-OUT-008), so pre-bake
   saves keep their changes with no remap step. Consequence: the baked
   plugin depends on the rule-set package, and removing that package
   disables the baked plugin with it under the missing-master rule
   ([LMC-ID-011](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)),
   so a bake cannot be shipped as a standalone replacement for its rule set.
   Alternative: fresh pairs under the output plugin's own `PluginId`
   (WED-REF-003 allocation) plus a generated-to-baked pair map applied
   before `apply_deltas`; the baked plugin stands alone, at the cost of a
   map the save path must read and the bake must keep. Alternative: fresh
   pairs and no map; pre-bake deltas on baked cells are listed unmatched by
   PGEN-SAV-003 and dropped, which makes baking a save-breaking act for the
   author's players. Reusing the rule set's `PluginId` as the output
   plugin's own identity is not an option: it breaks LMC-ID-010.
---

## 11. Document map

| Need | Go to |
|---|---|
| How a coordinate becomes a loaded cell; the worker and apply budget | [Exterior Grid Streaming](../engine/exterior-grid-streaming.md) |
| The translate boundary generated data must stay in front of | [EXAL §1](../engine/exal.md#1-the-three-tier-model) |
| Distant terrain and why object LOD is out | [EXAL §5](../engine/exal.md#5-lod-distance-rendering-the-must-have) |
| Ground cover over generated terrain; palette-only per-game data | [EXAL — Ground Cover](../engine/exal-groundcover.md) |
| Generator versioning and disk-cache precedent | [EXAL — Terrain height maps](../engine/exal-terrain-height.md) |
| Generated identity, plugin identity, conflict tie-break | [Plugin Loading](../engine/plugin-loading.md) |
| Package activation, fingerprint, ownership, save-side profile changes behind PGEN-DOM-007 and PGEN-SAV-001/003 | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| The output plugin and placement identity behind the bake and §10 Q6 / Q10 | [In-Engine World Editor PRD](world-editor.md) |
| Save capture, reload and delta overlay | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| State of streamed-out generated references | [Stream-Boundary State Continuity](../engine/stream-boundary-state-continuity.md) |
| The 2^20 RT ceiling behind §10 Q3 | [Shader Pipeline](../engine/shader-pipeline.md#absolute-world-space-rt-path--and-its-f32-ceiling) |
| Per-game tiers behind §7.1 | [Game Compatibility](../engine/game-compatibility.md) |
| WASM option for §10 Q2 | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) |
| Smoke-test index and conventions | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: authored-cell read sites (worker + apply job + ~40 sites), distant terrain needs a feed (finest band only, WRLD-bounds clip), identity aligned with LMC-ID and M50 (`FormIdPair` carried as a pair, reserved local range, package activation, fingerprint-owned rule-set hash, bake through WED-OUT-008 keeping generated pairs, manifest-UUID identity per WED §3.4, bake identity Q10, format-neutral BAKE-002), worldspace record for a new domain (PGEN-DOM-006), `prid` / `find_by_form_id` / telemetry premises corrected, existing `terrain.seams` / `lod.coverage` / `cell.owners` observables reused, testable TER/BIO wording, FO76 reason, Starfield citation narrowed | claude: revise |
