# GPU-Driven Rendering — Product Requirements

**Status**: PROPOSED (2026-11-05). Revision 3 records `GpuInstance` moving
into one struct-only header. Revision 2 (2026-11-01) folds in review
round 1 (bench-harness columns the evidence needs, the ECS change-tracking
state read from code instead of left as a verify item, the indirect-grouping
and draw-site line corrections, meshoptimizer size guidance and the
Wihlidal / Gamebryo citations narrowed to what the sources say, the sibling
PRD contracts — `GpuInstance` lockstep, texture-streaming draw list, the M53
cluster-format boundary — cited instead of implied). Revision 1 drafted the
M52 promise (GPU visibility culling, draw commands generated on the GPU, an
optional mesh-shader path, and a persistent GPU-side instance scene), the
evidence gate Tier 11 requires before any of it is built, requirements by
area, phases and the six decisions it needs from the owner. Nothing below
is built as a result of this document yet. Statements about how an
original engine, an external specification, a paper or a tool behaves cite
an external source; a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: M52 in the [ROADMAP Tier 11 table](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon).
The main raster pass's per-frame visibility test, draw grouping and draw
counts move from the CPU to the GPU, so the CPU's per-frame render cost
stops scaling with the number of drawable entities in a cell; on devices
that expose `VK_EXT_mesh_shader`, opaque static geometry may additionally
draw as meshlets through task and mesh shaders with per-meshlet frustum and
normal-cone culling. The current CPU-driven path stays as the fallback and
as the parity reference. In scope: the evidence that names the bottleneck
(Phase 0), GPU instance culling, GPU-written indirect draws and draw
counts, a persistent instance scene with delta uploads, meshlet generation
at mesh registration, the mesh/task-shader raster path, the TLAS's
relationship to all of the above, shader-contract upkeep and the telemetry
that proves each step. Out of scope: occlusion culling, LOD selection and
virtual geometry (M53), ray-traced-pass changes, the water, sky, ground
cover, particle and UI pipelines, and any RT-less renderer.

**Related**: this document states the *promise*; the designs below own the
*how*. No design owns GPU-driven submission yet; Phase 0 writes it
(`docs/engine/gpu-driven-rendering.md`, to be registered in
[docs/engine/index.md](../engine/index.md)).

