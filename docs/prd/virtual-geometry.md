# Virtual Geometry — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (build-thread and Starfield-density HEAD claims corrected,
one cluster layout with M52 per its Q6, distant-LOD authority cited from
EXAL instead of re-decided, a Starfield evidence row, a named-manifest mod
corpus, bundle-served parity for M82, citation scopes tightened, the
cross-vendor cluster-AS check dated). Revision 1 was the first draft: the
M53 promise (a deterministic cluster hierarchy per eligible mesh and
per-cluster level selection on the GPU), the evidence Tier 11 requires
before any of it is built, how it coexists with the mandatory ray-traced
passes, where it stops at the distant-LOD boundary, requirements by area,
phases and the seven decisions it needs from the owner. Nothing below is
built as a result of this document yet. Statements about how an original engine, a file format,
a paper, an external specification or a tool behaves cite an external
source; a claim that could not be verified is marked **Phase 0: verify**
instead of being stated as fact.

**Scope**: M53 in the [ROADMAP Tier 11 table](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon).
An eligible opaque rigid mesh is converted, once, into a hierarchy of
geometry clusters whose coarser levels are simplified from the finer ones;
each frame the GPU picks, per cluster, the coarsest level whose projected
error is below a pixel threshold, and the raster pass draws that cut through
M52's GPU-driven submission. Ray-traced passes keep tracing one fixed level
per mesh through today's BLAS lifecycle. In scope: the Phase 0 evidence
(bench attribution plus a per-game triangle census) that decides whether the
milestone is built at all, eligibility, the cluster format and its
deterministic build, GPU selection, the RT level, geometry residency, the
boundary with authored distant LOD, and the telemetry that proves each step.
Out of scope: GPU culling, indirect-draw generation and the mesh-shader path
(M52), persisting cluster data in a bundle (M82), texture residency (M39),
replacing authored distant LOD (decided by
[EXAL § 5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need);
decision Q4 covers only the ROADMAP row), a software micro-triangle
rasterizer, a visibility-buffer material pipeline, and any detail beyond the
source mesh.

**Related**: this document states the *promise*; the designs below own the
*how*. No design owns the cluster format, the simplification chain or the
selection yet; Phase 0 writes it (`docs/engine/virtual-geometry.md`, to be
registered in [docs/engine/index.md](../engine/index.md)).

| Mechanism | Owning design |
|---|---|
| Static BLAS per mesh, compaction, LRU eviction, per-frame TLAS, skinned BLAS refit | [Vulkan Renderer § RT acceleration structures](../engine/renderer.md#rt-acceleration-structures) |
| BLAS budget arithmetic and eviction ledger | [Memory Budget § Acceleration Structures](../engine/memory-budget.md#acceleration-structures-blas--tlas), [§ LRU eviction](../engine/memory-budget.md#lru-eviction) |
| Global vertex / index pools, admission tiers, 104 B vertex | [Memory Budget § Mesh Registry](../engine/memory-budget.md#mesh-registry) |
| Canonical mesh the builder consumes (Imported → translate → Canonical) | [NIFAL § 1](../engine/nifal.md#1-the-three-tier-model) |
| Distant LOD authority (asset-driven, no runtime baker) | [EXAL § 5](../engine/exal.md#5-lod-distance-rendering-the-must-have), [§ 5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need); remainder in the [Distant LOD Completion PRD](distant-lod-completion.md) |
| GPU culling, GPU-written draws, meshlets, mesh path, front-end share | [GPU-Driven Rendering PRD](gpu-driven-rendering.md) (M52; no design yet) |
| Bundle section that may persist cluster data | [Asset Preprocessing PRD](asset-preprocessing.md) (M82, `BAKE-DER-004`) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |
| Bench methodology and bench-of-record | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench) |
| Cluster format, DAG build, error metric, selection, RT level, cluster residency | `docs/engine/virtual-geometry.md` (Phase 0 writes it) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `VGEO-` prefix.

---

## 1. Product thesis

Virtual geometry, as Nanite defines it, exists so that "artist authoring
choices [are] disconnected from rendering cost": meshes are split into
128-triangle clusters, groups of clusters are simplified to half their
triangles with their shared boundary locked, the levels form a DAG, and the
renderer draws the cut where a cluster's projected error is under a pixel
while its parent's is not (B. Karis, R. Stubbe, G. Wihlidal, *Nanite: A Deep
Dive*, SIGGRAPH 2021 Advances in Real-Time Rendering,
<https://advances.realtimerendering.com/s2021/Karis_Nanite_SIGGRAPH_Advances_2021_final.pdf>).
Its premise is source content far denser than a game's low-poly meshes; the
same talk lists what the first version did not cover ("Translucent or masked
materials", "Non-rigid deformation, skeletal animation") and where it is
weak ("Not great with aggregates … Grass, leaves, hair").

The seven titles were authored for the hardware of their day. The Skyrim Creation
Kit tells authors that distant LOD meshes "must be VERY low poly" and that a
small building may need only one LOD level (CK wiki, *Distant LOD Meshes
Dialog*, local dump `ck-uesp-wiki/(main)/Distant LOD Meshes Dialog.wiki`).
Whether full-detail vanilla meshes are dense enough for a cluster hierarchy
to remove meaningful triangle work is not known: it is the first thing
Phase 0 measures (VGEO-EV-003). Epic's own guidance names the good
candidates: a mesh that "Contains many triangles, or has triangles that will
be very small on screen", "Has many instances in the scene" or "Acts as a
major occluder of other Nanite geometry" (Epic Games, *Nanite Virtualized Geometry in Unreal Engine*,
<https://dev.epicgames.com/documentation/en-us/unreal-engine/nanite-virtualized-geometry-in-unreal-engine>).
The many-instances case fits Bethesda interiors. The many-triangles case
is the hypothesis for Starfield, whose `BSGeometry` format can carry
authored meshlets and LOD triangle lists (§3.2 row 9), and for high-poly
mod replacers; how many Starfield meshes actually carry them, and how dense
either corpus is, is not measured at HEAD and is what VGEO-EV-004 and
VGEO-EV-005 report.

The feature is for two readers. The player on the 6 GB RT-minimum card
([Memory Budget](../engine/memory-budget.md)) needs dense exteriors and
cluttered interiors to fit a triangle and geometry-memory budget that does
not grow with what the camera can see from far away. The modder who ships a
high-poly replacement mesh needs it not to cost its full triangle count at
every distance. The [Tier 11 rule](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon)
applies to both: the milestone starts only when a real-content bench names
triangle work as the bottleneck.

The renderer is ray-traced first and ray query is mandatory: the device
picker rejects a GPU without `VK_KHR_acceleration_structure` and
`VK_KHR_ray_query` (`RT_EXTENSIONS`, `is_device_suitable`,
`crates/renderer/src/vulkan/device.rs`, at HEAD on 2026-11-01). A BLAS is
built once over fixed geometry, and an update must keep the source build's
`geometryCount` and `primitiveCount`
(VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03758 and
-primitiveCount-03769,
<https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdBuildAccelerationStructuresKHR.html>),
so a per-frame raster cut cannot be the traced geometry. Nanite's answer is
the same: ray tracing uses a "Fallback Mesh" by default, and native
ray tracing of Nanite meshes is experimental (Epic, *Nanite Virtualized
Geometry*); the 2021 talk gives the reasons as "DXR isn't flexible enough"
for its LOD logic and "No partial BVH updates" (Karis et al. 2021).

**Corollary that shapes every requirement below: virtual geometry changes
only how many triangles the raster pass draws; it never adds detail beyond
the source mesh, never changes what the ray-traced passes trace, and never
replaces geometry an author shipped as LOD.** The engine will not trace a
per-frame cut, will not cluster skinned, morphed, blended or aggregate
geometry in this revision, will not generate distant LOD from full meshes,
and will not be built at all if Phase 0 finds no scene it would speed up.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Meshlet** | As defined in the [GPU-Driven Rendering PRD](gpu-driven-rendering.md#2-terminology): a bounded cluster of vertices and triangles cut from one canonical mesh, with a bounding sphere and normal cone. Starfield's authored `Meshlet` struct (`crates/nif/src/blocks/bs_geometry.rs`) is a flat instance of the same idea |
| **Geometry cluster** | A meshlet that also carries its own simplification error and its parent's error, and belongs to a cluster DAG. Distinct from the *light clusters* of `cluster_cull.comp` |
| **Cluster group** | The set of clusters merged and re-simplified together to produce the next coarser level; its outer boundary is locked during that simplification (Karis et al. 2021; meshoptimizer `meshopt_SimplifyLockBorder`, <https://github.com/zeux/meshoptimizer>) |
| **Cluster DAG** | The levels of one mesh from the source (level 0) to a single root cluster; a DAG rather than a tree because groups are re-split after simplification (Karis et al. 2021) |
| **Projected error** | A cluster's object-space error bound projected to pixels for the current view |
| **Error threshold** | The pixel error below which a cluster may be drawn instead of its children; one setting (VGEO-SEL-002) |
| **Cut** | The set of clusters drawn for one instance in one frame: those whose projected error is under the threshold while their parent's is not (Karis et al. 2021, "View dependent cut") |
| **Virtual mesh** | A canonical mesh that has a cluster DAG and draws by cut. Every other mesh is **conventional** and draws exactly as at HEAD |
| **Eligibility reason** | The single code that says why a mesh is conventional (§3.1) |
| **RT level** | The one fixed cut, chosen per mesh at build time, whose triangles the mesh's BLAS is built from; the analogue of Unreal's Fallback Mesh |
| **Cluster residency** | Which levels of a virtual mesh hold GPU memory (§5.6) |
| **CLAS** | Cluster-level acceleration structure of `VK_NV_cluster_acceleration_structure` (revision 4, not ratified, NVIDIA): BLAS built from pre-built cluster structures (<https://docs.vulkan.org/refpages/latest/refpages/source/VK_NV_cluster_acceleration_structure.html>) |
| **Front-end share** | The resolution-invariant part of `gpu_main_render`, as defined by [GPU-Driven Rendering § 8](gpu-driven-rendering.md#8-metrics) |
| **Triangle census** | The per-game Phase 0 table of how a scene's submitted triangles split across mesh classes and eligibility reasons (VGEO-EV-003) |

---

## 3. Core model

### 3.1 Mesh classes and eligibility reasons

Every canonical mesh is exactly one class. `mesh.clusters` (VGEO-TEL-001)
reports the count, triangles and bytes per class and per reason; a mesh's
reason is printed by `mesh.clusters <entity>`.

| Class / reason | Meaning | Why it is excluded in this revision |
|---|---|---|
| `virtual` | Opaque, rigid, at or above the size threshold, cluster DAG built | — |
| `below-threshold` | Fewer triangles than the design's minimum for a DAG to have more than one level | A one-cluster mesh gains nothing; Epic's sky-sphere exception makes the same point |
| `skinned` | Has bone weights (`DrawCommand.bone_offset != 0` at draw) | Raster skins inline in `triangle.vert`; RT refits a per-entity BLAS that must keep its primitive count (§3.2 row 5); decision Q5 |
| `morph` | Driven by morph targets (FaceGen) | Unreal: "Deformation with Morph Targets is not supported with Nanite" (Epic, *Nanite Virtualized Geometry*) |
| `blended` | Alpha-blended or sorted | Karis et al. 2021: translucent not supported; blended draws are depth-sorted per draw on the CPU (`draw_sort_key`, `byroredux/src/render/mod.rs`) |
| `alpha-tested` | Alpha-tested foliage and cards | Aggregates are Nanite's weak case (Karis et al. 2021); simplification moves cut-out silhouettes. Current Unreal accepts the Masked blend mode (Epic, *Nanite Virtualized Geometry*), so this exclusion is a Redux choice that VGEO-ELIG-005 can lift. Phase 0 measures the share (VGEO-EV-003) |
| `lod-family` | `IsLodTerrain` entities: terrain LOD blocks, `.bto`, `.btt`, placement LOD | Authored distant LOD stays the far-field authority (§5.7, decision Q4) |
| `excluded-pipeline` | Water, particles, decals, ground cover, sky, UI | Own pipelines, out of scope |
| `build-failed` | The builder returned an error | Counted; draws conventionally (VGEO-FMT-007) |

### 3.2 The substrate at HEAD

The inventory below is the state at HEAD on 2026-11-01. It is hand-written;
Phase 0's design replaces it with the `mesh.clusters` report once Phase 1
ships, and rows 2, 3 and 10 are re-checked against the M52 PRD's own
inventory when M52 lands.

| # | Mechanism | Today |
|---|---|---|
| 1 | RT device requirement | Mandatory: `RT_EXTENSIONS` (acceleration structure, ray query, deferred host operations) must all be present or `is_device_suitable` returns `Ok(None)` (`crates/renderer/src/vulkan/device.rs`, #3759, #4894) |
| 2 | Mesh / task shaders | None: no `VK_EXT_mesh_shader` use anywhere under `crates/renderer/` |
| 3 | Raster submission | CPU-driven: `build_render_data` (`byroredux/src/render/mod.rs`) walks drawable entities; the CPU writes indirect commands drawn with `cmd_draw_indexed_indirect` (`crates/renderer/src/vulkan/context/geometry_pass.rs`) |
| 4 | Static BLAS | One per `MeshHandle`, built at cell load by `build_blas_batched` with `STATIC_BLAS_FLAGS` = `PREFER_FAST_TRACE \| ALLOW_COMPACTION` (`acceleration/blas_static.rs`, `constants.rs`), compacted, LRU-evicted under `blas_budget_for_heap` (`predicates.rs`), restored on draw by `restore_missing_static_blas_for_draws` (`context/resources.rs`) |
| 5 | Skinned geometry | Raster skins inline from the bone palette (`triangle.vert`); RT builds one BLAS per entity from the position-only skin-slot output and refits it with `UPDATE` (`acceleration/blas_skinned.rs`) |
| 6 | Distant LOD and RT | `IsLodTerrain` entities (`byroredux/src/components/render.rs`) build no BLAS at spawn (`cell_loader/terrain_lod.rs`, `placement_lod.rs` module docs) and are kept out of the TLAS by `tlas_exclusion` → `TlasExclusion::DistantLodBlock` (`byroredux/src/render/static_meshes.rs`) unless within `LOD_SHADOW_CASTER_DISTANCE` (`crates/renderer/src/shader_constants.rs`) |
| 7 | Geometry pools | Strict admission caps `VERTEX_POOL_HARD_CAP` / `INDEX_POOL_HARD_CAP` (`crates/renderer/src/mesh/pool_limits.rs`) with `GeometryAdmissionClass` tiers; values, the 104 B vertex and the cap history driven by FO4 exterior radius are in [Memory Budget § Mesh Registry](../engine/memory-budget.md#mesh-registry) |
| 8 | Authored per-mesh LOD | `ImportedMesh::bs_lod_cutoffs` (FO4 `BSMeshLODTriShape`, Skyrim `BSLODTriShape`) is write-only (#4236, `crates/nif/src/import/types/mesh.rs`); `NiLODNode` walks child 0 only (`switch_active_children`, `crates/nif/src/import/walk/mod.rs`) |
| 9 | Starfield authored clusters | `BSGeometryMeshData` parses `lods`, `meshlets` and `cull_data` (`crates/nif/src/blocks/bs_geometry.rs`); nothing outside that file reads them. `Meshlet` holds vertex/primitive counts and offsets only, no error or hierarchy. The share of Starfield meshes with a non-empty `meshlets` or `lods` table is not measured (VGEO-EV-004) |
| 10 | GPU telemetry | `BYRO_PROFILE` prints a `gpu_geometry phases:` line (main, opaque, blended, water, ground cover) and the one pipeline statistic queried is `FRAGMENT_SHADER_INVOCATIONS` (`crates/renderer/src/vulkan/gpu_timers.rs`); no rasterized-triangle counter exists |
| 11 | Simplifier / clusterizer | No dependency: no `meshopt` or meshoptimizer entry in any `Cargo.toml` |
| 12 | Debug views | `render.debug lod` selects `RtLod`, the shading-effect distance tier in `triangle.frag`, not a geometry level (`crates/renderer/src/vulkan/render_debug.rs`) |
| 13 | Where import and material translate run | Streamed exterior cells import NIFs on the pre-parse worker (`import_nif_with_collision_and_resolver`, `byroredux/src/streaming/pre_parse.rs`); interior and initial cell loads import through `parse_and_import_nif` (`byroredux/src/cell_loader/references/import.rs`) on the cell-load path; material translation, which decides blended and alpha-tested, runs at spawn and draw (`byroredux/src/material_translate.rs` callers in `cell_loader/` and `render/static_meshes.rs`), not on the worker |

---

## 4. Goals and non-goals

### 4.1 Goals

- **VGEO-G-001 — Evidence decides.** Phase 0 publishes, per bench scene and
  per in-scope game, the front-end share and the triangle census, and a
  go/no-go verdict under the rule of decision Q1.
- **VGEO-G-002 — Deterministic hierarchy.** Every eligible mesh in each
  in-scope game's corpus builds a cluster DAG, and two builds of the same
  canonical mesh are byte-identical.
- **VGEO-G-003 — Fewer triangles at equal image.** On each bench scene the
  rasterized triangle count falls with the image difference to the
  conventional path bounded by the design's threshold, and frame ms does not
  rise.
- **VGEO-G-004 — RT cost unchanged or lower.** Resident static BLAS bytes
  and the `rt.integrity` membership counts on each bench scene are no worse
  than the conventional control.
- **VGEO-G-005 — Geometry memory bounded by need.** If decision Q6 approves
  cluster residency, resident geometry-pool vertices on the FO4 exterior
  boundary route fall at the same radius.
- **VGEO-G-006 — One switch.** The feature turns off with one setting and
  the frame matches the conventional path.

### 4.2 Non-goals

- **VGEO-NG-001.** Replacing authored distant LOD (`.btr`, `.bto`, `.btt`,
  Oblivion `.lod` / `_far.nif`, FO3/FNV `blocks\` quads) with clusters
  generated from full meshes. Already decided, not reopened here: EXAL
  states that runtime distant rendering is "asset-driven", that the shipped
  `.bto` / `.lod` assets "already encode the result", and that an engine LOD
  baker is out of scope
  ([EXAL § 5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need));
  its research log resolves the object-LOD authority as "prebaked assets"
  with external sources ([EXAL Q3](../engine/exal.md#q3--object-lod-runtime-authority--resolved-prebaked-assets-not-mnam)).
  The older ROADMAP M53 row wording contradicts it; decision Q4 asks the
  owner only to amend that row.
- **VGEO-NG-002.** GPU culling, GPU-written draws and the mesh-shader path.
  M52 owns them ([GPU-Driven Rendering PRD](gpu-driven-rendering.md));
  M53 consumes them.
- **VGEO-NG-003.** A software rasterizer for micro-triangles. Nanite
  software-rasterizes "clusters whos triangles are less than 32 pixels long"
  (Karis et al. 2021); vanilla densities are not expected to reach that
  regime, and Phase 0's census says whether modded content does.
- **VGEO-NG-004.** A visibility-buffer / deferred-material rewrite of the
  main pass. The G-buffer and `triangle.frag` stay.
- **VGEO-NG-005.** Clustering skinned, morph-target, blended, alpha-tested
  or aggregate geometry in this revision (§3.1); Q5 decides whether skinned
  joins later.
- **VGEO-NG-006.** Texture residency. M39 owns it
  ([Texture Streaming PRD](texture-streaming.md)).
- **VGEO-NG-007.** Persisting cluster data on disk. M82 reserves the
  section (`BAKE-DER-004`, [Asset Preprocessing PRD](asset-preprocessing.md));
  this PRD only promises the data is deterministic enough to cache.
- **VGEO-NG-008.** Adding detail: tessellation, displacement or mesh
  upscaling. Level 0 is the source mesh.
- **VGEO-NG-009.** Tracing a per-frame cut, or requiring
  `VK_NV_cluster_acceleration_structure`. A vendor extension MAY be
  evaluated (decision Q3); the baseline works on every device the picker
  accepts.

---

## 5. Requirements by area

### 5.1 Evidence (`VGEO-EV`)

- **VGEO-EV-001.** Phase 0 MUST reuse the M52 Phase 0 bench run and
  attribution table (`GDR-EV-001`, `GDR-EV-002`,
  [GPU-Driven Rendering § 5.1](gpu-driven-rendering.md#51-evidence-gdr-ev))
  rather than run a second matrix, and MUST cite its TSV by path.
- **VGEO-EV-002.** Phase 0 MUST add one pinned-pose exterior row per
  in-scope exterior game (FNV, FO3, Oblivion, Skyrim SE, FO4) at the
  `m-exteriors.sh static` fixture pose, under the same methodology and in at
  least the `taa` and FSR Performance configs (the two the front-end share
  of `GDR-EV-003` needs), and check its raw rows in as
  `docs/audits/BENCH_m53-phase0_<hash>.tsv`. If M52's Phase 0 has not run,
  M53's Phase 0 MUST run `GDR-EV-001` itself under that ID's file name, so
  the two PRDs still share one matrix.
- **VGEO-EV-003.** Phase 0 MUST produce a triangle census per bench scene:
  submitted triangles split by §3.1 class and reason, and the share of
  submitted triangles in meshes the builder would accept. Today no engine
  counter exists (§3.2 row 10). **Phase 0: verify** that a GPU frame
  capture (RenderDoc or Nsight Graphics) of the pinned frame reports
  per-draw index counts for indirect draws, so the census needs no engine
  code; otherwise the counter of VGEO-TEL-002 moves into Phase 1 and the
  census follows it.
- **VGEO-EV-004.** Phase 0 MUST produce a corpus triangle-count
  distribution per game (static meshes only, triangles per canonical
  mesh, percentiles) from an `--ignored` corpus test in `crates/nif/tests/`
  that runs the import the runtime runs, alongside `parse_real_nifs.rs`.
- **VGEO-EV-005.** The census of VGEO-EV-003 and the distribution of
  VGEO-EV-004 MUST be repeated on a mod corpus the owner names (decision
  Q2). The corpus MUST be identified by a checked-in manifest
  (`docs/audits/m53-mod-corpus.tsv`: mod name, version, source URL, archive
  SHA-256) that holds no mod bytes; the files are installed locally and
  never checked in, and a run whose local archives do not match the
  manifest hashes MUST report itself as not comparable.
- **VGEO-EV-006.** The Phase 0 verdict MUST state, per scene, whether the
  front-end share and the eligible-triangle share meet the Q1 rule, and the
  milestone MUST NOT pass Phase 0 when no scene meets it.
- **VGEO-EV-007.** Phase 0 MUST add one pinned-pose row for the walkable
  Starfield Cydonia interior under the same methodology, since Starfield
  is the in-scope title the thesis leans on (§1) and has no
  bench-of-record scene, and VGEO-EV-004's Starfield distribution MUST
  report the share of meshes with a non-empty `meshlets` and `lods` table.

### 5.2 Eligibility (`VGEO-ELIG`)

- **VGEO-ELIG-001.** Every canonical mesh MUST be assigned exactly one
  class from §3.1 at registration, and `mesh.clusters` MUST print counts,
  triangles and bytes per class.
- **VGEO-ELIG-002.** Eligibility MUST be decided from canonical mesh and
  material data after NIFAL's translate step; an eligibility rule MUST NOT
  name a game, a NIF block type or a `bsver` value.
- **VGEO-ELIG-003.** The size threshold below which a mesh is
  `below-threshold` MUST be one design-owned constant, printed by
  `mesh.clusters`.
- **VGEO-ELIG-004.** A conventional mesh MUST draw through the same path,
  with the same `DrawCommand` fields, as at HEAD.
- **VGEO-ELIG-005.** A mesh that is `alpha-tested` MAY become eligible only
  after a Phase 2 A/B shows its cut-out coverage stays within the design's
  bound; until then it MUST stay conventional.

### 5.3 Cluster format and build (`VGEO-FMT`)

- **VGEO-FMT-001.** A virtual mesh MUST carry one cluster layout: level 0
  of its DAG either is the M52 meshlet set of the same mesh or replaces that
  set for the mesh, and `mesh.clusters <entity>` MUST report exactly one
  resident layout. This follows the recommended answer to GPU-Driven
  Rendering decision Q6 (M52's layout is renderer-private and flat; the
  M53 PRD owns any cluster format and may replace it); which of the two the
  design picks is its call, since meshoptimizer recommends 64 / 126
  vertex / triangle meshlets for NVIDIA mesh shaders
  ([meshoptimizer README](https://github.com/zeux/meshoptimizer)) while
  Nanite builds 128-triangle clusters (Karis et al. 2021).
- **VGEO-FMT-002.** Each coarser level MUST be produced by merging cluster
  groups and simplifying each group with its outer boundary locked, so any
  cut is crack-free (Karis et al. 2021; meshoptimizer
  `meshopt_SimplifyLockBorder`). A new test
  `cluster_cut_boundaries_are_crack_free` MUST check, on synthetic meshes,
  that every cut's boundary edges are shared by exactly two triangles or lie
  on the source mesh's own border.
- **VGEO-FMT-003.** Every cluster MUST store its error and its parent's
  error with parent ≥ child along every edge of the DAG, so the cut is
  decidable per cluster in parallel (Karis et al. 2021, "Force error to be
  monotonic"). A new test `cluster_error_is_monotonic` MUST check it on
  every corpus mesh of VGEO-FMT-006.
- **VGEO-FMT-004.** The build MUST be deterministic: the same canonical
  mesh MUST produce byte-identical output across two runs, across thread
  counts and across machines of the same architecture. A new test
  `cluster_dag_build_is_byte_identical` checks the first two; the third is
  checked by recomputing the A4 corpus hash on a second machine before
  M82's cluster section (its Phase 5) is filled.
  **Phase 0: verify** whether meshoptimizer (if Q7 selects it) documents
  determinism, since its README does not state it (checked 2026-11-01).
- **VGEO-FMT-005.** The output MUST carry a format version and a content
  key derived from the canonical mesh bytes and the build parameters, so an
  M82 bundle section can serve it as a cache and be rejected when either
  changes. The geometry half of that key is the converged vertex / index
  data the import already emits ([NIFAL § Geometry](../engine/nifal.md#geometry--transform--converged-reference-template)),
  which an M82 bake produces through the runtime's own import call
  (`BAKE-DER-003`).
- **VGEO-FMT-006.** A corpus test (`--ignored`, new `cluster_build_corpus`)
  MUST build every eligible static mesh of each in-scope game's mesh
  archives and report the count built, failed and skipped per reason; the
  pass rule is zero failures.
- **VGEO-FMT-007.** A mesh whose build fails MUST be classed `build-failed`
  and draw conventionally; a build failure MUST NOT drop a mesh from raster
  or from the TLAS.
- **VGEO-FMT-008.** The builder MUST take canonical mesh data only;
  `crates/renderer` MUST NOT gain a `byroredux-nif` dependency for it.
  Today it has none.
- **VGEO-FMT-009.** The build MUST run off the render thread, and its
  per-mesh time MUST be reported by the corpus test and summed per cell load
  in the `mesh.clusters` report. At HEAD only streamed exterior cells import
  on a worker
  ([Exterior Grid Streaming § 4](../engine/exterior-grid-streaming.md#4-streaming-phase-1-async-pre-parse-worker));
  interior loads import on the cell-load path, and the material translation
  that decides `blended` / `alpha-tested` runs at spawn (§3.2 row 13). The
  Phase 0 design MUST state which thread builds for each load path and
  where the class is decided before the build starts.
- **VGEO-FMT-010.** Cluster data MUST be counted in the
  [Memory Budget](../engine/memory-budget.md) ledger as a new Mesh Registry
  row in the commit that allocates it.
- **VGEO-FMT-011.** Cluster data served from an M82 bundle section MUST be
  byte-identical to a fresh build with the same content key
  (VGEO-FMT-005); scenario A14 checks it once M82's cluster section exists.
  This is the "load acceptance" the Asset Preprocessing PRD's Phase 5 exit
  refers to.

### 5.4 Selection and raster (`VGEO-SEL`)

- **VGEO-SEL-001.** The cut for each visible instance of a virtual mesh
  MUST be chosen on the GPU each frame; the CPU's per-frame cost for that
  instance MUST be the one instance record M52's path already writes, with
  no per-cluster CPU work.
- **VGEO-SEL-002.** The error threshold MUST be one registered setting in
  `SettingsRegistry` (`crates/core/src/settings/`), defaulting to the
  design's value, so presets in `assets/graphics_presets.toml` can move it;
  Karis et al. 2021 use one pixel ("If < 1 pixel error, they are
  imperceptibly different").
- **VGEO-SEL-003.** Projected error MUST account for the render extent the
  upscaler actually rasterizes at, not the output extent, so FSR
  configurations select for the pixels drawn.
- **VGEO-SEL-004.** With the error threshold forced to zero (every cut at
  level 0), a scene MUST render a `screenshot` within the design's image
  tolerance of the switch-off control (§7.2 A6); the same pair captured in
  `render.debug facing_ratio` SHOULD match too, as the normal check.
- **VGEO-SEL-005.** On a static pinned camera the number of instances
  whose cut changed since the previous frame MUST be zero, printed by
  `mesh.clusters` as a cut-change count, so a resting view never flickers
  between levels. Today no cut exists, so the count is defined from
  Phase 2.
- **VGEO-SEL-006.** On a device or frame where M52's GPU path is not
  active, a virtual mesh MUST draw its RT level through the conventional
  path, never nothing.
- **VGEO-SEL-007.** A new `render.debug cluster_level` mode MUST colour each
  pixel by the DAG level that drew it; the name `lod` stays with the RT
  shading tier (§3.2 row 12).
- **VGEO-SEL-008.** A level change MUST NOT reset an instance's TAA or SVGF
  history; the end-of-orbit frame-to-frame image difference of §7.2 A7
  with the switch on MUST NOT exceed the switch-off control's beyond its
  run range.

### 5.5 Ray tracing (`VGEO-RT`)

- **VGEO-RT-001.** A virtual mesh's BLAS MUST be built from its RT level,
  once, through `build_blas_batched` with `STATIC_BLAS_FLAGS`, keyed by its
  `MeshHandle`; a BLAS MUST NOT be built from a per-frame cut.
- **VGEO-RT-002.** The RT level MUST be the cut at one design-owned
  object-space error bound, the analogue of Unreal's "Fallback Relative
  Error" (Epic, *Nanite Virtualized Geometry*), printed by `mesh.clusters`;
  a bound of zero MUST select level 0, which is today's BLAS geometry.
- **VGEO-RT-003.** Compaction, LRU eviction and per-draw restore MUST apply
  to RT-level BLAS unchanged; `rt.integrity`'s eligible / emitted /
  missing counts on a pinned frame MUST equal the conventional control's.
- **VGEO-RT-004.** Resident static BLAS bytes (`resident_static_blas_bytes`)
  on each bench scene MUST be at or below the conventional control's.
- **VGEO-RT-005.** Shadow and reflection rays from a raster surface drawn at
  a finer level than the RT level MUST NOT self-occlude: the
  `render.debug shadow_visibility` capture of §7.2 A8 MUST show no more
  shadowed pixels on virtual meshes than the RT-level-equals-level-0
  control beyond the design's tolerance. **Phase 0: verify** the size of
  the effect with the current `offsetRayOriginForDirection` origin offset
  ([Vulkan Renderer § RT acceleration structures](../engine/renderer.md#rt-acceleration-structures))
  by tracing a coarser proxy against a finer raster surface in the
  `--cornell` scene.
- **VGEO-RT-006.** The TLAS MUST hold one instance per placed virtual mesh,
  as today; `MAX_INSTANCES` MUST NOT change for M53.
- **VGEO-RT-007.** A CLAS path MAY be added only behind a device probe for
  `VK_NV_cluster_acceleration_structure`, with VGEO-RT-001's path as the
  baseline on every other device, and only if decision Q3 approves it.
  **Phase 0: verify** whether CLAS is usable from ray queries without
  `VK_KHR_ray_tracing_pipeline`: `SPV_NV_cluster_acceleration_structure`
  adds `OpRayQueryGetIntersectionClusterIdNV`, but its capability
  "implicitly declares `RayTracingKHR`"
  (<https://github.khronos.org/SPIRV-Registry/extensions/NV/SPV_NV_cluster_acceleration_structure.html>),
  and NVIDIA's `vk_lod_clusters` sample traces through ray-generation
  shaders (<https://github.com/nvpro-samples/vk_lod_clusters>).
- **VGEO-RT-008.** Skinned BLAS refit MUST be unaffected: skinned meshes stay
  conventional (§3.1), and their BLAS keeps its build-time primitive count
  as VUID-…-primitiveCount-03769 requires.

### 5.6 Residency and memory (`VGEO-MEM`)

- **VGEO-MEM-001.** In Phase 2 every level of a virtual mesh MUST be
  resident while the mesh is resident, and the added bytes per mesh MUST be
  reported by `mesh.clusters` and the Memory Budget row (VGEO-FMT-010).
- **VGEO-MEM-002.** Virtual-mesh geometry MUST count against the same
  vertex and index pool caps and admission tiers as conventional geometry
  (`GeometryAdmissionClass`); M53 MUST NOT add a second pool budget.
- **VGEO-MEM-003.** If decision Q6 approves cluster residency, a virtual
  mesh MUST keep its coarse levels down to its RT level resident and MAY
  release finer levels not selected within a design-owned hysteresis
  window; a level selected but not resident MUST draw its nearest resident
  coarser ancestor, never a hole.
- **VGEO-MEM-004.** Cluster residency changes MUST retire buffers through
  the deferred-destroy ring with `MAX_FRAMES_IN_FLIGHT` aging
  ([Memory Budget § Deferred-Destroy Queue](../engine/memory-budget.md#deferred-destroy-queue));
  a residency path MUST NOT call `device_wait_idle`.
- **VGEO-MEM-005.** `ownership_sample.rs` MUST sample resident cluster
  bytes so `m-exteriors.sh soak` gates their return to baseline with the
  other owners.

### 5.7 Distant LOD boundary (`VGEO-LOD`)

- **VGEO-LOD-001.** `lod-family` entities MUST stay conventional and keep
  today's `tlas_exclusion` behaviour; M53 MUST NOT change what the
  [Distant LOD Completion PRD](distant-lod-completion.md) gates.
- **VGEO-LOD-002.** The `lod.coverage` and `terrain.seams` gates of
  `docs/smoke-tests/m-exteriors.sh boundary` MUST pass with virtual
  geometry on, on every profile that passes them with it off.
- **VGEO-LOD-003.** Authored per-mesh LOD data (`bs_lod_cutoffs`, Starfield
  `lods` and `meshlets`) MUST NOT be discarded by the builder's input path:
  the existing passthrough tests
  (`crates/nif/src/import/mesh/bs_tri_shape_kind_passthrough_tests.rs`) and
  `bs_geometry.rs`'s parser tests MUST keep passing unchanged, and Q7
  decides whether the builder reads the data.
- **VGEO-LOD-004.** Any later proposal to extend the full-detail radius
  because virtual meshes are cheaper at distance MUST go through the
  Distant LOD Completion PRD's band decisions, not through this PRD.

### 5.8 Telemetry and control (`VGEO-TEL`)

- **VGEO-TEL-001.** A new console command `mesh.clusters` MUST print the
  class table of §3.1, the error threshold, the RT-level bound, the size
  threshold, resident cluster bytes and the last cell load's build time;
  with an entity argument it MUST print that mesh's class, reason, level
  count, RT-level triangles and the level drawn last frame.
- **VGEO-TEL-002.** The `bench:` line MUST gain a rasterized-triangle token
  (`tris=`) counting triangles of the cut actually submitted, so the
  triangle metric of §8 is read from the engine on both paths.
- **VGEO-TEL-003.** One registered setting MUST switch virtual geometry off,
  restoring the conventional path for every mesh in the same binary, so
  every A/B in §7.2 has a same-binary control.
- **VGEO-TEL-004.** The `rt.integrity` line MUST NOT change format; virtual
  meshes appear in its existing counts.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Per-object LOD is authored: a Skyrim static picks LOD meshes per level, level 0 mandatory, levels not skipped (CK wiki, *Distant LOD Meshes Dialog*); "Has Distant LOD" makes it visible beyond the loaded area (CK wiki, *Static*) | Full-detail cell meshes additionally draw by cut inside the loaded area (VGEO-SEL-001); distant LOD unchanged (VGEO-LOD-001) | — | Silhouette pop at the level change → threshold setting (VGEO-SEL-002), rest stability (VGEO-SEL-005), history check (VGEO-SEL-008) |
| 2 | Engine-internal: raster and RT see identical triangles | Raster may draw finer than the RT level | — | Self-shadowing or light leaks where the surfaces disagree → VGEO-RT-002 bound, VGEO-RT-005 A/B |
| 3 | `NiLODNode` switches by distance in the original engine | Still child 0 only (§3.2 row 8); M53 does not use it | — | None new: the in-cell switch stays parked ([EXAL § 5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need)) |
| 4 | Starfield ships authored meshlets and LOD triangle lists (§3.2 row 9) | Rebuilt from the canonical mesh unless Q7 decides otherwise | — | Authored intent ignored → VGEO-LOD-003 keeps the data; Q7 |
| 5 | Mod LOD generators write vanilla-format LOD: xLODGen "creates vanilla style object LOD, tree LOD, terrain LOD … for many different game modes" (<https://dyndolod.info/Help/xLODGen>); DynDOLOD builds "a comprehensive LOD patch mod" (<https://dyndolod.info/>) | Consumed unchanged by M35's providers (VGEO-NG-001) | — | None, as long as Q4 keeps VGEO-NG-001 |
| 6 | Engine-internal: geometry memory is the whole source mesh | Adds coarse levels (Phase 2); may release fine levels (Q6) | — | More bytes per mesh before residency lands → VGEO-MEM-001 reporting, VGEO-MEM-002 shared caps |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The builder and the selection are game-agnostic: they read canonical mesh
data after NIFAL's translate step (VGEO-ELIG-002, VGEO-FMT-008). Per-game
differences in geometry encoding (half-float BSTriShape, Starfield
`BSGeometry` UDEC3 normals, SSE skinned reconstruction) stay in
`crates/nif/src/import/mesh/` as today. Waves follow Phase 0's census, not
the games: the wave a title lands in is the order of its eligible-triangle
share.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Interiors and exteriors render ([Game Compatibility](../engine/game-compatibility.md#per-game-support-detail)); the lowest-density corpus, so the likeliest `below-threshold` majority, which Phase 0 measures |
| Fallout 3 | yes | Interiors and exteriors render |
| Fallout: New Vegas | yes | Reference title; Prospector is a bench-of-record scene |
| Skyrim (LE / SE / AE) | SE / AE yes; LE no | SE: WhiterunBanneredMare is a bench scene and Tamriel loads. LE is parse-only (ROADMAP compatibility matrix), so there is no rendered scene to accept against |
| Fallout 4 | yes | MedTek and Dugout Inn are [bench-of-record](../../ROADMAP.md#how-to-run-a-bench) scenes; the Commonwealth route is the geometry-pool pressure case (§3.2 row 7) |
| Fallout 76 | no | Parse-only, no cell loader ([Game Compatibility](../engine/game-compatibility.md#per-game-support-detail)); the builder serves it unchanged once cells render |
| Starfield | yes | The walkable Cydonia interior (VGEO-EV-007); the only format that can ship authored clusters (§3.2 row 9), so Phase 0 measures it first; its wave still follows the census |

### 7.2 Acceptance

Game data and mod files stay on the machine; nothing below implies
redistribution. Every live scenario follows the
[bench methodology](../../ROADMAP.md#how-to-run-a-bench) (pinned
`--camera-pos` / `--camera-forward`, isolated `BYROREDUX_SETTINGS_PATH`,
same-machine control) and uses the VGEO-TEL-003 switch as its control.
Release builds need `BYRO_DEBUG_SERVER=1` for `byro-dbg` to attach.

| # | Scenario | Driven by | Data | Passes when |
|---|---|---|---|---|
| A1 | Determinism | new `cargo test` `cluster_dag_build_is_byte_identical` (crate named by the design) | synthetic meshes | two builds and a 1-thread vs N-thread build hash equal |
| A2 | Crack-free cuts | new `cargo test` `cluster_cut_boundaries_are_crack_free` | synthetic grid and closed meshes | every cut passes VGEO-FMT-002's edge rule |
| A3 | Monotonic error | new `cargo test` `cluster_error_is_monotonic` | synthetic, then A4's corpus | parent ≥ child on every DAG edge |
| A4 | Corpus build | new `--ignored` corpus test `cluster_build_corpus` | each in-scope game's mesh archives (the `parse_real_nifs` archive set) | zero `build-failed`; per-reason counts printed; determinism hash identical on a second run |
| A5 | Triangle reduction | `scripts/fsr-bench-matrix.sh 3 300`, once per arm, with the VGEO-TEL-003 setting seeded into each run's fresh settings file (the script gains that input in Phase 2) | the five bench-of-record scenes, plus the VGEO-EV-002 and VGEO-EV-007 rows | `tris=` lower on every scene where the census found eligible triangles; frame ms not higher beyond run range |
| A6 | Level-0 parity | `--bench-hold` + `byro-dbg`: threshold set to 0, `screenshot` on and off | Prospector, MedTek, Cydonia | image difference within the design's tolerance |
| A7 | Temporal stability | `--bench-hold` + `byro-dbg`: static pinned pose, then two consecutive `screenshot`s at the end of a pinned orbit, switch on and off | Whiterun, an FO4 exterior | cut-change count 0 at rest (VGEO-SEL-005); orbit frame difference within the control's run range (VGEO-SEL-008) |
| A8 | RT self-shadowing | `--bench-hold` + `byro-dbg`: `render.debug shadow_visibility`, RT bound default vs 0 | `--cornell` (redistributable control), MedTek | shadowed-pixel delta on virtual meshes within tolerance (VGEO-RT-005) |
| A9 | RT membership | `byro-dbg` `rt.integrity` and `mesh.clusters` on a pinned frame, switch on vs off | the five bench scenes | eligible / emitted / missing equal; resident static BLAS bytes not higher (VGEO-RT-003, -004) |
| A10 | Exterior gates | `docs/smoke-tests/m-exteriors.sh all boundary` with the switch on | installed exterior profiles | same verdict per profile as with the switch off (VGEO-LOD-002) |
| A11 | Residency soak (if Q6) | `docs/smoke-tests/m-exteriors.sh fo4 soak` | FO4 Commonwealth | resident cluster bytes return to baseline every cycle (VGEO-MEM-005) |
| A12 | Fallback | `BYRO_VALIDATION=1` run with M52's GPU path disabled | Prospector | every virtual mesh drawn at its RT level; zero validation messages (VGEO-SEL-006) |
| A13 | Mod corpus build | A4's corpus test pointed at the locally installed archives of the VGEO-EV-005 manifest | the owner's named mod corpus (local only, manifest checked in, no mod bytes) | zero `build-failed`; manifest hashes match; per-reason counts and corpus density published without mod content |
| A14 | Bundle-served parity (after M82 Phase 5) | A4 run twice, built fresh and served from M82 bundles | each in-scope game's mesh archives | determinism hash identical between the two arms (VGEO-FMT-011) |

---

## 8. Metrics

The numbers live in ROADMAP: Phase 0 adds an *M53 evidence* table beside the
M52 evidence table under the
[bench-of-record](../../ROADMAP.md#how-to-run-a-bench), and each later phase
adds a row; raw rows stay in `docs/audits/BENCH_*.tsv`.

| Metric | Definition | Granularity |
|---|---|---|
| Front-end share | As [GPU-Driven Rendering § 8](gpu-driven-rendering.md#8-metrics) defines it; not re-derived here | Per scene, Phase 0 and each phase |
| Eligible-triangle share | Submitted triangles in meshes the builder would accept ÷ all submitted triangles | Per scene and per game, Phase 0 |
| Corpus density | Triangles per canonical static mesh, 50th / 90th / 99th percentile | Per game corpus and per named mod corpus, Phase 0 |
| Triangle reduction | `tris=` switch on ÷ `tris=` switch off | Per scene × upscaler config, each phase |
| Frame delta | Median frame ms switch on − switch off | Per scene × upscaler config |
| Build cost | Corpus build ms per mesh (median, 99th) and summed per cell load | Per game, each phase |
| Cluster bytes | Cluster data bytes ÷ source geometry bytes for virtual meshes | Per game corpus |
| RT bytes delta | Resident static BLAS bytes switch on − switch off | Per bench scene |
| Pool occupancy | Resident vertex-pool count on the FO4 boundary route at fixed radius | Per phase (Q6 only) |

---

## 9. Delivery sequence

Depends on M52 (GPU-driven rendering): the cut is drawn through its
GPU-written draws and, where present, its mesh path. Phase 0 can run while
M52's Phase 0 runs and shares its bench run (VGEO-EV-001); Phase 1 depends
on M52 only if the design reuses M52's meshlet set as level 0
(VGEO-FMT-001); Phase 2 starts only after M52's GPU-culled path ships.
M82's cluster section (its Phase 5) depends on this PRD's Phase 1 format.

**Phase 0 — Evidence, census and decisions (no engine code).** Close the
verify items below; run VGEO-EV-001 to VGEO-EV-006 (the corpus census is an
`--ignored` test, no runtime change), VGEO-EV-007 and the VGEO-EV-005
manifest; write `docs/engine/virtual-geometry.md`
(format, error metric, selection, RT level, the constants this PRD calls
design-owned) and register it; get Q1–Q7 decided. Exit: the M53 evidence
table is published in ROADMAP with a go or no-go verdict under Q1, 5 of 5
verify items are closed or filed as issues, and 7 of 7 decisions carry
**Decided**. A no-go verdict moves this PRD to WITHDRAWN or parks it per Q1.

| V | Question | Settled by |
|---|---|---|
| V1 | A GPU frame capture reports per-draw index counts for the engine's indirect draws (VGEO-EV-003) | One RenderDoc or Nsight Graphics capture of the Prospector pinned frame, compared with `draws=` |
| V2 | meshoptimizer's clusterization and simplification output is deterministic across runs and thread counts (VGEO-FMT-004) | The meshoptimizer source and issue tracker, then A1 run against the chosen binding (crates.io `meshopt` 0.7.0 lists a `clusterlod` module, <https://docs.rs/meshopt/latest/meshopt/>; which meshoptimizer version it binds is not stated there) |
| V3 | CLAS is usable from ray queries without the ray-tracing-pipeline feature (VGEO-RT-007) | The `VK_NV_cluster_acceleration_structure` feature dependencies in the Vulkan registry and a `vulkaninfo` feature dump on the RTX 4070 Ti |
| V4 | Whether a cross-vendor (KHR / EXT) cluster acceleration structure extension exists by Phase 1 | Re-reading `xml/vk.xml` in [Vulkan-Docs](https://github.com/KhronosGroup/Vulkan-Docs) at Phase 1 start. On 2026-11-01 it lists only `VK_NV_cluster_acceleration_structure` (number 570, no `promotedto`) and `VK_NV_partitioned_acceleration_structure`; the meshoptimizer README also points at D3D's DXR 2 clustered geometry, which is not a Vulkan extension |
| V5 | Size of the self-shadowing error between a coarser RT level and a finer raster surface (VGEO-RT-005) | A `--cornell` capture with a deliberately simplified proxy BLAS, using a worktree spike (no PRD needed for a spike, [README § 3](README.md#3-lifecycle)) |

**Phase 1 — Format and build.** The builder, the class assignment and the
corpus gates, with nothing drawn differently: VGEO-ELIG-001 to -004,
VGEO-FMT-001 to -010, VGEO-TEL-001 (class table only), VGEO-TEL-002.
Exit: A1–A4 pass on every in-scope game, A13 passes on the named mod
corpus, and the corpus build cost and cluster-bytes ratio are published in
the M53 evidence table.

**Phase 2 — GPU selection.** Cut selection and drawing on M52's path, every
level resident, RT level at level 0 (bound zero): VGEO-SEL-001 to -008,
VGEO-RT-001, -003, -004, -006, -008, VGEO-MEM-001, -002, VGEO-LOD-001 to
-004, VGEO-TEL-003, -004. Exit: A5, A6, A7, A9, A10 and A12 pass on the
first wave's scenes, and the triangle reduction and frame delta are
published.

**Phase 3 — Coarser RT level.** The RT-level bound above zero:
VGEO-RT-002, -005. Exit: A8 and A9 pass with the design's default bound,
and the RT bytes delta is published per bench scene.

**Phase 4 — Cluster residency (only if Q6 approves).** VGEO-MEM-003 to
-005. Exit: A11 passes, and pool occupancy on the FO4 boundary route is
published at the same radius as the conventional control.

**Phase 5 — Later waves and optional paths.** Remaining waves by census
order; VGEO-ELIG-005 for alpha-tested meshes; VGEO-RT-007 if Q3 approves;
skinned meshes if Q5 approves; VGEO-FMT-011 once M82's cluster section
exists. Exit: A4–A10 pass on each added wave's scenes, with the
eligible-triangle share drawn as virtual published per game, and A14 passes
once M82 Phase 5 has shipped.

---

## 10. Decisions required

1. **Q1 — Go / no-go rule for Phase 0.** Recommended: proceed past Phase 0
   only if at least one pinned scene (bench-of-record or VGEO-EV-002
   exterior) has a front-end share above one half and an eligible-triangle
   share above one half, and park M53 in the ROADMAP parking lot otherwise,
   re-checked at each bench-of-record refresh. This makes the Tier 11
   "names it as the bottleneck" rule a number. Alternative A: proceed on
   the mod corpus's census alone (VGEO-EV-005); consequence: the milestone
   serves content the vanilla bench cannot see and its benefit is measured
   only on locally installed mods. Alternative B: WITHDRAWN on a no-go;
   consequence: a later density change (a Starfield-scale corpus, a mod
   wave) reopens with a new PRD.

2. **Q2 — Thesis: which content M53 is for.** Recommended: built for
   high-density content (Starfield and high-poly mod replacers) and
   accepted on vanilla only as "no regression, triangles reduced where
   eligible", with the owner naming one mod corpus for VGEO-EV-005 as a
   checked-in manifest (no mod bytes; A13 runs it locally). Rationale:
   Nanite's premise is source content dense enough that "artist authoring
   choices [are] disconnected from rendering cost" (Karis et al. 2021),
   and the vanilla full-detail density is unmeasured (VGEO-EV-004); the CK's
   "VERY low poly" guidance (CK wiki, *Distant LOD Meshes Dialog*) applies
   to distant LOD meshes only, so it says nothing about full-detail meshes.
   Alternative: promise a vanilla frame-time win;
   consequence: the promise stands or falls on a census that has not been
   run, and the Tier 11 rule may never be met.

3. **Q3 — RT geometry for virtual meshes.** Recommended: one fixed RT
   level per mesh through today's BLAS build, compaction and LRU
   (VGEO-RT-001), which works on every device the picker accepts and
   mirrors Unreal's default Fallback Mesh; meshoptimizer's README adds
   that cluster acceleration structures "currently [result] in reduced ray
   tracing performance for static geometry (for which a traditional BLAS
   may be more suitable)". Alternative A: add a CLAS path on NVIDIA behind
   a probe (VGEO-RT-007); consequence: two RT geometry paths, NVIDIA-only
   (the only vendor extension in the registry on 2026-11-01, V4), pending
   V3. Alternative B: rebuild BLAS from the
   cut on level changes; consequence: BLAS build cost every frame the cut
   moves, against the update rule (VUID-…-03769) and Karis's "No partial
   BVH updates" objection.

4. **Q4 — Amend the ROADMAP M53 row to match EXAL.** Not a decision about
   distant-LOD authority: EXAL already made it (runtime distant rendering
   is asset-driven from the shipped `.bto` / `.lod` assets, an engine LOD
   baker is out of scope,
   [EXAL § 5.4](../engine/exal.md#54-what-the-runtime-lod-does-not-need) and
   [EXAL Q3](../engine/exal.md#q3--object-lod-runtime-authority--resolved-prebaked-assets-not-mnam)),
   and this PRD follows it (VGEO-NG-001). The ROADMAP row predates that
   design and still says M53 "Makes M35 (.btr terrain LOD) obsolete …
   load full-resolution geometry"; changing a milestone's promise is a T1
   edit only the owner makes. Recommended: the row says instead that M53
   applies to full-detail meshes inside the loaded area and leaves authored
   distant LOD to M35 and the Distant LOD Completion PRD. Alternative: keep
   the row and reopen EXAL § 5.4 first, in an EXAL design revision rather
   than in this PRD; consequence: until that revision lands this PRD
   cannot be approved against its own milestone row, and the reopening
   carries what EXAL ruled out (an engine LOD baker, every worldspace
   REFR's full mesh resident or streamable, objects drawn at distance that
   authors excluded via "Has Distant LOD", CK wiki *Static*, and xLODGen /
   DynDOLOD output ignored, §6 row 5).

5. **Q5 — Skinned meshes.** Recommended: out of this PRD's phases
   (VGEO-NG-005), revisited only if Phase 0's census shows skinned
   triangles above a tenth of any scene's submitted triangles. Unreal's
   skeletal Nanite has "No geometry LODs" and uses animation LODs instead
   (Epic, *Nanite Virtualized Geometry*), and a skinned BLAS must keep its
   primitive count. Alternative: the ROADMAP row's "same data structure …
   for static and skinned meshes"; consequence: per-cluster bounds under
   bone motion and a raster/RT split per skinned entity, for a class that
   is a few NPCs per cell.

6. **Q6 — Cluster residency.** Recommended: Phase 2 keeps every level
   resident; Phase 4 (fine-level release) is built only if Phase 2 shows
   the vertex pool cap binding on the FO4 boundary route. Alternative:
   Nanite-style page streaming from disk from the start ("fine-grained
   streaming", Epic); consequence: a disk read path that M82's bundle is
   the natural home for, so M53 would wait on M82 while ROADMAP has M82
   wait on M53 (the Asset Preprocessing PRD's own decision Q6 recommends
   narrowing that to "M53 (Phase 5)", which removes the cycle only for
   M82's earlier phases).

7. **Q7 — Builder source and input.** Recommended: meshoptimizer (MIT;
   `meshopt_buildMeshlets`, `meshopt_simplify` with
   `meshopt_SimplifyLockBorder`, `meshopt_partitionClusters` for
   "hierarchical simplification schemes similar to Nanite", and the
   `clusterlod.h` library, all in its README), the same base NVIDIA's
   `vk_lod_clusters` uses, through a Rust binding under the
   [dependency philosophy](../engine/dependencies.md#dependency-philosophy),
   rebuilding Starfield clusters from the canonical mesh so every game has
   one format. Alternative A: a Rust simplifier written in-house;
   consequence: determinism fully owned, at the cost of re-deriving QEM
   with attributes (Karis et al. 2021). Alternative B: seed Starfield's
   level 0 from its authored `meshlets`; consequence: one game with a
   different level-0 origin, and authored meshlets carry no error to seed
   the DAG with (§3.2 row 9).

---

## 11. Document map

| Need | Go to |
|---|---|
| BLAS lifecycle, compaction, LRU, TLAS membership | [Vulkan Renderer § RT acceleration structures](../engine/renderer.md#rt-acceleration-structures) |
| BLAS budget and eviction ledger | [Memory Budget § Acceleration Structures](../engine/memory-budget.md#acceleration-structures-blas--tlas) |
| Geometry pools, admission tiers, deferred destroy | [Memory Budget § Mesh Registry](../engine/memory-budget.md#mesh-registry), [§ Deferred-Destroy Queue](../engine/memory-budget.md#deferred-destroy-queue) |
| Canonical mesh and the translate boundary | [NIFAL § 1](../engine/nifal.md#1-the-three-tier-model) |
| Distant LOD authority and its open work | [EXAL § 5](../engine/exal.md#5-lod-distance-rendering-the-must-have), [Distant LOD Completion PRD](distant-lod-completion.md) |
| GPU culling, meshlets, front-end share, M52 evidence | [GPU-Driven Rendering PRD](gpu-driven-rendering.md) |
| Persisted cluster section | [Asset Preprocessing PRD](asset-preprocessing.md) |
| Texture residency | [Texture Streaming PRD](texture-streaming.md) |
| Per-game tiers | [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |
| Exterior smoke gates | [`m-exteriors.sh`](../smoke-tests/m-exteriors.sh), [smoke-test index](../smoke-tests/README.md) |
| Cluster format, error metric, selection, RT level | `docs/engine/virtual-geometry.md` (Phase 0) |
| Live milestone state, bench-of-record and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: build-thread and Starfield-density HEAD claims, one cluster layout per GDR Q6, distant-LOD authority cited from EXAL (Q4 narrowed to the ROADMAP row), Starfield evidence row, mod-corpus manifest and A13, bundle-served parity (VGEO-FMT-011, A14), citation scopes, dated cross-vendor CLAS check, MUST NOT phrasing, SEL order | claude: revise |