| Mechanism | Owning design |
|---|---|
| Per-frame draw order, instanced batching, multi-draw indirect, GPU timer brackets | [Vulkan Renderer § Per-frame draw](../engine/renderer.md#per-frame-draw) (its step 5 places `build_render_data` inside `draw_frame`; at HEAD on 2026-11-01 it runs before it, §3.2 row 1) |
| Frame-in-flight fences and the all-slots wait | [Vulkan Renderer § Sync model](../engine/renderer.md#sync-model) |
| TLAS build / refit, BLAS residency, instance identity | [Vulkan Renderer § RT acceleration structures](../engine/renderer.md#rt-acceleration-structures), [Memory Budget § Acceleration Structures](../engine/memory-budget.md#acceleration-structures-blas--tlas) |
| Pass order, `GpuInstance` layout, descriptor sets, capacity constants | [Shader Pipeline](../engine/shader-pipeline.md) |
| Instance / indirect / material SSBO sizing, global geometry SSBO | [Memory Budget § Scene Buffers](../engine/memory-budget.md#scene-buffers-per-frame-ssbos--ubos), [Memory Budget § Mesh Registry](../engine/memory-budget.md#mesh-registry) |
| CPU-side parallel dispatch the milestone builds on (R7 / M27) | [M27 Phase 1 Survey](../engine/m27-phase1-survey.md) |
| Canonical mesh data the meshlet builder consumes; per-game translate step | [NIFAL](../engine/nifal.md), [Asset Pipeline](../engine/asset-pipeline.md) |
| The one shipped GPU-generated indirect draw (ground cover) | [EXAL Ground Cover § 12.15](../engine/exal-groundcover.md#1215-the-2026-10-overhaul-curve-blades-chunk-lod-terrain-matched-light-authored-density) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| `GpuInstance` size and its GLSL mirror set; no growth of the lockstep surface (`MDL-GPU-001`, `MDL-GPU-005`) | [Material and Decal Layering PRD § GPU contract](material-decal-layering.md#51-gpu-contract-and-the-r1-budget-mdl-gpu) |
| Wanted-mip signal read from the raster draw list and TLAS membership (`TXS-WANT-002`, `TXS-WANT-004`) | [Texture Streaming PRD § Wanted mip](texture-streaming.md#54-wanted-mip-txs-want) |
| Path-traced reference: primary visibility by camera rays, no raster draw (`PTR-MODE-002`) | [Path-Tracing Reference Mode PRD](path-tracing-reference-mode.md#51-mode-and-accumulation-ptr-mode) |
| M53 cluster format, its bundle section (`BAKE-NG-006`) | [Virtual Geometry PRD](virtual-geometry.md); [Asset Preprocessing PRD](asset-preprocessing.md) |
| Cull predicate, instance-scene delta protocol, meshlet layout, path selection | `docs/engine/gpu-driven-rendering.md` (Phase 0 writes it) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `GDR-` prefix.

---

## 1. Product thesis

Tier 11 is the performance ceiling: no new visuals, no new gameplay
surface. Its rule, in the ROADMAP's words, is that its milestones "start
only when a real-content bench names them as the bottleneck"
([Tier 11](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon)).
The same paragraph names the two facts that make M52 the candidate lever:
`build_render_data` walks every entity every frame and scales with cell
complexity, and the live bench-of-record shows the FO4 scenes fence-bound
from their current spawn views
([Bench-of-record](../../ROADMAP.md#bench-of-record-live--stepped-camera-refresh-2026-10-22-head-d9a201c71),
[Known Issues § Performance](../../ROADMAP.md#performance-and-measurement),
R6a-regress-22 and #5365).

Those two facts pull in different directions, and the PRD exists to keep
them apart. A fence-bound frame is a frame where the CPU waits for the GPU;
removing CPU draw-submission work does not shorten it. At HEAD the wait is
also over *every* frame slot (`sync_and_acquire_frame.rs:83`, #5365), so
the CPU work recorded after it runs in series with the GPU instead of
overlapping it; a CPU saving there does show up in wall time. Which of the
two regimes a scene is in, and which CPU or GPU stage carries the frame,
is a measurement, not a reading of the milestone row. M52 as written
bundles three levers with different costs and different beneficiaries:

1. **GPU culling and GPU-written draw counts** remove the CPU frustum test,
   sort and batch work from the per-frame path. They run on any device the
   engine already accepts, provided the Vulkan 1.2 `drawIndirectCount`
   feature is present.
2. **A persistent instance scene** removes the per-frame rebuild of every
   instance from ECS components, which is the cost that scales with entity
   count.
3. **Mesh and task shaders** move work off the GPU's fixed-function
   front end (vertex fetch, primitive assembly, back-facing and off-screen
   triangles). They help only a frame whose GPU time is in that front end,
   and only on devices with `VK_EXT_mesh_shader`.

The corollary that shapes every requirement below: **each lever ships only
when a pinned, same-machine bench names the cost it removes, and the CPU
path it replaces stays as the reference it is measured against.** A Phase 0
that finds no scene where a lever's cost dominates is a successful Phase 0;
the lever's phase is then not started, and §9 records that outcome.

What the engine will *not* do under M52: cull the TLAS by the camera
frustum (decided in #516 and kept by `visibility::ray_exclusion`), ship a
second renderer without ray tracing, branch per game in the renderer,
reorder or compact the instance SSBO behind the TLAS's back, or add
occlusion culling or LOD selection.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **CPU-driven path** | The raster submission path at HEAD: `build_render_data` produces `DrawCommand`s, the CPU frustum-tests, sorts (`draw_sort_key`), batches (`DrawBatch`) and writes `VkDrawIndexedIndirectCommand`s (§3.2). |
| **GPU-culled path** | The same vertex/fragment pipelines, fed by indirect commands and draw counts that a compute pass writes after testing every instance on the GPU. |
| **Mesh path** | Task + mesh shaders drawing meshlets (`vkCmdDrawMeshTasksIndirectEXT` family) in place of `triangle.vert` for eligible draws. |
| **Render path** | Which of the three paths a frame used; one value per frame (§3.1). |
| **Raster-visible** | The repository's `in_raster` predicate: the draw's `WorldBound` sphere intersects the view frustum, or it has no bound yet (`visibility::raster_visible`). |
| **TLAS member** | The repository's `in_tlas` predicate: not excluded by `tlas_exclusion` (distant LOD block, decal surface, fire refraction, or the off-frustum diagnostic). Independent of raster visibility by design (#516). |
| **Instance scene** | The GPU-resident array of `GpuInstance` records plus the per-instance cull inputs (bound, draw group), updated by deltas from the ECS instead of rebuilt every frame. |
| **Draw group** | The key the CPU path merges batches on today (`group_state`, `context/frame_params.rs:1892`): pipeline key, render layer, two-sided, depth test / write / compare and the two-sided-blend split eligibility. The CPU path merges each *consecutive run* of equal keys into one indirect call, so one key can appear in several runs; a GPU-written indirect range is one key. |
| **Meshlet** | A cluster of at most `max_vertices` vertices and `max_triangles` triangles cut from one canonical mesh, with a bounding sphere and a normal cone (apex, axis, cutoff), in the sense of meshoptimizer's `meshopt_buildMeshlets` / `meshopt_computeMeshletBounds` ([meshoptimizer README](https://github.com/zeux/meshoptimizer)). |
| **Cone culling** | Rejecting a whole meshlet whose triangles are all back-facing from the camera, by the normal-cone test of the meshoptimizer README (§11). |
| **Fence-bound** | A frame whose `fence=` token is the largest CPU term of the `bench:` line, i.e. the CPU waited on the GPU. |
| **Front-end share** | The part of `gpu_main_render` that does not shrink with render resolution: geometry work, as opposed to per-pixel shading (§8, `GDR-EV-003`). |
| **Phase 0 evidence** | The checked-in attribution table of §8 that names, per bench scene, which lever (if any) addresses the dominant cost. |

---

## 3. Core model

### 3.1 Render paths

| Path | Selected when | Culling | Draw source | Reported as |
|---|---|---|---|---|
| `cpu` | Fallback: `drawIndirectCount` absent, or forced by the selector (`GDR-FB-002`) | CPU sphere test (`raster_visible`) | CPU-written indirect commands, CPU draw count | `render_path=cpu` on the `bench:` line and in `gdr.stats` |
| `gpu-cull` | Default once Phase 2 exits, on every device with `drawIndirectCount` | Compute pass over the instance scene, same predicate | GPU-written indirect commands and per-group draw counts | `render_path=gpu-cull` |
| `mesh` | Opt-in (Q2), device exposes `VK_EXT_mesh_shader` with `taskShader` and `meshShader` | Compute instance cull, then task-shader meshlet frustum + cone cull | `vkCmdDrawMeshTasksIndirectCountEXT` per draw group; ineligible draws fall back to `gpu-cull` in the same frame | `render_path=mesh` plus the eligible / fallen-back draw counts |

A frame uses exactly one render path. The `cpu` path is never removed
(`GDR-NG-004`).

### 3.2 Per-frame stages at HEAD

The inventory below is the state at HEAD on 2026-11-01. It is
hand-written; Phase 0's design replaces it with the stage table of
`docs/engine/gpu-driven-rendering.md`, and each later phase updates that
table in the commit that moves a stage.

| # | Stage | Runs on | Today |
|---|---|---|---|
| 1 | ECS walk to `DrawCommand`s | CPU, rayon-joined with lights and fog | `build_render_data` (`byroredux/src/render/mod.rs:1040`) calls `static_meshes::collect_static_mesh_draws` (`static_meshes.rs:1352`) over every `MeshHandle` entity each frame. It is called at `app_frame.rs:392`, before `ctx.draw_frame` at `app_frame.rs:735` (and so before the row-13 fence wait), so it overlaps the GPU's previous frame. [Vulkan Renderer § Per-frame draw](../engine/renderer.md#per-frame-draw) step 5 still lists it inside `draw_frame`. |
| 2 | Raster frustum test | CPU | `visibility::raster_visible` (`visibility.rs:30`): `WorldBound` sphere against six planes; no bound means visible. `BYRO_NO_CULL=1` disables it (`static_meshes.rs:467`). Unit tests in `byroredux/src/render/frustum_tests.rs`. |
| 3 | TLAS membership | CPU, same loop | `visibility::ray_exclusion` → `tlas_exclusion`; off-frustum draws stay TLAS members unless `frustum_only_rt` is set (`static_meshes.rs:1446`). Counted per reason in `TlasPolicyCounts` (the `tlas-policy:` bench row). |
| 4 | Draw sort | CPU | `draw_sort_key` (`render/mod.rs:756`), `par_sort_unstable_by_key` above a size threshold (`render/mod.rs:954`); tests in `draw_sort_key_tests.rs`. |
| 5 | Instance SSBO and batching | CPU, after the fence wait | `build_and_upload_instances` (`context/build_and_upload_instances.rs:288`) writes one `GpuInstance` per `DrawCommand`, including off-frustum TLAS members, and merges consecutive same-mesh draws in `push_or_extend_batch` (`:1079`); draws with `in_raster == false` or water get a slot but no batch (`:415`). Counted in the `ssbo=` bench token. |
| 6 | Indirect commands | CPU-written | `upload_indirect_draws` (`scene_buffer/upload.rs:977`), content-hash dirty-gated; one `cmd_draw_indexed_indirect` per consecutive run of equal `group_state` keys (`context/geometry_pass.rs:527-541`); two-sided alpha-blend batches leave the grouping and draw direct, back faces then front (`:496-513`); all of it used only when `indirect_draws_supported()` (multiDrawIndirect + drawIndirectFirstInstance, `device.rs:275`). |
| 7 | Draw count read by the GPU | — | None. `drawIndirectCount` is not in the enabled `PhysicalDeviceVulkan12Features` chain (`device.rs:936`); no `*IndirectCount` call exists in `crates/renderer/src`. |
| 8 | GPU-generated indirect draws | GPU | Ground cover only: `groundcover_scatter.comp` writes one `VkDrawIndirectCommand` per chunk and `groundcover/frame.rs:635` draws them with `cmd_draw_indirect`. |
| 9 | Mesh / task shaders | — | None: no `VK_EXT_mesh_shader` reference in `crates/` or `byroredux/`. [EXAL Ground Cover § 12.15](../engine/exal-groundcover.md#1215-the-2026-10-overhaul-curve-blades-chunk-lod-terrain-matched-light-authored-density) item (d) records the decision not to build one for ground cover. |
| 10 | Meshlets | Parsed, unused | Starfield `BSGeometryMeshData` meshlets and per-meshlet cull data are parsed (`crates/nif/src/blocks/bs_geometry.rs:314`, `Meshlet` at `:340`) and not read by `crates/nif/src/import/mesh/bs_geometry.rs`. No other block parser in `crates/nif/src/blocks/` has a meshlet field. |
| 11 | Occlusion culling | — | None: no Hi-Z or occlusion-cull code in `crates/renderer/src` or `byroredux/src`. |
| 12 | TLAS build | CPU-assembled, GPU-built | `AccelerationManager::build_tlas` (`acceleration/tlas.rs:48`) rebuilds or refits one TLAS per frame slot from the `DrawCommand` slice; `instance_custom_index` comes from `instance_map` so it equals the instance SSBO index (#419). Counted in `cpu_tlas_ms=` and `gpu_tlas_build=`. |
| 13 | Frame fence | CPU | `wait_for_fences(&in_flight, true, …)` over all slots (`context/sync_and_acquire_frame.rs:83`); the argument is pinned by `the_all_slots_wait_argument_is_pinned` (`vulkan/sync.rs:790`). Narrowing it is #5365. |
| 14 | `cluster_cull.comp` | GPU | Assigns lights to froxel clusters (shader header comment); it does not cull geometry. |
| 15 | Device floor | — | RT is mandatory: `is_device_suitable` returns no device when ray query is missing (`device.rs:582`, #3759). Instance API version 1.3 (`instance.rs:48`). |
| 16 | ECS change signals | CPU | `Component::TRACK_CHANGES` (`crates/core/src/ecs/storage.rs:20-32`): a per-entity dirty set exists only in `PackedStorage` (`packed.rs:61`, `take_dirty` / `drain_dirty_into`, destructive); `SparseSetStorage` keeps only a structural generation counter (`sparse_set.rs:36-44`). Tracked at HEAD: `Transform`, `GlobalTransform`, `Parent` / `Children` and a few others; `MeshHandle` is not (`components/mesh.rs:14`), and the material component's counter sees inserts and removes but not in-place field writes (`components/material.rs:865-872`). `GlobalTransform`'s dirty set is already drained by world-bound propagation, and its comment records that a second consumer needs a fan-out resource (`components/global_transform.rs:154-160`). |

### 3.3 Draw eligibility for the mesh path

| Draw class | Mesh path | Why |
|---|---|---|
| Opaque or alpha-tested static mesh, rigid transform | Eligible | Meshlet bounds and cones are valid in object space under a rigid transform. |
| Skinned or morph-target mesh | Not eligible in M52; drawn by `gpu-cull` | Deformation invalidates precomputed cones and bounds (`GDR-MS-005`). |
| Alpha-blended draw | Not eligible; drawn by `gpu-cull` | Blend order comes from `draw_sort_key`, which a per-meshlet emission order does not preserve. |
| Two-sided draw | Eligible without cone culling | A back-facing meshlet of a two-sided surface is visible. |
| Terrain, water, ground cover, particles, UI, sky | Out of scope | Each has its own pipeline (§2 of the Scope). |

---

## 4. Goals and non-goals

### 4.1 Goals

- **GDR-G-001 — The bottleneck is named before code.** Every bench-of-record
  scene has a Phase 0 attribution row that names the dominant frame cost and
  the lever (or "none") that addresses it.
- **GDR-G-002 — CPU render cost stops scaling with entity count.** On the
  `gpu-cull` path with the instance scene, the CPU render-side cost
  (§8, *CPU render ms*) of a static view no longer grows with the number of
  drawable entities in the cell.
- **GDR-G-003 — No frame gets slower.** No bench-of-record scene's wall
  frame time regresses beyond the same-machine control's run range on the
  path selected by default.
- **GDR-G-004 — Same image.** The `gpu-cull` and `mesh` paths produce the
  same raster-visible set as the `cpu` path, and the same image within the
  temporal-noise envelope, on every bench scene.
- **GDR-G-005 — RT unaffected.** TLAS membership and `instance_custom_index`
  values are identical across render paths.

### 4.2 Non-goals

- **GDR-NG-001.** Occlusion culling (depth pyramid, two-phase culling as in
  Haar & Aaltonen 2015). The ROADMAP row promises frustum and cone culling
  only; occlusion culling would be a new promise (README trigger T1).
- **GDR-NG-002.** LOD selection, cluster hierarchies and virtual geometry.
  Owned by M53, which depends on M52
  ([Tier 11](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon)).
- **GDR-NG-003.** Frustum-culling the TLAS. Off-frustum geometry occludes
  and reflects for visible pixels; the split raster / TLAS predicate is
  #516's decision and `visibility::ray_exclusion`'s documented contract.
- **GDR-NG-004.** Removing the CPU-driven path. It is the fallback on
  devices without `drawIndirectCount` and the parity reference (Q3).
- **GDR-NG-005.** An RT-less fallback. RT is mandatory at HEAD (§3.2 row 15)
  and "RT-first" is on the [keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)
  (whose row still reads "RT-first, rasterized fallback"; this PRD does not
  read that as an RT-less renderer);
  "falls back to the current path" in the ROADMAP row means the CPU-driven
  raster path.
- **GDR-NG-006.** Moving the water, sky, ground-cover, particle or UI
  passes onto the new paths. Ground cover already generates its draws on
  the GPU (§3.2 row 8).
- **GDR-NG-007.** Per-game render paths. The cull inputs and meshlets are
  built from canonical data; any per-game difference lives in the
  translate step ([NIFAL](../engine/nifal.md)).

---

## 5. Requirements by area

### 5.1 Evidence (`GDR-EV`)

- **GDR-EV-001.** Phase 0 MUST run the bench-of-record matrix
  (`scripts/fsr-bench-matrix.sh 3 300`) at the HEAD it measures, with the
  same-machine control and pinned camera the
  [bench methodology](../../ROADMAP.md#how-to-run-a-bench) requires, and
  check in the raw TSV under `docs/audits/` named `BENCH_m52-phase0_<hash>.tsv`.
  At HEAD on 2026-11-01 the TSV header (`scripts/fsr-bench-matrix.sh:214`)
  carries `wall_ms`, `fence_ms`, `brd_ms`, `gpu_main`, `entities` and
  `draws` but not `ssbo`, `cpu_tlas_ms`, `cmd` or `render_extent`, although
  the `bench:` line prints all four (`byroredux/src/app_events.rs:292-308`);
  Phase 0 MUST append those four as TSV columns, after every existing
  column, before the run (a harness change, not engine code).
- **GDR-EV-002.** The Phase 0 attribution table MUST give, per scene and
  per upscaler config, the CPU pre-fence ms, the CPU post-fence ms, the
  fence share and the front-end share as defined in §8, each computed from
  TSV columns of `GDR-EV-001`'s file.
- **GDR-EV-003.** The front-end share MUST be estimated from the
  difference in `gpu_main_render` between the `taa` (native) and FSR
  Performance configs of the same run set (both in the matrix's `CONFIGS`,
  `fsr-bench-matrix.sh:67-72`), with the pixel ratio taken from the
  `render_extent` column of `GDR-EV-001`; the method's assumption (per-pixel work scales with render
  extent, geometry work does not) MUST be stated next to the number.
- **GDR-EV-004.** Phase 0 MUST run each scene once more with
  `BYRO_NO_CULL=1` and report the change in `gpu_main_render` and in the
  `draws=` token, as the measured sensitivity of the frame to submitted
  geometry.
- **GDR-EV-005.** Phase 0 SHOULD capture one GPU frame per FO4 scene with
  an external profiler (RenderDoc or NVIDIA Nsight Graphics) and record
  the vertex / primitive / fragment split of the opaque geometry range,
  as a cross-check of `GDR-EV-003`.
- **GDR-EV-006.** The attribution table MUST state, per scene, which lever
  of §1 addresses the dominant cost, or "none", and MUST be re-measured
  after #5365 lands if it lands before Phase 1 starts (Q1).
- **GDR-EV-007.** The attribution table MUST carry each scene's
  `entities` column value, so that Phase 4 can replace the hand-written
  entity figure in the ROADMAP M52 row with a link to it; at HEAD on
  2026-11-01 that figure and the bench-of-record's MedTek entity count do
  not agree.

### 5.2 Path selection and fallback (`GDR-FB`)

- **GDR-FB-001.** The renderer MUST select the render path once at device
  creation from probed capabilities and log the choice and the reason in
  one line beginning `render path:`.
- **GDR-FB-002.** A selector (Q3) MUST force any path the device supports,
  and forcing an unsupported path MUST fall back to the best supported
  one with a logged warning, never fail device creation.
- **GDR-FB-003.** The `cpu` path MUST remain selectable and MUST keep
  passing its existing tests (`draw_sort_key_tests`, `frustum_tests`,
  `sort_key_tests`) unchanged by any later phase.
- **GDR-FB-004.** The `bench:` line MUST carry a `render_path=` token, so
  every TSV row records which path produced it.
- **GDR-FB-005.** The renderer MUST enable `drawIndirectCount` when the
  device reports it; today it is not enabled (§3.2 row 7). Whether every
  device that passes the ray-query suitability check also reports it is
  **Phase 0: verify** (`GDR-V-003`).

### 5.3 GPU culling (`GDR-CULL`)

- **GDR-CULL-001.** A compute pass MUST evaluate, for every instance in
  the instance scene, the same predicate as `visibility::raster_visible`:
  the `WorldBound` sphere against the six frustum planes, a missing bound
  or one with radius ≤ 0 counting as visible, and ground-cover templates
  never culled (`visibility.rs:30-44`).
- **GDR-CULL-002.** A `cargo test` MUST pin the shader predicate to the
  Rust one (new test `gpu_cull_predicate_matches_raster_visible`): it
  evaluates `raster_visible` and a line-for-line Rust transcription of the
  GLSL function over generated spheres and planes, and fails when the GLSL
  function's text changes without the transcription, in the source-shape
  style `shader_contract_tests.rs` already uses for struct layouts.
- **GDR-CULL-003.** The cull pass MUST NOT change TLAS membership: the
  `tlas=` bench token and every `tlas-policy:` count MUST be equal on the
  `cpu` and `gpu-cull` paths for the same pinned frame.
- **GDR-CULL-004.** The cull pass MUST reference instances by their
  existing instance-SSBO index; it MUST NOT reorder or compact the instance
  SSBO, because the TLAS's `instance_custom_index` is that index (#419,
  §3.2 row 12).
- **GDR-CULL-005.** The cull pass MUST honour `BYRO_NO_CULL=1` as the CPU
  path does, so the existing debug bisect keeps one meaning.
- **GDR-CULL-006.** A debug readback MUST report the number of instances
  whose GPU verdict differs from the CPU verdict for the same frame; the
  pass rule is zero on every bench scene (`gdr.stats`, `GDR-TEL-002`).
- **GDR-CULL-007.** The cull pass SHOULD carry its own GPU timer bracket,
  appended to `BENCH_GPU_KEYS` under the "appended last" rule
  (`byroredux/src/main.rs`), as `instance_cull`.

### 5.4 GPU-written draws (`GDR-IND`)

- **GDR-IND-001.** The cull pass MUST write one `VkDrawIndexedIndirectCommand`
  per surviving instance range into a per-draw-group region of the indirect
  buffer, and the per-group draw count into a count buffer read by
  `vkCmdDrawIndexedIndirectCount`
  ([Vulkan refpage](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdDrawIndexedIndirectCount.html)).
- **GDR-IND-002.** Draw groups MUST be formed by the same key the CPU path
  merges on (`group_state`), so pipeline binds and dynamic state per frame
  are no more than on the `cpu` path for the same view.
- **GDR-IND-003.** Within the opaque and alpha-tested groups, the GPU path
  MAY emit draws in an order different from `draw_sort_key`; within
  alpha-blended groups it MUST preserve `draw_sort_key` order, or keep
  blended draws on CPU-written commands (design choice, recorded in the
  Phase 0 design). Two-sided alpha-blend batches MUST keep their
  back-then-front direct draws (`geometry_pass.rs:496-513`) on every path.
- **GDR-IND-004.** The indirect region per group MUST be sized so that a
  frame with every instance raster-visible fits; overflow MUST clamp and
  log, as `upload_indirect_draws` does today for `MAX_INDIRECT_DRAWS`.
- **GDR-IND-005.** The CPU MUST NOT read back the count buffer on the
  frame that produced it; the `draws=` bench token on GPU paths MUST come
  from a readback at least one frame-in-flight later and say so in its
  name or in `gdr.stats`.
- **GDR-IND-006.** On GPU paths the per-instance raster verdicts MUST stay
  readable by CPU consumers through the same delayed readback, because the
  texture-streaming PRD's wanted-mip rules key on the raster draw list
  (`TXS-WANT-002`, `TXS-WANT-004`); the Phase 0 design MUST state the
  latency in frames that consumer sees.

### 5.5 Instance scene (`GDR-SCN`)

- **GDR-SCN-001.** The instance scene MUST hold a `GpuInstance` and the
  cull inputs for every entity the CPU path would emit a `DrawCommand` for,
  including off-frustum TLAS members, keyed by entity.
- **GDR-SCN-002.** On a frame where no drawable entity changed transform,
  material, mesh, visibility marker or spawn state, the CPU MUST upload
  zero instance bytes, and the drawable-entities-iterated count of
  `gdr.stats` MUST read zero.
- **GDR-SCN-003.** A change to one entity MUST upload that entity's records
  only; the measured upload bytes per frame MUST be reported (`gdr.stats`).
- **GDR-SCN-004.** The change signal MUST come from the ECS's
  `TRACK_CHANGES` mechanism (§3.2 row 16), extended where it falls short;
  the Phase 0 design MUST list, per component that feeds an instance
  record, whether it is tracked per entity, tracked structurally only, or
  untracked at HEAD, and which it needs. Because the `GlobalTransform`
  dirty set is drained destructively by world-bound propagation, the
  design MUST name the fan-out that lets the instance scene read it too.
- **GDR-SCN-005.** Instances MUST be removed from the scene in the frame
  their entity despawns or loses its `MeshHandle`, and freed slots MUST NOT
  be reused until every frame in flight that could read them has retired
  ([Memory Budget § Deferred-Destroy Queue](../engine/memory-budget.md#deferred-destroy-queue)).
- **GDR-SCN-006.** A full rebuild of the instance scene from the ECS MUST
  produce byte-identical records to the delta-maintained scene for the
  same world state (new test `instance_scene_delta_matches_full_rebuild`).
- **GDR-SCN-007.** Per-frame animated inputs (animated visibility, alpha,
  colour, UV and shader controllers; skinned palettes) MUST reach the
  instance scene on the frame they change; the Phase 0 design MUST list
  them, starting from the optional queries in `StaticDrawQueries`
  (`static_meshes.rs`).

### 5.6 Mesh path (`GDR-MS`)

- **GDR-MS-001.** Meshlets MUST be generated once per canonical mesh at
  mesh registration, from the same vertex and index data that the global
  geometry SSBO holds, with per-meshlet bounding sphere and normal cone.
- **GDR-MS-002.** Meshlet limits MUST NOT exceed the device's
  `maxMeshOutputVertices` and `maxMeshOutputPrimitives`
  ([VK_EXT_mesh_shader proposal](https://docs.vulkan.org/features/latest/features/proposals/VK_EXT_mesh_shader.html));
  the starting values SHOULD come from meshoptimizer's guidance
  ([README](https://github.com/zeux/meshoptimizer)): 64 vertices and 126
  triangles, which it gives as NVIDIA's recommendation; 64 / 96, which it
  suggests as more realistic limits that reduce overhead on other GPUs; and
  equal limits (64 / 64 or 128 / 128), which it reports as best on earlier
  AMD GPUs. Phase 3's sweep picks the per-vendor values.
- **GDR-MS-003.** The task shader MUST reject meshlets outside the frustum
  and, for single-sided draws, meshlets whose normal cone faces away from
  the camera, rejecting when `dot(normalize(apex − camera), axis) ≥ cutoff`,
  the test the meshoptimizer README gives.
- **GDR-MS-004.** The mesh shader MUST produce the same G-buffer outputs as
  `triangle.vert` → `triangle.frag` (position, normal, tangent, UVs,
  motion vector inputs, instance index), verified by the image-parity
  scenario of §7.2.
- **GDR-MS-005.** Skinned, morph-target and alpha-blended draws MUST stay on
  `gpu-cull` in the same frame (§3.3); the count of draws that fell back
  MUST appear in `gdr.stats`.
- **GDR-MS-006.** The mesh path MUST be selected only when the device
  enables both `taskShader` and `meshShader`; `multiviewMeshShader` and
  `primitiveFragmentShadingRateMeshShader` MUST NOT be required. Whether
  `vkCmdDrawMeshTasksIndirectCountEXT` additionally needs the
  `drawIndirectCount` feature is **Phase 0: verify** (`GDR-V-007`).
- **GDR-MS-007.** The mesh path MUST NOT start unless the Phase 0 evidence
  names front-end work as a dominant cost in at least one bench scene (Q2).
- **GDR-MS-008.** Meshlet data MUST be counted in the memory ledger
  ([Memory Budget](../engine/memory-budget.md)) as a new Mesh Registry row
  in the commit that allocates it.

### 5.7 Ray tracing interaction (`GDR-RT`)

- **GDR-RT-001.** The TLAS MUST be built from the same membership on every
  render path (`GDR-CULL-003`); in M52 its instance list stays assembled on
  the host by `build_tlas` unless Q4 decides otherwise.
- **GDR-RT-002.** The CPU TLAS cost (`cpu_tlas_ms=`) MUST be reported in
  the Phase 0 table beside the raster CPU cost, because removing raster
  submission from the CPU leaves it in place.
- **GDR-RT-003.** If Q4 moves TLAS instance writes to the GPU, inactive
  slots MUST be written with instance `mask = 0`, which no ray can hit
  ("The instance may only be hit if Cull Mask & instance.mask != 0",
  [VkAccelerationStructureInstanceKHR](https://docs.vulkan.org/refpages/latest/refpages/source/VkAccelerationStructureInstanceKHR.html)),
  so the host-supplied primitive count stays valid without
  `vkCmdBuildAccelerationStructuresIndirectKHR`.
- **GDR-RT-004.** A render path MUST NOT change the RT shaders' view of
  `GpuInstance` (`instance_custom_index` → instance SSBO index), checked by
  the `tlas_eligible`, `tlas_emitted`, `missing_skinned`, `missing_rigid`,
  `missing_ssbo` and `verdict` fields of the `rt-integrity:` line
  (`RtIntegrityStats::machine_line`, `crates/core/src/ecs/resources/mod.rs:1103`)
  being equal across paths on a pinned frame; its frame counter and
  pending-destroy fields are excluded.

### 5.8 Shader contracts (`GDR-SH`)

- **GDR-SH-001.** A new shader that reads `GpuInstance` MUST obtain it by
  `#include "include/gpu_instance.glsl"` and MUST NOT declare its own copy,
  so the lockstep surface the material-decal PRD pins (`MDL-GPU-001`) does
  not grow. The header is struct-only, so no binding layout rules it out;
  `shared_gpu_structs_are_declared_once` fails on a second declaration.
- **GDR-SH-002.** Any new cull-input or meshlet struct shared between Rust
  and GLSL MUST have a size-and-offset test in `shader_contract_tests.rs`
  and use only scalar and vec4-equivalent fields, per the `GpuInstance`
  std430 rule (`scene_buffer/gpu_types.rs`).
- **GDR-SH-003.** Every new pass MUST appear in
  [Shader Pipeline § Per-Frame Submission Order](../engine/shader-pipeline.md#per-frame-submission-order)
  and every new buffer in
  [Memory Budget § Scene Buffers](../engine/memory-budget.md#scene-buffers-per-frame-ssbos--ubos)
  in the commit that adds it.
- **GDR-SH-004.** New barriers MUST be validated with `BYRO_VALIDATION=1`
  on both upscaler modes on at least one game scene before the phase
  exits, and a barrier change that cannot be validated that way MUST be
  checked in a RenderDoc capture (the speculative-Vulkan rule).
- **GDR-SH-005.** Cull inputs and meshlet descriptors MUST live in their
  own buffers and MUST NOT add a field to `GpuInstance`, which stays at the
  size `MDL-GPU-001` pins.

### 5.9 Telemetry (`GDR-TEL`)

- **GDR-TEL-001.** The `bench:` line MUST keep every existing token and
  append new ones only (`render_path=`, `instance_cull` bracket,
  instance-scene upload bytes), so the existing extractors and the
  bench-of-record TSVs stay comparable.
- **GDR-TEL-002.** A console command `gdr.stats` MUST print, for the last
  completed frame: render path, instances in the scene, instances
  raster-visible, drawable entities iterated by the CPU, CPU / GPU verdict
  mismatches, draws issued per draw group, mesh-path eligible and
  fallen-back draws, and instance-scene upload bytes.
- **GDR-TEL-003.** `gdr.stats` MUST be reachable through `byro-dbg`
  against a `--bench-hold` engine, as every existing console command is.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The Gamebryo reference engine culls on the CPU into a per-camera visible set (`NiCullingProcess::Cull` filling an `NiVisibleArray`, Gamebryo 3.2 `Include/NiCullingProcess.h:52-56`, `/mnt/data/src/reference/gamebryo-v32`); shipped titles' engine forks are not in the reference set | Visibility of the main raster pass is decided on the GPU (`GDR-CULL-001`) | The CPU frustum test in `collect_static_mesh_draws` on GPU paths | A predicate drift between GLSL and Rust culls visible geometry → `GDR-CULL-002`, `GDR-CULL-006` |
| 2 | Bethesda content ships per-shape geometry; only Starfield ships meshlets and per-meshlet cull data: nifly's only meshlet structs are `BSGeometryMeshData::Meshlet` / `CullData` ([nifly `Geometry.hpp`, `BSGeometryMeshData::Sync`](https://github.com/ousnius/nifly)), and nif.xml (`/mnt/data/src/reference/nifxml/nif.xml`) defines no meshlet field | One meshlet builder for every game (Q5); Starfield's authored meshlets stay unused | Starfield's authored meshlet and cull-data arrays | Authored meshlets may be better tuned than ours → Q5 alternative; `GDR-V-004` |
| 3 | Redux at HEAD draws every raster-visible batch in `draw_sort_key` order | Opaque GPU-written draws may run in a different order (`GDR-IND-003`) | — | Z-fighting or decal-order changes on coplanar opaque geometry → image parity, §7.2 scenario 4 |
| 4 | Redux at HEAD rebuilds every `GpuInstance` per frame | Delta-maintained instance scene (`GDR-SCN-002`) | The per-frame instance fill in `build_and_upload_instances` on GPU paths | A missed change signal leaves a stale transform or material on screen → `GDR-SCN-006`, `GDR-SCN-007` |
| 5 | Redux at HEAD knows the raster draw list on the CPU in the frame it is built | On GPU paths the CPU learns it from a delayed readback (`GDR-IND-005`) | — | Texture streaming's on-screen-first ordering lags a fast turn by the readback latency → `GDR-IND-006` |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

M52 is game-neutral: it consumes canonical `DrawCommand`s, `WorldBound`s
and registered meshes, all produced after the translate step. A title is
in scope where it has a rendering cell path; it is verified on the
bench-of-record scenes, and the others are covered by their existing scene
loads.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | Yes | Interior cells render through the shared `cell_loader` (ROADMAP Status). Wave 2: checked by one interior image-parity capture, no bench scene. |
| Fallout 3 | Yes | Interiors and exteriors render (Megaton). Wave 2, same check. |
| Fallout: New Vegas | Yes | Wave 1: Prospector Saloon is a bench-of-record scene. |
| Skyrim (LE / SE / AE) | SE / AE yes; LE n/a | SE is Wave 1 (Whiterun BanneredMare bench scene). LE is parse-only in the [compatibility matrix](../../ROADMAP.md#compatibility-matrix), so there is nothing to render. |
| Fallout 4 | Yes | Wave 1, and the reason the milestone exists: MedTek Research 01 and Dugout Inn are the high-entity bench scenes. |
| Fallout 76 | n/a | Parse-only; no cell loader ([Game Compatibility](../engine/game-compatibility.md)). In scope by construction once it renders. |
| Starfield | Yes | Wave 2: the walkable Cydonia interior; it is also the only title with authored meshlets (Q5, `GDR-V-004`). |

### 7.2 Acceptance

Data: the bench-of-record installs (FNV, Skyrim SE, FO4) under
`BYROREDUX_GAMES_ROOT`, plus the redistributable Cornell scene
(`--cornell`) that needs no game data. Wave-2 checks need Oblivion, FO3 and
Starfield installs. Nothing from game data is checked in; TSVs and
attribution tables hold numbers only.

A scenario passes when:

1. **Evidence (Phase 0).** `scripts/fsr-bench-matrix.sh 3 300` and its
   same-machine control produce zero rejected rows on every scene of the
   matrix's default `SCENES` list (`fsr-bench-matrix.sh:200`), and
   the attribution table names a lever or "none" for each (`GDR-EV-001`
   to `GDR-EV-006`).
2. **Predicate parity.** `cargo test -p byroredux --bin byroredux
   gpu_cull_predicate_matches_raster_visible` and the existing
   `frustum_tests` pass (`GDR-CULL-002`).
3. **Set parity, live.** With `BYRO_DEBUG_SERVER=1 cargo run --release --
   --game <profile> --cell <bench cell> --camera-pos … --camera-forward …
   --bench-frames 300 --bench-hold`, then `byro-dbg` → `gdr.stats`,
   mismatches read zero and the `tlas=` and `tlas-policy:` values equal a
   `cpu`-path run of the same pose (`GDR-CULL-003`, `GDR-CULL-006`,
   `GDR-RT-004`).
4. **Image parity.** New script `docs/smoke-tests/m52-gpu-driven.sh`
   (written in Phase 2, indexed in [the smoke-test README](../smoke-tests/README.md))
   captures the same pinned pose on `cpu` and on each supported GPU path
   with the debug server's `screenshot`, and passes when the per-pixel
   difference is within twice the run-to-run difference of two `cpu`
   captures, and `r.health` reports no non-finite pixels.
5. **Instance scene.** `cargo test … instance_scene_delta_matches_full_rebuild`
   passes, and `gdr.stats` reports zero upload bytes on a static pinned
   frame of every bench scene (`GDR-SCN-002`, `GDR-SCN-006`).
6. **Shader contracts.** `cargo test -p byroredux-renderer
   shared_gpu_structs_are_declared_once` passes (`GDR-SH-001`).
7. **Performance.** The bench matrix on the default path, against a
   same-machine `cpu`-path control, shows no wall-time regression beyond
   the control's run range on any scene (`GDR-G-003`) and the phase's
   target metric of §8 moved in the direction its exit names.

---

## 8. Metrics

The numbers live in ROADMAP: Phase 0 adds an *M52 evidence* table under
the [Bench-of-record](../../ROADMAP.md#bench-of-record-live--stepped-camera-refresh-2026-10-22-head-d9a201c71)
section, and each phase exit adds a row to it; raw rows stay in
`docs/audits/BENCH_*.tsv`.

| Metric | Definition | Granularity |
|---|---|---|
| CPU pre-fence ms | `brd_ms`: CPU render work that runs before the frame fence and overlaps the GPU (§3.2 rows 1–4), median of runs | Per scene × upscaler config × render path |
| CPU post-fence ms | `ssbo + cpu_tlas_ms + cmd`: CPU render work after the fence, serial with the GPU while the all-slots wait stands (§3.2 rows 5, 12, 13), median of runs | Per scene × config × path |
| CPU render ms | CPU pre-fence ms + CPU post-fence ms | Per scene × config × path |
| Fence share | `fence ÷ wall_ms` | Per scene × config × path |
| Front-end share | `F ÷ G_n`, with `F = (s · G_p − G_n) ÷ (s − 1)`, where `G_n` and `G_p` are `gpu_main_render` at native and FSR Performance and `s` is the ratio of their `render_extent` pixel counts: the resolution-invariant part under the two-term model of `GDR-EV-003` | Per scene, Phase 0 and each later phase |
| Cull sensitivity | `gpu_main_render[BYRO_NO_CULL=1] − gpu_main_render[default]`, and the same for `draws=` | Per scene, Phase 0 |
| Entity scaling | Slope of CPU render ms against `entities` across the matrix's game scenes | Per path, each phase |
| Verdict mismatches | Instances whose GPU and CPU raster verdicts differ ÷ instances in the scene | Per scene, per GPU path (pass rule: 0) |
| Instance upload bytes | Bytes written to the instance scene per frame | Per scene, static pinned frame and orbit |
| Mesh-path coverage | Draws drawn as meshlets ÷ raster-visible draws | Per scene, `mesh` path |
| Cone-cull rate | Meshlets rejected by the cone test ÷ meshlets tested | Per scene, `mesh` path |

---

## 9. Delivery sequence

Milestone dependencies: M27 and R7 are complete
([Completed Milestones](../../ROADMAP.md#completed-milestones)). #5365
(narrowing the all-slots fence wait) is not a dependency of Phase 0 but
changes what Phase 0 measures (Q1).

**Phase 0 — Evidence, design and decisions (no engine code).** Append
the four TSV columns of `GDR-EV-001` to the bench harness, run the
matrix, the `BYRO_NO_CULL=1` pass and the profiler captures
(`GDR-EV-001` to `GDR-EV-007`); close the **Phase 0: verify** items below;
write `docs/engine/gpu-driven-rendering.md` with the cull predicate, the
instance-scene change sources, the meshlet layout and the path-selection
table; get Q1–Q6 decided. Exit: the attribution table is checked in with a
lever or "none" per scene, every verify item has a recorded answer, and
the design is registered in `docs/engine/index.md`. If no scene names a
lever, the PRD moves to WITHDRAWN with that table as its reason.

| Verify item | What settles it |
|---|---|
| **GDR-V-001** Linux drivers for the RT floor (NVIDIA proprietary, Mesa RADV, Mesa ANV) expose `VK_EXT_mesh_shader` with `taskShader` | `vulkaninfo` on the owner's RTX 4070 Ti, plus [vulkan.gpuinfo.org](https://vulkan.gpuinfo.org/) reports for one RADV and one ANV device of the RT generation |
| **GDR-V-002** The front-end share method of `GDR-EV-003` agrees with a profiler's vertex / primitive / fragment split | One RenderDoc or Nsight capture per FO4 scene compared with the TSV-derived share (`GDR-EV-005`) |
| **GDR-V-003** Every device the ray-query suitability check accepts also reports `drawIndirectCount` | vulkan.gpuinfo.org feature listing filtered on `VK_KHR_ray_query` |
| **GDR-V-004** Starfield's authored meshlets fit the chosen limits and carry no normal cone | A census over the Cydonia cell's `.mesh` files through the existing `BSGeometryMeshData` parser (an `--ignored` test), against nifly's `BSGeometryMeshData::Sync` layout |
| **GDR-V-005** Mesa lavapipe exposes `VK_EXT_mesh_shader`, so the mesh path can be validated without a GPU | `vulkaninfo` against lavapipe on the dev machine |
| **GDR-V-006** (withdrawn, rev 2) | Answered from code at HEAD: §3.2 row 16; the remaining work is a design item of `GDR-SCN-004` |
| **GDR-V-007** `vkCmdDrawMeshTasksIndirectCountEXT` requires the `drawIndirectCount` feature in addition to the mesh-shader features | The valid-usage list of the [vkCmdDrawMeshTasksIndirectCountEXT refpage](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdDrawMeshTasksIndirectCountEXT.html) |

**Phase 1 — `drawIndirectCount` and GPU-written draws.** Enable the
feature (`GDR-FB-005`), add the cull pass over the instances the CPU path
already uploads and the GPU-written commands and counts
(`GDR-CULL-001`…`007`, `GDR-IND-001`…`005`), the path selector and
telemetry (`GDR-FB-001`…`004`, `GDR-TEL-001`…`003`); the CPU still walks
the ECS. Starts only if Phase 0 names CPU sort / batch / indirect work or
post-fence CPU time. Exit: scenarios 2, 3 and 6 pass on Wave 1, verdict
mismatches are zero, and the `ssbo` + `cmd` portion of CPU render ms on
`gpu-cull` is published against a `cpu` control.

**Phase 2 — Instance scene.** Delta-maintained instance scene
(`GDR-SCN-001`…`007`), so a static view iterates no drawable entity; the
image-parity smoke script is written and indexed. Exit: scenarios 4, 5
and 7 pass on Wave 1, and the entity-scaling slope of CPU render ms on
`gpu-cull` is published beside the `cpu` slope. `gpu-cull` becomes the
default path when this exit is met.

**Phase 3 — Mesh path.** Meshlet generation, task / mesh shaders, cone
culling (`GDR-MS-001`…`008`), with the limit sweep of `GDR-MS-002`.
Starts only if Phase 0 or Phase 2's re-measure names front-end work
(`GDR-MS-007`). Exit: scenarios 3, 4 and 7 pass on the `mesh` path for
Wave 1, mesh-path coverage and cone-cull rate are published, and the
`gpu_main_render` change against `gpu-cull` is published per scene.

**Phase 4 — Wave 2 and close-out.** Oblivion, FO3 and Starfield
image-parity captures; the ROADMAP M52 row restated from the published
numbers; the stage table in the design updated. Exit: scenario 4 passes
on one interior per Wave-2 title, and the M52 row cites the evidence
table instead of a hand-written entity figure.

---

## 10. Decisions required

1. **Q1 — Order against #5365.** Recommended: run Phase 0 now, and start
   Phase 1 only after #5365 lands and Phase 0's table is re-measured on
   it, because the all-slots wait serialises post-fence CPU work with the
   GPU and so inflates exactly the CPU cost Phase 1 removes.
   Alternative: start Phase 1 on the current evidence; consequence: the
   Phase 1 gain is measured under a frame structure that #5365 will change,
   so the published number may not survive the fix.

2. **Q2 — Which levers M52 carries.** Recommended: all three, in the order
   GPU-written draws → instance scene → mesh path, each gated by the
   Phase 0 table (`GDR-MS-007`), because the first two run on every
   accepted device and attack the CPU cost the Tier 11 paragraph names,
   while the mesh path helps only front-end-bound frames on a subset of
   devices. Alternative A: the ROADMAP row's order, mesh path first;
   consequence: two raster vertex paths (`triangle.vert` and a mesh
   shader) to keep in lockstep before any CPU saving lands. Alternative B:
   drop the mesh path from M52 and fold it into M53, which needs clusters
   anyway; consequence: the ROADMAP row's promise changes (T1), and M53
   inherits the device-coverage question `GDR-V-001`.

3. **Q3 — Path selector surface.** Recommended: an environment variable
   (`BYRO_RENDER_PATH=cpu|gpu-cull|mesh`), following the `BYRO_NO_CULL`
   and `BYRO_PROFILE` precedent, plus the read-only `gdr.stats` command;
   it adds no host-facing contract. Alternative: a `--render-path` CLI
   flag persisted through `settings-io`; consequence: a CLI and settings
   contract change (README trigger T2) and a launcher-profile field to
   maintain for a switch players should not need.

4. **Q4 — TLAS instance assembly.** Recommended: unchanged in M52 — the
   host keeps assembling TLAS instances in `build_tlas`, and its cost is a
   reported metric (`GDR-RT-002`); the TLAS has no frustum culling to gain
   (`GDR-NG-003`). Alternative: write TLAS instances on the GPU from the
   instance scene with zero-mask inactive slots (`GDR-RT-003`);
   consequence: removes `cpu_tlas_ms`, but moves the BUILD-vs-UPDATE
   decision (`decide_use_update`) and the skinned-BLAS override away from
   the host code that owns them today, and reopens the #419 index
   invariant.

5. **Q5 — Meshlet source.** Recommended: one builder for every game at
   mesh registration, using meshoptimizer (MIT,
   [README](https://github.com/zeux/meshoptimizer)) through Rust bindings,
   with Starfield's authored meshlets left unused; one producer, cones for
   every game. Alternative A: read Starfield's authored meshlets in its
   translate step; consequence: a second meshlet producer, and its cull
   data is a `(center, expand)` pair per meshlet with no normal cone
   (nifly's `BSGeometryMeshData::Sync`; §6 row 2), so cone culling still
   needs generation. Alternative B: a hand-written Rust builder; consequence: no
   new dependency, but meshlet quality and its tuning become the project's
   to own.

6. **Q6 — Meshlet layout and M53.** The ROADMAP M53 row already gives the
   cluster mesh format to M53, and the asset-preprocessing PRD defers that
   format to the [Virtual Geometry PRD](virtual-geometry.md) (M53,
   `BAKE-NG-006`); the open question is only whether M52's meshlets are
   built to be reused. Recommended: the M52 meshlet layout is
   renderer-private, flat (no hierarchy) and disposable; M53 may replace
   it. Alternative: shape the M52 layout so M53 can extend it; consequence:
   M52 fixes layout choices (index width, per-meshlet header) before M53's
   simplification and hierarchy needs are known, and M53 inherits them.

---

## 11. Document map

| Need | Go to |
|---|---|
| Milestone row, Tier 11 start rule | [ROADMAP § Tier 11](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon) |
| The live bench numbers M52 is judged against | [ROADMAP § Bench-of-record](../../ROADMAP.md#bench-of-record-live--stepped-camera-refresh-2026-10-22-head-d9a201c71) |
| Bench methodology (control, pinned camera, settings path) | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench), [roadmap-history § 2](../archive/roadmap-history.md#2-superseded-bench-records) |
| Open performance issues (#5365, R6a-regress-22) | [ROADMAP § Performance and measurement](../../ROADMAP.md#performance-and-measurement) |
| Anti-scope and keep list | [ROADMAP § anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope), [keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation) |
| Current per-frame draw, batching, timers | [Vulkan Renderer § Per-frame draw](../engine/renderer.md#per-frame-draw) |
| TLAS / BLAS | [Vulkan Renderer § RT acceleration structures](../engine/renderer.md#rt-acceleration-structures), [Memory Budget § Acceleration Structures](../engine/memory-budget.md#acceleration-structures-blas--tlas) |
| Pass order, `GpuInstance` | [Shader Pipeline](../engine/shader-pipeline.md) |
| Buffer sizes and the ledger new buffers join | [Memory Budget](../engine/memory-budget.md) |
| Smoke-test index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Mesh shading spec | [VK_EXT_mesh_shader proposal](https://docs.vulkan.org/features/latest/features/proposals/VK_EXT_mesh_shader.html), [refpage](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_mesh_shader.html) |
| Draw counts from a buffer | [vkCmdDrawIndexedIndirectCount](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdDrawIndexedIndirectCount.html), [VkPhysicalDeviceVulkan12Features](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceVulkan12Features.html) |
| Compute-shader per-primitive filtering ahead of the fixed-function front end (session abstract only) | G. Wihlidal, "Optimizing the Graphics Pipeline with Compute", GDC 2016, [GDC Vault](https://gdcvault.com/play/1023109/Optimizing-the-Graphics-Pipeline-With) |
| Mesh clusters, GPU culling, a handful of indirect draws; two-phase occlusion culling with a depth pyramid (slides) | U. Haar and S. Aaltonen, "GPU-Driven Rendering Pipelines", SIGGRAPH 2015 Advances in Real-Time Rendering, [slides (PDF)](https://advances.realtimerendering.com/s2015/aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf), [course page](https://advances.realtimerendering.com/s2015/index.html) |
| Meshlet building, bounds, cone test, size guidance | [meshoptimizer README](https://github.com/zeux/meshoptimizer) |
| `GpuInstance` contract and mirror set | [Material and Decal Layering PRD § GPU contract](material-decal-layering.md#51-gpu-contract-and-the-r1-budget-mdl-gpu) |
| Consumers of the raster draw list and TLAS membership | [Texture Streaming PRD § Wanted mip](texture-streaming.md#54-wanted-mip-txs-want), [Path-Tracing Reference Mode PRD](path-tracing-reference-mode.md#51-mode-and-accumulation-ptr-mode) |
| M53 cluster format boundary, bundle section | [Virtual Geometry PRD](virtual-geometry.md); [Asset Preprocessing PRD](asset-preprocessing.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: bench TSV columns and the pre/post-fence CPU split, ECS change tracking read from code (`GDR-V-006` withdrawn), indirect-grouping and line corrections, meshoptimizer / Wihlidal / Gamebryo citations narrowed, `GpuInstance` lockstep (`GDR-SH-001`, `GDR-SH-005`), texture-streaming readback (`GDR-IND-006`), `GDR-RT-004` fields, `GDR-V-007`, Q6 boundary; coordinator cross-PRD sync (sibling links and citations) | claude: revise |
| 3 | 2026-11-05 | `GpuInstance` moved into one struct-only header: GDR-SH-001 includes `include/gpu_instance.glsl`, and its allowance for a listed copy is gone because no binding layout can rule that header out. The Phase exit check cites `shared_gpu_structs_are_declared_once` | — |
