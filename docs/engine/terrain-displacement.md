# Terrain displacement near the camera — TSD-4 design note (#6198)

**Status**: REVIEWED and approved by Matías on 2026-11-21 (#6198); the decisions are in §9.
Drafted 2026-10-22. Follows TSD-1 (#6195 height maps), TSD-2 (#6196 height blend) and
TSD-3 (#6197 parallax), which are shipped. #7149 (BTXT-only tile slots) and the §8 spike
(#7150, result in §8.1: go) are done. Stage 4.1 (#7152) is built (§7.1) and measured
(§8.2), with a two-level lattice (fine within 2 m, coarse beyond), and was approved on
2026-11-22. Stage 4.2 (#7221, patches in the TLAS with a refit BLAS each) is built (§7.2)
and measured (§8.3), awaiting review before 4.3. The 4.3 design is drafted in §7.3, also
for review. Its step 2, the screen-space level selector, is built with a 128 px² default
and measured in §8.5.

**Question this answers**: if parallax (TSD-3) is not enough, how would real geometry
displacement work in an engine whose shadows, reflections and GI are ray-traced, what would
it cost, and what could go wrong?

**Recommendation in one paragraph**: build it as a *fixed pool of camera-following,
cell-contained terrain patches* (3.66 m squares, 5.7 cm lattice) that a compute pass
re-displaces every frame from the same blended layer height TSD-3 marches, written in place
into a reserved slice of the global vertex pool, with their BLASes refit every frame — the
lifecycle the skinned-mesh pipeline already has. Displacement is **upward-only** and fades to
a 1 mm lip at every patch outer edge, cell border and range limit, so the existing base mesh
stays as the floor (no holes, no base-BLAS rebuilds, physics untouched). The estimate says it
is cheap in memory (≈13 MB) and plausibly cheap per frame, but **the BLAS refit cost is the
one number that decides it and nobody has measured it here**. The first step after approval is
a short measurement spike with a stated go/no-go bar (§8), not the feature.

Status legend used below: **[derived]** = arithmetic from engine constants, **[anchor]** =
an existing measurement in this repo, **[unmeasured]** = needs the spike.

---

## 1. What the engine looks like today (facts this design leans on)

| Fact | Source |
|---|---|
| A LAND cell is 4096 u = **58.5 m** (70 u/m); the mesh is 33×33 = 1089 vertices, 128 u = **1.83 m** apart, 2048 triangles, one BLAS per cell. | `cell_loader/terrain.rs` `spawn_terrain_mesh`, `BETHESDA_UNITS_PER_METER` |
| Terrain albedo/normal come from up to 8 splat layers + base; per-vertex weights are packed 2×RGBA8 and **interpolated**; layer textures are per-cell (`GpuTerrainTile`, one slot per cell, tile index in `GpuInstance.flags`). | `terrain.rs`, `bindings.glsl` |
| Layer textures repeat 6× per cell: one repeat = **9.75 m**; height maps are ≤1024² → **~0.95 cm/texel** (Oblivion's 512² maps: 1.9 cm). | `LAND_TEXTURE_TILES_PER_CELL`, TSD-1 cap |
| The collider is a **separate ghost entity** built from the whole LAND tile; the render entity is at identity. Physics never reads the render mesh. | `spawn_land_collider` |
| Whole LAND quadrants can already be dropped from the index buffer (XCLC Hide Quad). | `land_tile_indices(hidden_quadrants)` |
| Vertices are **104 B**; raster and RT hit shading both read the *global vertex pool* (cap 7 M verts = 728 MB, 28 M indices; was 4 M / 416 MB / 16 M before bb97be77c). Mesh handles are **never reused**; every upload goes through `pending_vertices` and a global geometry-SSBO (re)build with an idle-reclaim path. | `mesh.rs`, `mesh/geometry_ssbo.rs` |
| Global vertex buffer usage = `STORAGE_BUFFER \| VERTEX_BUFFER` (+ AS build input) — compute can write it. | `geometry_ssbo.rs:345` |
| Skinned meshes already run **compute → position buffer → per-frame BLAS refit** (`skin_vertices.comp`, `blas_skinned.rs`; rebuild after 600 refits to reset bounds). | `skin_compute.rs`, `acceleration/constants.rs` |
| Cell crossings are a measured hot path: single-hash apply up to ~300 ms, batched BLAS submission 20–55 ms, residual frame spikes "tied to global geometry work". | `exterior-readiness-plan.md` [anchor] |
| `terrain.seams` audits **LAND height agreement between adjacent resident cells on the CPU**; it does not look at GPU geometry. | `TerrainSeamStats`, `terrain_seam::check_seam` |
| Every drawn LAND cell has a terrain tile, BTXT-only cells included (#7149; until then they had none, so their base height map was not bound and TSD-3 parallax skipped them). | `terrain.rs` `terrain_tile_record` |

## 2. Why the obvious shapes don't work

- **Hardware tessellation**: does not feed acceleration structures (the issue's own constraint).
- **Displace whole cells** (what "tiles nearest the player" can read as): a cell at 5.7 cm is
  **1.05 M vertices / 2.1 M triangles** [derived]; the 3×3 neighbourhood is ~19 M triangles.
  Not viable. Displacement has to be confined to a short radius around the camera.
- **A single camera-centred clipmap grid**: elegant (one mesh, one BLAS) but a lattice that
  spans a cell border needs two cells' tiles at once, and a triangle can carry only one tile
  index — weights meant for tile A would be interpreted against tile B's layer list. Patches
  must therefore be **contained in one cell**.
- **Streaming one mesh per patch through `upload_scene_mesh`**: every upload dirties the global
  geometry SSBO (a rebuild, with an idle-reclaim path) and consumes a never-reused handle.
  Camera-driven churn would grow the handle table without bound and hitch. Patches need a
  **reserved region filled in place**.

## 3. Proposed design

### 3.1 Geometry
- **Patch** = 256 u (**3.66 m**) square, 64×64 quads at 4 u (**5.7 cm**): 4225 vertices,
  8192 triangles, 0.54 MB [derived]. World-aligned inside a cell (16×16 patches per cell), so a
  patch never straddles a cell border and always has exactly one tile.
- **Pool**: 24 slots (≈ the ~9–16 patches a 6 m radius touches, plus hysteresis) =
  101 k vertices / 197 k triangles / **12.9 MB** in the global pool [derived]. Reserved **once at
  start-up** as placeholder geometry (24 stable handles), never allocated or freed again.
- **One shared index buffer** for all slots (identical topology; local indices — the draw path
  already adds a per-mesh vertex offset).
- **Range**: patches whose centre is within ~6 m of the camera (a setting; same slider family
  as TSD-3). Beyond it, TSD-3's parallax carries on to its 20 m range unchanged.

### 3.2 The displacement function (shared with TSD-3 — one source of height)
`h(uv)` is exactly `byroTerrainBlendedHeight` (the Phase-2 blend core, already shared by the
colour blend and the parallax march). Vertex displacement = `amplitude · fade · (h − h_min)`,
always **≥ 0 above the base surface**, along the base normal. No second height pipeline; units
in metres through `BETHESDA_UNITS_PER_METER` at the one broadcast site, as in TSD-3.
Geometry amplitude: ~3 cm default (the issue's "a few centimetres"), tunable.

### 3.3 Why upward-only (the main decision — please challenge it)
The base cell mesh stays in place and in its BLAS. A patch sits on top of it, 1 mm above at its
borders and up to `amplitude` above in the interior.

- **No holes** → no base-index edits, **no base-BLAS rebuild every 1.8 m of movement**, no
  T-junction cracks between patch and base (the lip is a vertical 1 mm step, not a gap).
- **Physics and gameplay unchanged** (separate ghost collider at base height); the visual
  surface stands ≤ 3 cm proud of it — feet sink ≤ 3 cm into relief.
- Costs: relief can only rise, so "dirt filling cracks between stones" reads as stones standing
  *proud* rather than dirt sunk *below*; and the hidden base is still drawn under each patch
  (overdraw confined to the patch footprint).
- The alternative (zero-mean displacement with quad-granular holes) is cleaner visually but
  needs per-camera-step edits to the base index buffer and BLAS — rejected for v1; revisit only
  if "proud only" looks wrong in review.

### 3.4 Fades and seams (the issue's "no cracks" requirement)
Displacement is forced to the 1 mm lip at (a) the patch's outer edge where no neighbour patch
is resident, (b) **every cell border** (adjacent cells have different splat layer lists, so
their blended heights disagree there; a 0.5–1 m ramp keeps both sides at the lip), and (c)
smoothly with camera distance via the same `smoothstep` form TSD-3 uses. Interior patch borders
within a cell share the same function on the same lattice → agree by construction.
Verification: a CPU mirror unit test of the fade/border rule, plus a new
`terrain.patches` console command that reads back each slot's border-vertex heights and reports
max |border − lip| — the geometric analogue of the data-level `terrain.seams`.

### 3.5 Per-frame work
1. **Compute pass** (new, `terrain_patch.comp`, modelled on `skin_vertices.comp`): for each
   resident slot, per vertex: sample ≤3 height lanes × 5 taps (centre + 4 for the normal),
   interpolate splat weights/colour/uv from the base 33×33 grid, write the full 104 B vertex
   in place.
2. **Barrier**, then raster reads the global vertex buffer as usual; the BLAS **refit**
   reads the same bytes — one source of truth, so raster and RT cannot disagree by float noise
   (the reason not to displace in the vertex shader and again in compute).
3. TLAS: +≤24 instances (≈ +3 % on a ~730-instance Whiterun TLAS [anchor]); it is rebuilt every
   frame already (`tlas = 0.0 ms` measured).
- **Slot (re)assignment** when the camera crosses a patch boundary: ≤ 2 patches/frame get a
  full BLAS *build* (8 k triangles each); everything else only refits. Never inside the
  cell-apply atomic span (§1, the 300 ms hot path) — a separate per-frame budgeted step.
- Rebuild (not refit) a slot's BLAS after 600 refits, like skinned meshes.

### 3.6 Interaction with TSD-3 parallax (must not double-count relief)
Inside a patch the surface already carries `amplitude_geo` of relief, so the parallax depth
there must drop to `depth_pom − amplitude_geo` (and to 0 where that is ≤ 0), blended with the
same distance fade. Outside the patch range parallax is unchanged. Needs a per-tile broadcast
of `amplitude_geo` and a fragment-side rule; first-order only — the geometry is the low
frequencies, parallax the 1 cm texel detail.

## 4. Cost estimates

### 4.1 Memory and geometry budget — **[derived]**
Vertex 104 B + index 12 B/triangle; patch lattice spacing × radius:

| radius | 4 cm | **6 cm** | 8 cm | 12 cm |
|---|---|---|---|---|
| 4 m | 63 k tris / 4.0 MB | 28 k / 1.8 | 16 k / 1.0 | 7 k / 0.4 |
| **6 m** | 141 k / 9.0 | **63 k / 4.0** | 35 k / 2.3 | 16 k / 1.0 |
| 8 m | 251 k / 16.1 | 112 k / 7.1 | 63 k / 4.0 | 28 k / 1.8 |
| 10 m | 393 k / 25.1 | 175 k / 11.2 | 98 k / 6.3 | 44 k / 2.8 |

(disc area only; the 24-slot square-patch pool is 197 k triangles / 12.9 MB.) Against the pool
cap of 7 M vertices (728 MB) it is **~1.4 % of the vertex pool** (2.5 % at the old 4 M cap). The 5.7 cm lattice is the
sensible floor: the source height texels are ~1 cm, so 4 cm geometry buys little over what
TSD-3 already resolves.

### 4.2 GPU time per frame
| Item | Estimate | Status |
|---|---|---|
| Compute displace + normals, ~101 k vertices × ~15 fetches | same order as the skin pass; I expect ≲ 0.2 ms but have **no number** | **[unmeasured]** |
| BLAS refit, ≤197 k triangles | **the deciding unknown** — refit throughput on this GPU has never been measured here (only animated actors: `blas_refit` 0.1–0.2 ms in the logged scenes, triangle count not recorded) | **[unmeasured]** |
| Raster of +≤197 k triangles over the existing ~900 draws | small next to the measured `main_render` of 5–9 ms (FO4 eye view 18.6 ms) at 853×480; not measured | **[unmeasured]** |
| TLAS +24 instances | ≈ 0 | [anchor] `tlas=0.0 ms` |
| Patch BLAS builds on slot reassignment | ≤2 × 8 k-triangle builds/frame, amortised; ≈1–3 patches/s at running speed | **[unmeasured]**, budget-capped |

Frame-time parametrisation, so the review is not asked to trust a single guess: *refit cost =
triangles ÷ throughput*. At 197 k triangles a throughput of 100 / 400 / 1000 M tri/s would cost
~2.0 / 0.5 / 0.2 ms. The spike picks the row.

### 4.3 What it adds to streaming
Nothing inside the cell-apply span: the pool is reserved at start-up, slot filling is a
budgeted per-frame step, and cell unload releases the cell's slots **before** its tile slot is
freed (`unload.rs` already frees terrain tile slots; the patch release hooks in beside it).

## 5. Risks
1. **BLAS refit cost / BVH quality** — the unknown above. Mitigation: the spike; fallback is
   *static* patches (build once with full amplitude, no per-frame work, distance pop hidden by
   hysteresis and TSD-3 beyond) — loses the smooth fade and costs a visible ≤ 3 cm pop.
2. **Placed ground meshes** (roads, rock plates sit a few cm above the base): terrain relief
   rising up to 3 cm could poke through them — the FO4 dirt-road test position is exactly
   this. Needs a look in review; amplitude is tunable; a per-vertex suppression near placed
   statics is the heavy fix, not planned.
3. **Denoiser/TAA stability**: real geometric normals at 5.7 cm give SVGF and ReSTIR finer
   normal/ID structure than the flat terrain they see today; history rejection could rise and
   indirect noise with it (see the 2026-10-15 noise investigation). Must be measured with the
   same still-camera harness; TSD-3's parked-camera shimmer metric was too noisy to settle it.
4. **Global-SSBO generation rebuilds** must preserve the reserved region (the compute pass
   rewrites it every frame, so a copy loss self-heals, but the barrier/ownership rules between
   compute-write, vertex-fetch and AS-build-read must be right: this needs RenderDoc and
   `BYRO_VALIDATION=1`, not reasoning — per the no-speculative-Vulkan-fixes rule).
5. **BTXT-only cells** (resolved by #7149: they now get a tile) had no splat tile, so they would have got no patches (and, honestly, no TSD-3
   parallax either — #6195 deferred binding their base map "to Phase 3" and Phase 3 did not
   pick it up). Cheap to fix at the tile-allocation site; should be done before this.
6. **Per-game**: none expected — the renderer consumes "a height map per terrain layer" only.

## 6. Test plan
CPU mirrors for fade/border/lattice math (as for TSD-2/3); GLSL pins for explicit-gradient
reads; layout pins for any tile fields; `terrain.patches` seam/border readback; a validation-
clean run at all five TSD positions; before/after screenshots at those positions plus the
frame-time table with the same interleaved on/off harness; the stricter still-camera noise
measurement from risk 3.

## 7. Implementation order (after sign-off)
**4.0 spike** (throwaway, measurement only) → **4.1** patch pool + compute + raster only, no
RT → **4.2** BLAS in the TLAS + refit + seam readback → **4.3** TSD-3 interplay + fades +
settings/UI. Each stage ends with a review, as in TSD-1–3.

### 7.1 Stage 4.1 as built (#7152)
- **Pool**: `vulkan/terrain_patch.rs`. 24 slots, each with a fine mesh (65×65 vertices)
  and a coarse one (33×33, see *Two-level lattice* below), reserved in
  `VulkanContext::new` with `upload_scene_mesh_global_only` (global pool only: no per-mesh
  buffers, no BLAS). They are the registry's first handles on purpose: the opaque sort
  groups by mesh handle, so every patch draws before any base cell and early-Z rejects
  the base under it. Each mesh has its own copy of the lattice indices (24 × 98 KB fine
  + 24 × 25 KB coarse), not the single shared buffer of §3.1; sharing needs index
  aliasing in `MeshRegistry`, which can wait for 4.2.
- **Two-level lattice** (added after the first §8.2 run showed raster cost scaling with
  triangle density). A square within the fine range (2 m, kept until 0.5 m past it)
  draws the fine 5.7 cm lattice; the rest of the 6 m range draws the coarse 11.4 cm
  lattice, a quarter of the triangles. Coarse vertices are the fine lattice's even
  points through the same function, so the levels agree wherever both have a vertex.
  Where a fine patch borders a coarse one (the host passes a coarse-neighbour mask beside
  the missing-neighbour mask), its odd border vertices are the midpoint of their two
  even neighbours, so they lie on the coarse edge: the T-junction stitch, no crack. The
  normal's gradient taps stay one fine step apart on both levels. At the defaults 4–5
  squares are fine, the rest coarse. `terrain.patches fine <m>` retunes it: 0 makes every
  patch coarse, a value at or past the range every patch fine (the single-level pool).
- **Compute**: `terrain_patch.comp`, one dispatch per resident slot, recorded after the
  instance upload and before the main pass (`record_terrain_patches`, GPU bracket
  `terrain_patch`). It interpolates the base vertex on the emitted base *triangle* (the
  `land_tile_indices` split), lifts it along the base normal by
  `lip + amplitude · fadeDistance · fadeMissing · h`, with `h` from
  `byroTerrainBlendedHeight` (now in `include/terrain_tile.glsl`, shared with the
  fragment shader), and tilts the base normal by the gradient of that displacement (four
  extra evaluations), so where the displacement is constant the normal is the base's own.
- **Departures from §3**, all to keep the "no cracks" rule by construction:
  - the 1 mm lip is a floor everywhere, not only at borders: a zero height would
    otherwise put the patch exactly on the base and z-fight;
  - the fade to the lip depends only on world position and the camera. It measures the
    distance to the nearest square of the 3×3 neighbourhood that has no resident patch,
    and squares across a cell border always count as missing, which is the cell-border
    rule of §3.4. A per-side "neighbour missing" fade, as §3.4 reads, cracks shared
    edges at a corner whose diagonal neighbour is missing;
  - assignment covers every square whose footprint comes within the range (not only
    squares whose centre does), so the faded disc is always covered while slots last,
    with 1 m of hysteresis;
  - a square inside an XCLC-hidden quadrant gets no patch: `TerrainCellOrigin` carries
    the cell's Hide Quad bits, the assignment skips those squares, and since they count as
    missing, the neighbouring patches fade to the lip there (the base drops the same
    quadrants from its index buffer, #6043);
  - the TSD-3 parallax march still runs on patch draws by default (§3.6 is 4.3). A patch
    draw carries `INSTANCE_FLAG_TERRAIN_PATCH` (bit 11, set by the renderer from the
    reserved patch meshes), and `terrain.patches parallax off` makes `triangle.frag` skip
    the march on it. Measured at FNV eye level in one engine run (10 interleaved rounds):
    0.92 ms with the march, 0.78 ms without, a 0.13 ms saving. Without it the near
    stones read much flatter, because 3 cm of real relief replaces 8 cm of parallax depth.
    So the default stays on, and 4.3 should march only the residual relief on patches,
    not none.
- **Raster**: the host (`byroredux/src/terrain_patches.rs`) clones each patch's base
  cell draw command onto the patch mesh with `in_tlas = false`, before the sort, so the
  patch carries the cell's exact material and tile.
- **Readback**: `terrain.patches` prints, one frame-in-flight late, every border that must
  sit at the lip and the largest |displacement − lip| on it, every border two resident
  patches share and the largest world-Y gap between them (on a fine/coarse border, between
  each odd fine vertex and the coarse edge's midpoint under it), each slot's level and
  largest displacement, and the `terrain_patch` / `main_render` GPU brackets. `terrain.patches
  on|off` is the A/B switch; `amplitude` / `range` retune the session.

### 7.2 Stage 4.2 as built (#7221)
- **One BLAS per patch mesh, not one multi-geometry BLAS.** §8.1 recommended one BLAS
  for all patches, to pay one refit's fixed cost. But every ray hit resolves through
  `GpuInstance` + the primitive id (`ray_hit.glsl`) and no shader reads a geometry
  index, so a multi-geometry BLAS would have meant changing every hit path. A BLAS per
  patch mesh hit-shades like any rigid instance, reading the live vertices the compute
  writes. The fixed cost §8.1 measured came from the shared-scratch serialize barrier,
  and that is avoided another way: all of a frame's patch builds and refits go out in
  **one** `vkCmdBuildAccelerationStructuresKHR` call over disjoint regions of the patch
  set's own scratch buffer (`acceleration/blas_terrain_patch.rs`).
- **Refit or build.** A mesh still showing last frame's square refits (only the
  distance fade moved its vertices). A mesh that changed square or level, whose pool
  addresses moved, or that reached 600 refits gets a full build, at most two a frame
  (§3.5). A deferred one stays out of the TLAS that frame, so rays see the base terrain
  there; it is not counted as a missing BLAS, so the TLAS integrity check stays clean.
  The static-BLAS restore path skips patch meshes, which it would otherwise snapshot.
- **Frame order.** The patches now record ahead of the TLAS build: the terrain-tile
  upload (split out of the instance upload), the patch compute, the patch BLAS, then
  `dispatch_skin_and_cluster`'s TLAS build. The compute's exit barrier adds the AS-build
  stage (`shader-pipeline.md` steps 1e–1g).
- **The base terrain stays in the TLAS** under the patches. Rays reaching the ground
  from above hit the patch first. A shadow ray starts on the patch surface, which is
  never below the base, so the 1 mm lip needs no mask.
- **Ground cover.** The blade and model tiers trace straight down to keep plants off
  placed geometry. The patches sit inside that span, so the first build dropped every
  blade under a patch. Patch TLAS instances now carry `TLAS_SBT_TAG_TERRAIN_PATCH` in
  the SBT record offset, which ray queries do not otherwise use. The shared
  `gcRootCovered` (`include/groundcover_cover.glsl`, now the single copy of that trace)
  skips them.
- **Telemetry.** The `terrain_patch_blas` GPU bracket, plus a `blas:` line in
  `terrain.patches` (ready / built / refit / deferred / failed, bytes); its GPU line also
  prints the TLAS build bracket.

### 7.3 Stage 4.3 design (draft, 2026-11-22, for review)
§7 listed 4.3 as TSD-3 interplay, fades and settings/UI. §8.3 adds a fourth item that
comes first, because it is where the cost is: **the lattice level is chosen by world
distance, but the cost follows the projected triangle size.**

**What 4.3 is answering.** At FNV the patches still cost 1.45–1.99 ms against the
1.0 ms bar (§8.3). Rays fired from the patch pixels carry nearly all of it, and per ray
they cost about the same as from base pixels. What differs is how many warps the same
pixels fill. #7261 stopped helper lanes tracing rays, which removed 35–43 % of the
premium. But a quad split by a triangle edge still costs a warp slot whose lane does
no work, and that warp still waits on its real lanes' rays. So what is left scales with
how many quads the triangle edges split.

**How small the triangles are** — **[derived]**, assuming the `Camera` default
`fov_y` of 45° (`camera.rs`; the bench camera's own value still needs confirming) and
an eye 1.7 m above flat ground. Each cell is the projected edge of one lattice quad
and the area of one of its triangles (half the quad, foreshortened by the view angle
to the ground), in render pixels:

| distance | fine 5.7 cm @ 480 lines | coarse 11.4 cm @ 480 | fine @ 1440 lines | coarse @ 1440 |
|---|---|---|---|---|
| 1 m | 16.7 px / 121 px² | 33.5 px / 483 px² | 50 px / 1088 px² | 100 px / 4350 px² |
| 2 m | 12.6 / 51 | 25.2 / 205 | 38 / 461 | 75 / 1846 |
| 4 m | 7.6 / 11 | 15.2 / 45 | 23 / 102 | 46 / 407 |
| 6 m | 5.3 / 4 | 10.6 / 15 | 16 / 34 | 32 / 138 |

Two things follow:
- **At the bench's render height, patch triangles are small.** A 50 px² triangle has
  about 43 px of perimeter, which splits about 20 quads, so roughly 40 % of its lanes
  are helpers. That is the share #7261 measured. The coarse lattice at 4–6 m is
  15–45 px², the worst of the pool.
- **The same distance rule is wrong at another resolution.** At 1440 render lines
  every triangle is nine times larger. The 2 m fine cut-off is then too timid: fine
  triangles at 6 m are as large as they are at 2 m at 480 lines. And with an
  upscaler, it is the render extent that matters (`FrameExtentSet::render`), not the
  output. The texture streamer's footprint uses the swapchain extent instead, which
  suits mip choice but not raster cost.

**Proposed selector.** Each square picks its level from the projected area of the fine
lattice's triangles at the square's nearest point (the same footprint point
`distance_to_square` uses now):

- `area_fine = 0.5 · (step · k / D)² · sin θ`. Here `k = render_height / (2 tan(fov_y/2))`,
  `D` is the 3D distance from the camera to that point, and `sin θ = h / D` with `h` the
  camera's height above it. Fine if `area_fine ≥ A_FINE`, otherwise coarse.
  *As built*, `sin θ` is `|n · v̂|`, where `n` is the normal of a plane fitted to the
  square's nine LAND vertices and `v̂` points from that point to the camera. That is
  `h / D` on flat ground, but it also credits a slope that faces the camera (FNV's
  spawn), which the flat form undercounts. The bench camera's `fov_y` is 45°, which
  `terrain.patches` now prints. So the table above holds as derived.
- **Distance and height only, never view direction.** Turning the camera must not
  change a level, because every level change costs a BLAS build, and those are capped
  at two a frame (§7.2).
- **Hysteresis in level terms**: a fine square drops to coarse only once its area is
  under `A_FINE / 2`, the analogue of today's 0.5 m.
- **`A_FINE` is a measured default, not a guess.** Sweep 32 / 64 / 128 / 256 px² at
  two render heights, with the interleaved harness and screenshots. Pick the knee
  where the premium flattens but the relief still reads (§8.2 showed that all-coarse
  reads flatter at eye level).
- **Shared projection helper.** `texture_streamer::compute_wanted` computes
  `viewport_height · 0.5 / tan(fov_y/2)` inline. Factor it into one helper that both
  call, each with its own extent.

**A third, coarser level (option, decided by measurement).** At 480 lines the coarse
squares at 4–6 m are the smallest triangles drawn. A 22.8 cm level (17×17 vertices,
289 per slot, 61–181 px² at 4–6 m) would cover them. There, 3 cm of relief projects to
only about 3–4 px. The fine-to-coarse stitch (odd border vertices on the even
neighbours' midpoint) works between any two adjacent power-of-two levels. So it holds
only if neighbouring squares differ by at most one level, which is a 2:1 balance
constraint the assignment would have to enforce. Add it only if the sweep shows the
coarse squares still carry a measurable share.

**Open input: the ground height.** The host knows each square in XZ only
(`TerrainCellOrigin` holds the origin and the hidden quadrants), so it cannot yet
compute `D` or `h`. Options:
1. Store each square's base-height range when the cell spawns: 16×16 pairs per cell,
   from the LAND heights `spawn_terrain_mesh` already has. **Recommended:** exact,
   cheap, no per-frame work. *Built* as `TerrainSquarePlanes` (`components/terrain.rs`):
   each square's least-squares plane and height range, rebuilt at every load and not
   saved.
2. Use a fixed eye height. This is wrong on slopes, which is exactly FNV's spawn.
3. Read it back from the GPU patch compute. That is a frame late and has no value for
   a square not yet assigned.

**Order within 4.3:**
1. **Re-baseline after #7261.** Done in §8.4: three poses are still over the bar, by
   0.08–0.18 ms (FNV down and eye, tundra eye).
2. **The selector, with option 1 heights.** CPU mirror tests:
   - the area formula against hand values;
   - rotation invariance (a yaw/pitch sweep changes no level);
   - hysteresis;
   - the existing `fine 0` / full-fine overrides, kept as forced modes.

   Then the `A_FINE` sweep at 480 and 1080 render lines. **Done** (§8.5), at 480 and
   675–696 lines: the 2560×1440 window meant for 960 lines is clamped to the bench
   machine's 1080p monitors (§8.9). The default is 128 px².
3. **The third level and 2:1 balance**, only if step 2's data asks for it.
4. **§3.6 residual parallax on patches**: march `depth_pom − amplitude_geo`. Its cost
   bound is small (0.31 / 0.20 / 0.03 ms, §8.3), but it keeps the near relief from
   reading flat. **Done** (§8.6). The patch geometry reaches the fragment as four
   `GpuTerrainTile` lanes broadcast with each frame's patch parameters (stride
   224 → 240). The march subtracts `patchAmplitude · fade`, where `fade` is the camera
   distance fade `terrain_patch.comp` applies. The compute's second fade, the 1 m
   ramp to the lip at a missing neighbour or a cell border, is not reproduced, so
   relief there reads slightly flatter (first order).
5. **Settings/UI**: amplitude, range and `A_FINE` (or a quality preset that maps to
   it), each one registry entry, through the settings pass. **Done** as three
   `Rendering` sliders, which the debug UI, the launcher and the in-game menu all
   draw from the registry:
   - `render.terrain_patch_amplitude_m`: 0–0.1 m, default 0.03. 0 turns the patches off.
   - `render.terrain_patch_range_m`: 0–7 m, default 6. 0 turns the patches off. The
     cap comes from the pool: 24 slots of 3.66 m squares, each kept until 1 m past
     the range, cover about 20 squares at 7 m on flat ground, so the whole fade
     stays patched.
   - `render.terrain_patch_fine_threshold_px2`: 0–512 px², default 128 (§8.5).
     0 makes every patch fine.

   The host reads the three each frame before it locks the pool. `terrain.patches
   amplitude | range | afine` now set these entries for the session only, like the
   other `terrain.*` switches, so they don't reach the settings file until a menu
   edit. `fine <m>` still forces 4.1's distance rule for the session, until
   `afine`. Among the quality presets (`assets/graphics_presets.toml`), Low turns
   the patches off (amplitude 0, §8.7) and Medium raises the threshold to 512 px²
   (§8.8). At 480 render lines that draws every patch coarse; at higher render
   heights the squares nearest the camera stay fine (§8.9). High and Ultra keep the
   defaults.

**Not in 4.3.** A visibility-buffer or deferred lighting pass would trace rays per
covered pixel with no quad overhead at all. It is the structural fix for every dense
mesh, not just the patches. It is far outside a TSD stage and is noted here only
because the patches are where the quad overhead was first measured.

## 8. The spike, and its go/no-go bar (proposed — adjust it)
Measure on the 4070 Ti, release build, at the five TSD positions: (a) refit time for 197 k
triangles of a displaced grid, (b) compute displace time for 101 k vertices, (c) added raster
time, (d) the extra `main_render` with patches resident.
**Go** if the total added GPU time at defaults is ≤ **1.0 ms** (the bar TSD-2/3 held) and
refit alone ≤ 0.5 ms. **Otherwise** fall back to static patches (risk 1) or drop Phase 4 and
keep parallax.

### 8.1 Spike result: proxy measurement (#7150, 2026-11-21)
Run as a proxy, not a throwaway build (decided with Matías): the engine already refits skinned
actor BLASes every frame, and `skin.coverage` now reports the triangles refit and the vertices
skinned beside the two GPU brackets. 4070 Ti, release, 20 samples per scene, stable to ±2 %.

| scene | refits / frame | triangles | `gpu_skin_blas_refit_ms` | vertices | `gpu_skin_dispatch_ms` |
|---|---|---|---|---|---|
| FNV `GSProspectorSaloonInterior` | 18 | 23 751 | 0.219–0.229 | 14 730 | 0.008–0.010 |
| Skyrim SE `WhiterunBanneredMare` | 11 | 8 249 | 0.131–0.135 | 6 170 | 0.005–0.009 |
| same, a busier frame | 23 | 23 731 | 0.280–0.282 | 18 156 | 0.009–0.010 |

**Refit is per-BLAS overhead first, triangles second.** The FNV and busy Skyrim rows have the
same triangle count; five more refits cost 0.058 ms, so a refit carries **≈ 11.6 µs of fixed
cost** (each refit self-emits the shared-scratch serialize barrier, `blas_skinned.rs`). What is
left is **≈ 0.6 µs per 1 000 triangles** (~1.7 G tri/s), and it fits all three rows.

Projected for the §3 defaults (24 patches × 8 192 triangles = 197 k):
- (a) refit, 24 BLASes: 24 × 11.6 µs + 197 × 0.6 µs ≈ **0.40 ms**, under the 0.5 ms bar with
  ~20 % margin. With all patches as geometries of **one** BLAS (one refit, one barrier):
  ≈ **0.13 ms**.
- (b) compute: the skin pass is ~6 µs fixed + ~0.25 ns/vertex, so 101 k vertices of
  skinning-equivalent work cost ~0.03 ms. Displacement does ~15 height fetches per vertex
  instead of 4 bone reads; at 2–5× the per-vertex cost that is **≈ 0.06–0.13 ms**.
- (c)/(d) raster and the `main_render` delta: **not measured** by the proxy. There is no depth
  prepass, so whether the base terrain under a patch is shaded twice depends on draw order.
  Looking down, the patches cover most of the screen; this is the open number.

**Verdict: GO for stage 4.1** (Matías, 2026-11-21; #7150 closed, stage 4.1 is #7152). (a) + (b) ≈ 0.46–0.53 ms with 24 BLASes, or
≈ 0.19–0.26 ms with one BLAS, which leaves 0.5–0.8 ms of the 1.0 ms total for raster. 4.1
(raster only, no RT) is where (c)/(d) get measured; 4.2 should build the patches as one
multi-geometry BLAS rather than 24.

### 8.2 Stage 4.1 measurement (#7152, 2026-11-21)
4070 Ti, release, `--bench-mode renderer-static`, radius 1, the engine's own spawn point on
each grid, camera pitch −80° (down) / −25° (eye) / −8° (graze). Each row is 10 interleaved
off/on rounds × 4 samples of the GPU brackets (`terrain.patches` toggled between rounds),
medians in ms; the patches were at the §9 defaults (3 cm, 6 m), 16 of 24 slots resident.
**Total** = the `terrain_patch` compute bracket + the `main_render` delta (raster, no RT yet).

| position (grid) | pose | `main_render` off | delta | compute | **total** |
|---|---|---|---|---|---|
| Skyrim, Whiterun tundra (2,−4) | down | 3.66 | +0.54 | 0.21 | **0.74** |
| | eye | 4.39 | +0.78 | 0.18 | **0.96** |
| | graze | 8.65 | +0.48 | 0.19 | **0.67** |
| Skyrim, Riverwood bank (4,−11) | down | 1.81 | +0.33 | 0.21 | **0.54** |
| | eye | 1.89 | +0.47 | 0.19 | **0.66** |
| | graze | 2.62 | +0.33 | 0.25 | **0.58** |
| FNV, Mojave (0,0) | down | 7.86 | +1.17 | 0.28 | **1.44** |
| | eye | 7.81 | +1.84 | 0.20 | **2.04** |
| | graze | 7.93 | +2.18 | 0.20 | **2.38** |
| FO4, Commonwealth (0,0) | down | 8.25 | +0.89 ± 2.3 sd | 0.19 | **1.08** |
| | eye | 9.64 | +0.34 ± 3.0 sd | 0.19 | **0.53** |
| | graze | 10.38 | +0.46 | 0.20 | **0.66** |
| Oblivion, forest (0,0) | down | 7.26 | +0.24 | 0.19 | **0.43** |
| | eye | 7.23 | +0.32 | 0.19 | **0.51** |
| | graze | 7.51 | −0.06 | 0.18 | **0.12** |

Sample spread is ≤ 0.6 ms sd except where shown (FO4 down/eye are noisy, so treat those
deltas as ±1 ms). The FO4 grid is TSD-1's painted 0,0 cell, not a verified dirt road.

- **Seams**: every run read back `max|d − lip| = 0` over 16 lip borders and a `0` gap over
  24 shared borders; the largest displacement is 3.08–3.10 cm (lip + amplitude).
- **Validation**: `BYRO_VALIDATION=1` (sync validation) at the Skyrim tundra and FNV
  positions, all three poses, patches on and off: no VUID or SYNC-HAZARD message.
- **Compute is 0.18–0.28 ms**, 2–3× the §8.1 projection (0.06–0.13 ms).
- **Raster is the cost, and it scales with triangle density, not patch count.** At FNV (the
  spawn faces a slope, so patch terrain fills the screen), a 3 m range (6 patches) left the
  raster delta at 1.0 / 2.0 / 2.0 ms while halving compute to 0.10 ms; a temporary 32×32-quad
  lattice (11.4 cm, a quarter of the triangles) roughly halved it, to 0.77 / 1.08 / 0.90 ms
  (compute 0.12–0.17 ms). The patches draw before their base cells (first mesh handles), so
  this is not the base shaded twice in full; it reads as the per-pixel overhead of small
  triangles over a heavy fragment shader.
- **Against the 1.0 ms bar** (before 4.2's ≈ 0.13 ms refit): Riverwood, Oblivion and FO4
  graze/eye pass; the tundra passes (eye 0.96); FO4 down is at the bar within its noise; FNV
  fails at every pose (1.4–2.4 ms).
- Not yet in these numbers: TSD-3 parallax still runs on the patches (the §3.6 interplay is
  4.3), and the patches are not in the TLAS (4.2).

**Two-level lattice re-run** (same build otherwise, same harness, 8 interleaved rounds of
off / single-level (`fine 99`) / two-level (`fine 2`) × 4 samples; 16 patches resident, 12
of them coarse at two-level). Totals are compute + `main_render` delta, ms:

| position | pose | `main_render` off | single-level | **two-level** | compute single → two |
|---|---|---|---|---|---|
| Skyrim tundra (2,−4) | down | 3.76 | 0.81 | **0.80** | 0.20 → 0.13 |
| | eye | 4.73 | 0.95 | **0.58** | 0.19 → 0.13 |
| | graze | 9.37 | 0.69 | **0.27** | 0.18 → 0.13 |
| Skyrim Riverwood (4,−11) | down | 1.81 | 0.55 | **0.47** | 0.19 → 0.13 |
| | eye | 1.91 | 0.70 | **0.57** | 0.20 → 0.14 |
| | graze | 2.73 | 0.52 | **0.34** | 0.21 → 0.14 |
| FNV Mojave (0,0) | down | 5.08 | 1.01 | **0.98** | 0.18 → 0.12 |
| | eye | 6.54 | 2.04 | **1.52** | 0.19 → 0.13 |
| | graze | 7.06 | 1.30 | **0.79** | 0.18 → 0.12 |

- **Seams**: zero gap over 24 shared borders, 8 of them fine/coarse, and zero lip deviation,
  at every position and pose.
- **The two-level lattice brings every Skyrim pose and FNV down/graze under the 1.0 ms bar.**
  FNV eye (1.52 ms) is still over it: the spawn faces a slope, so the fine squares fill
  the lower half of the frame. Looking down, the gain is small everywhere, because the
  near fine squares are most of what is on screen.
- The single-level numbers reproduce the first run within noise (FNV eye 2.04 both times).
  FNV's `main_render` baseline is lower this run (5–7 ms against ~7.9 ms), so compare
  deltas, not absolute times.
- Screenshots of single-level against two-level at FNV show no seam and no visible level
  boundary at 3 cm; the pixel differences are wind-animated grass.
- **FNV eye is noise-bound.** A fine radius of 1, 1.5 or 2 m picks the same 4 fine squares
  at this spawn, which sits on a patch column boundary, and all three read the same within
  noise; all-coarse (`fine 0`) read 1.28 ms against 1.86 ms in the same run. Across
  separate engine runs the identical two-level setting read 1.52, 1.64, 1.86 and 0.92 ms,
  with the patches-off baseline moving between 5.7 and 7.4 ms. Only interleaved
  comparisons within one run are meaningful here, and FNV eye sits on the 1.0 ms bar
  rather than clearly over it.

### 8.3 Stage 4.2 measurement (#7221, 2026-11-22)
Same harness as §8.2, 8 interleaved off/on rounds × 4 samples. Two-level lattice, 3 cm,
6 m, patch parallax on (the defaults), 16 patches resident. Every patch is in the TLAS
now. **Total** = patch compute + patch BLAS + the TLAS-build delta + the `main_render`
delta, ms. The `main_render` delta now includes ray queries traversing the patch BLAS.

| position | down | eye | graze | compute | patch BLAS | TLAS Δ |
|---|---|---|---|---|---|---|
| Skyrim tundra (2,−4) | **0.87** | **0.95** | **0.48** | 0.13–0.14 | 0.021–0.022 | 0.000 |
| Skyrim Riverwood (4,−11) | **0.53** | **0.78** | **0.42** | 0.12–0.14 | 0.019–0.021 | 0.000 |
| FNV Mojave (0,0) | **1.45** | **1.99** | **1.51** | 0.12–0.14 | 0.019 | 0.000 |
| FO4 Commonwealth (0,0) | **1.16** (sd 2.7) | **0.56** (sd 3.3) | **0.16** | 0.12–0.13 | 0.018 | 0.000 |
| Oblivion forest (0,0) | **0.82** | **0.55** | **0.14** | 0.13 | 0.020–0.022 | 0.000 |

- **The refit is not the cost.** All 16 patch BLAS refit in one batched call in
  0.018–0.022 ms, against the 0.5 ms bar and §8.1's ≈ 0.13 ms one-BLAS projection,
  because the batched call pays no per-BLAS serialize barrier. The TLAS-build bracket
  moves by under 0.002 ms with 16 more instances (1276 against 1260 at FNV;
  `rt.integrity` PASS, emitted = eligible).
- **Against the 1.0 ms bar**: every Skyrim and Oblivion pose and FO4 eye/graze pass; FO4
  down is inside its own noise; FNV fails at every pose (1.45–1.99), as in §8.2, where
  that position was shown to be noise-bound across runs.
- **The RT share, isolated in one run.** `terrain.patches rt off` keeps the patch draws
  out of the TLAS (rays then see the base terrain under them) while the raster, compute
  and BLAS refit stay the same. The three states, patches off / raster-only / with RT,
  ran interleaved, 8 rounds × 4 samples each. The `main_render` deltas are in ms. Each
  cell is raster − off / RT − raster:

  | position | down | eye | graze | state sd |
  |---|---|---|---|---|
  | Skyrim tundra | +0.54 / +0.13 | +0.40 / +0.24 | +0.06 / +0.18 | 0.16–0.50 |
  | Skyrim Riverwood | +0.33 / +0.13 | +0.23 / +0.19 | +0.27 / −0.11 | 0.13–0.38 |
  | FNV Mojave | +0.75 / +0.12 | +0.84 / +0.31 | +0.67 / +0.01 | 0.22–0.39 |
  | FO4 Commonwealth | +0.81 / +0.40 | +0.16 / +0.08 | +0.36 / +0.17 | 0.5–3.1 |
  | Oblivion forest | +1.20 / +0.32 | +0.16 / +0.07 | −0.19 / +0.12 | 0.42–0.72 |

  With an sd of 0.15–0.5 ms per state and n = 32, a delta carries roughly ±0.1 ms; FO4
  carries several times that. **Rays through the patch BLAS cost 0.0–0.3 ms, about
  0.15 ms typically**, and the TLAS build is unchanged. The raster-only state is the
  larger part: up to 1.2 ms looking down, and 0.7–0.8 ms at FNV at every pose. That
  state still fires every main-pass ray from the patch pixels, though; it only removes
  the patches as something rays can hit. So it is not raster cost. The two-switch and
  ray-family runs below show that rays from patch pixels carry nearly all of it. The `main_render`
  baselines here are higher than §8.2's, so the absolute ms are not comparable across
  runs; only the in-run deltas are.
- **The raster, broken down at FNV.** Six interleaved states in one run, patches out of
  the TLAS throughout, 8 rounds × 4 samples. The `main_render` deltas over patches-off
  are in ms; the sd per state is 0.12–0.44:

  | pose | defaults | amplitude 0 | parallax off | all coarse | all fine |
  |---|---|---|---|---|---|
  | down | +0.77 | +0.76 | +0.46 | +0.27 | +0.53 |
  | eye | +0.92 | +0.86 | +0.72 | +0.55 | +1.20 |
  | graze | +0.81 | +0.79 | +0.78 | +0.79 | +1.23 |

  - **The relief height is free.** At amplitude 0 the cost stays within 0.06 ms.
  - **The parallax march on patches costs 0.31 / 0.20 / 0.03 ms**, an upper bound on what
    4.3's residual march can save.
  - **Triangle density** is the other near-camera term. All-coarse saves 0.50 / 0.37 ms
    looking down / at eye level, and nothing at graze, where the two-level pool is
    already mostly coarse. All-fine adds 0.3–0.4 ms at eye and graze. All-fine coming in
    *under* the defaults looking down (0.53 against 0.77, sd 0.23) is unexplained.
  - **A floor of 0.3–0.8 ms remains even all-coarse**, growing toward graze. Relief,
    parallax and triangle count do not explain it. The patches draw before the base
    terrain they cover (lower mesh handles, same sort group), so the base is early-Z
    rejected under them rather than shaded twice. The capture and the switch runs below
    attribute the floor to rays fired from patch pixels.
- **RenderDoc capture at FNV eye level** (RenderDoc 1.45, per-draw `EventGPUDuration`
  plus pipeline statistics; patches on / off / all coarse). The patches add no pixels
  and no overdraw. With patches off, the base cell under the camera shades 409 k
  fragments. With patches on, the patches shade 359 k of them and the base the other
  48 k, with fragment invocations equal to samples passed in both captures. The same
  near-region pixels cost 1.10 ms on the patches against 0.70–0.87 ms on the base: a
  1.25–1.6× per-pixel premium. The two near patches cost 0.314 / 0.408 ms fine and
  0.278 / 0.375 ms coarse, so lattice density is worth only about 0.07 ms there.
  Ground cover is unchanged.
- **The premium is the main-pass rays.** Patches off/on were interleaved under the
  `r.dbg` bits, 6 rounds × 4 samples. Each cell is the patch delta in ms, with the
  patches-off `main_render` in brackets:

  | pose | baseline | no main rays (`0x40000000`) | no POM (`0x1`) | neither |
  |---|---|---|---|---|
  | down | +1.44 (5.37) | +0.07 (0.61) | +1.35 (5.21) | +0.05 (0.54) |
  | eye | +1.97 (7.59) | +0.06 (1.19) | +1.97 (7.37) | +0.08 (1.03) |
  | graze | +1.20 (6.53) | +0.26 (1.27) | +0.76 (6.42) | +0.25 (1.13) |

  With the main-pass rays off, the patches cost 0.05–0.08 ms (0.25 at graze), so their
  geometry is nearly free to raster. Parallax on patches matters only at graze
  (≈ 0.43 ms). The baseline deltas are higher than in the earlier runs; only in-run
  comparisons hold.
- **Split by ray family** (same harness, one family off at a time; patch delta in ms,
  patches-off `main_render` in brackets):

  | pose | baseline | no direct shadows (`0x8000000`) | no GI (`0x10000000`) | no reflection / glass (`0x20000000`) |
  |---|---|---|---|---|
  | down | +1.33 (5.45) | +0.52 (2.89) | +0.92 (3.31) | +1.21 (5.45) |
  | eye | +1.51 (7.82) | +0.63 (2.70) | +1.30 (6.51) | +2.02 (7.73) |
  | graze | +1.19 (6.33) | +0.40 (2.15) | +0.69 (5.49) | +0.82 (6.47) |

  The sd per state is 0.14–0.68. **Direct shadow queries from patch pixels carry most
  of the premium**, 0.8–0.9 ms at every pose. **The diffuse GI path is next**: about
  0.4–0.5 ms looking down and at graze, and within noise at eye level. Reflection and
  glass rays add nothing measurable (the eye cell's +0.5 is inside its sd), and
  shadows plus GI account for the whole baseline delta. The next two bullets show why
  those rays cost more from patch pixels.
- **Per-ray causes ruled out** (code reading, plus the ReSTIR run below):
  - *Ray count.* The shadow sample count (`rayBudget.directShadowSamples`) is
    frame-wide, the visibility mask comes from the light rather than the pixel, and
    the trace (`traceShadowTransmittanceDetailed`, closest-hit, opaque) is the same
    for patch and base pixels.
  - *The relief normal.* At amplitude 0, out of the TLAS, the patches have the base's
    normals and keep the raster-only cost (+0.76 against +0.77 ms looking down,
    table above).
  - *The patch BLAS.* The raster-only state keeps most of the premium.
  - *History.* Patch instances take the base cell's model matrix and stable surface
    ids (`u32::MAX − 1 − slot`, no sentinel collision under the reservoir mask), so
    they are never flagged as moved. With ReSTIR temporal reuse off (`0x40000`), or
    temporal and spatial reuse and the direct-radiance EMA off (`0x8000`), the
    premium does not move (same harness, patch delta in ms, sd 0.15–0.45 per state):

    | pose | baseline | no temporal | no ReSTIR | no main rays |
    |---|---|---|---|---|
    | down | +0.70 | +0.75 | +0.72 | +0.07 |
    | eye | +0.86 | +0.74 | +0.86 | +0.14 |
    | graze | +0.71 | +0.69 | +0.41 | +0.14 |

    The graze no-ReSTIR cell is about one sd. This run's baselines are 0.4–1.0 ms below
    the other runs', with the same binary, position and harness, so only in-run
    comparisons hold.
- **The premium is per pixel rather than per ray: small triangles.** Two experiments
  used temporary shader switches (not kept). Both were compiled into
  `triangle.frag.spv` and `triangle_early.frag.spv`. Opaque terrain, base and patch,
  draws with the early-test variant, so a switch compiled only into `triangle.frag.spv`
  never reaches it. A first helper-lane run did exactly that; its null result
  (+1.30 / +1.85 / +0.94 against +1.25 / +1.79 / +0.97) is two baselines.
  - *Per-pixel ray latency barely differs.* A subgroup shader-clock heatmap
    (`clock2x32ARB` around the direct-light and GI sections, 6 frames per state)
    averaged over the pixels the patches cover:

    | pose | direct, off → on | direct, flat | GI, off → on | GI, flat | rays off (direct) |
    |---|---|---|---|---|---|
    | down | 274 k → 290 k (+6 %) | 270 k (−2 %) | 251 k → 227 k (−9 %) | 222 k | 10 k / 8 k |
    | eye | 274 k → 298 k (+9 %) | 290 k (+6 %) | 196 k → 184 k (−6 %) | 177 k | 6 k / 6 k |
    | graze | 310 k → 327 k (+6 %) | 317 k (+2 %) | 134 k → 135 k (0 %) | 128 k | 6 k / 6 k |

    Values are cycles per pixel. "Flat" is amplitude 0 with the patches out of the
    TLAS. The other pixels are unchanged within 1–3 %. The rays are 97 % of the
    per-pixel time. Their latency from a patch pixel is within −9 % to +9 % of a base
    pixel's. That cannot account for a premium that RenderDoc measured at
    1.25–1.6× per pixel.
  - *Helper lanes carry a third or more.* With shadow and GI rays skipped in helper
    invocations (`gl_HelperInvocation`), the patch delta drops (same harness, ms,
    sd 0.22–0.59):

    | pose | baseline | helper-lane skip | no main rays |
    |---|---|---|---|
    | down | +1.48 | +0.94 | +0.08 |
    | eye | +1.87 | +1.06 | +0.15 |
    | graze | +0.99 | +0.65 | +0.14 |

    That is 0.35–0.81 ms, or 35–43 %. Eye-level screenshots with the skip differ from
    baseline no more than two baseline shots differ from each other: helper lanes'
    shading is never stored, and nothing after the ray branches takes derivatives of
    their results. The `aaRoughness` derivative is hoisted above them, and the AO
    fetch is screen-space. The patches-off frame also drops by 0.37–0.41 ms at eye and
    graze, which is about one sd, so the skip may pay on every dense mesh, not only
    the patches.
  - *The remainder* fits the same cause, but this is inferred, not measured. Edges
    of small triangles split 2×2 quads, so the same pixels fill more warps. Each warp
    then waits through the same ray latency even when its helper lanes fire nothing.
    That matches all-coarse saving 0.37–0.50 ms and all-fine adding 0.3–0.4 ms in the
    raster breakdown above. RenderDoc's fragment invocations do not count helper
    lanes, which is why they equal samples passed.

  **For 4.3:** the lever is screen-space triangle size, not the parallax march.
  Coarser triangles where they cover few pixels, or a lattice sized in screen space,
  is the patch-side fix. The helper-lane skip itself shipped engine-wide in #7261:
  `triangle.frag` and `water.frag`, plus the RT reflection ray below the
  `aaRoughness` hoist. That A/B ran with patches on, in one binary, with 32
  samples per state. It saved:

  | location | saved (ms) |
  |---|---|
  | FO4 MedTek | +0.43 to +0.55 |
  | Oblivion forest | +0.27 to +0.40 |
  | FNV Mojave | +0.23 to +0.47 (+0.16 to +0.43 with patches off) |
  | Skyrim Riverwood | +0.04 to +0.06 |
  | FNV Prospector Saloon | −0.14 to +0.08 (noise) |

  The skip had no visible effect on the image. Shots alternated between skip on
  and skip off at equal spacing. Pairs one step apart in different states
  differed no more than pairs two steps apart in the same state.
- **Raster-only is not a fallback.** At Riverwood (eye, on the riverbank) the raster-only
  frame darkens the bank's crease by up to 23 levels of 8-bit luma in a band of rows:
  rays leaving the patch surface hit the base terrain the patch covers. With the patches
  in the TLAS, the frame matches patches off to within 1–6 levels per band. So the
  switch stays a diagnostic, not a quality setting.
- **Sync validation** (`BYRO_VALIDATION=1`) at the tundra and FNV positions, all poses,
  patches on and off: no VUID or SYNC-HAZARD message.
- **Seams**: unchanged, zero gap and zero lip deviation everywhere.
- **Risk 3 (denoiser stability)**: six still-camera screenshots per state at eye level,
  per-pixel temporal standard deviation (8-bit luma), patches on / off: tundra
  0.80 / 0.84, Riverwood 10.3 / 10.6, FO4 1.54 / 1.55, Oblivion 8.9 / 9.5, FNV 2.60 / 2.39
  (p95 14.6 / 13.1). No position shows the patches raising the noise beyond FNV's
  small rise. Six frames is a coarse probe, not the stricter still-camera measurement §6
  asks for.
- **Ground cover**: with the patches in the TLAS, the first build dropped every plant
  under a patch (§7.2); with the tag the near grass is back.

### 8.4 Re-baseline after #7261 (2026-11-22)
#7261 stopped helper lanes tracing shadow, GI and reflection rays. This re-run uses the
§8.3 harness: release build of main at 0cbde6b5a, 8 interleaved off/on rounds × 4
samples, with the defaults (two-level lattice, 3 cm, 6 m, patch parallax on, patches in
the TLAS) and 16 patches resident. **Total** is defined as in §8.3. Cells are ms, with
§8.3 in brackets:

| position | down | eye | graze |
|---|---|---|---|
| Skyrim tundra (2,−4) | **0.76** (0.87) | **1.09** (0.95) | **0.32** (0.48) |
| Skyrim Riverwood (4,−11) | **0.52** (0.53) | **0.57** (0.78) | **0.39** (0.42) |
| FNV Mojave (0,0) | **1.08** (1.45) | **1.18** (1.99) | **0.91** (1.51) |
| FO4 Commonwealth (0,0) | **1.15**, sd 2.6–2.9 (1.16) | **0.33** (0.56) | **0.25**, sd 2.2 (0.16) |
| Oblivion forest (0,0) | **0.73**, sd 2.0–2.8 (0.82) | **0.51** (0.55) | **0.25**, sd 1.3–1.7 (0.14) |

The per-state sd is 0.10–0.49 ms except where shown. Compute is 0.12–0.17 ms, the
patch BLAS 0.019–0.023 ms, and the TLAS build changes by under 0.004 ms. Seams are zero
gap and zero lip deviation at every position and pose. No engine log has a validation
line.

- **The gain is where the triangles are densest.** FNV, whose spawn faces a slope,
  drops by 0.37–0.81 ms, as much as or more than the helper-lane share §8.3 measured.
  Skyrim and Oblivion move within noise, matching #7261's ~0.05 ms at Riverwood.
  These are separate runs and FNV's baseline drifts 0.4–1.0 ms between runs, so read
  the brackets as a rough comparison, not an in-run delta.
- **Against the 1.0 ms bar**, three poses are still over: FNV down (1.08), FNV eye
  (1.18) and tundra eye (1.09). Each is about one sd or less from the bar. Tundra eye
  is 0.14 ms above its §8.3 value, inside the sd of 0.25–0.42. FO4 down is noise-bound
  as before (±1 ms). Every other pose passes.
- **For 4.3** (§7.3): the selector now has to save about 0.1–0.2 ms at FNV and tundra
  eye level, not 0.5–1.0 ms. In §8.3, all-coarse saved 0.37–0.50 ms at FNV. That
  number predates #7261, so it may now be smaller; the `A_FINE` sweep measures it
  again.

### 8.5 Screen-space level selector sweep (2026-11-22)
The 4.3 selector (§7.3, step 2) compared against 4.1's distance rule. Same harness as
§8.4, interleaved in one run per position. The states were:
- patches off;
- the distance rule (`fine 2`);
- all coarse (`fine 0`);
- `afine` at 32, 64, 128 and 256 px².

The two render heights are the default 1280×720 window under FSR Quality (853×480) and
a 2560×1440 window meant to give 1707×960. **Correction (§8.9):** the window manager
clamps that window to the bench machine's 1920×1080 monitors, so the "960" rows below
actually rendered at 1280×696 (FNV) and 1235×675 (tundra). The camera's `fov_y` is 45°,
so `k` is 579.4 px at 480 lines. Each cell is the §8.3 total in ms, with the fine/coarse square split of the 16
resident in brackets:

| lines | position | pose | distance | all coarse | 32 px² | 64 px² | **128 px²** | 256 px² |
|---|---|---|---|---|---|---|---|---|
| 480 | FNV Mojave | down | 0.88 [4/12] | 0.67 | 0.83 [4/12] | 0.79 [4/12] | **0.80** [4/12] | 0.78 [2/14] |
| | | eye | 1.02 | 0.71 | 1.05 | 1.04 | **0.98** | 0.91 |
| | | graze | 0.95 | 0.63 | 0.90 | 0.86 | **0.86** | 1.07 |
| 480 | Skyrim tundra | down | 0.75 [4/12] | 0.42 | 0.77 [4/12] | 0.75 [4/12] | **0.78** [4/12] | 0.75 [4/12] |
| | | eye | 0.69 | 0.57 | 0.77 | 0.73 | **0.66** | 0.62 |
| | | graze | 0.58 | 0.58 | 0.54 | 0.62 | **0.59** | 0.60 |
| 696 | FNV Mojave | down | 1.87 [4/12] | 1.50 | 1.82 [8/8] | 1.91 [8/8] | **1.86** [4/12] | 1.89 [4/12] |
| | | eye | 1.57 | 1.09 | 1.23 | 1.18 | **0.75** | 0.80 |
| | | graze | 1.19 | 0.83 | 1.97 | 2.12 | **1.29** | 1.32 |
| 675 | Skyrim tundra | down | 1.56 [4/12] | 0.92 | 1.11 [7/9] | 1.33 [7/9] | **1.17** [4/12] | 1.19 [4/12] |
| | | eye | 1.38 | 1.35 | 1.45 | 1.40 | **1.38** | 1.17 |
| | | graze | 0.57 | 0.60 | 0.76 | 0.82 | **0.39** | 0.81 |

The per-state sd is 0.06–0.50 ms, except FNV at 696, where it is 0.6–1.07 ms. A cell
there is only good to about ±0.25 ms. No run logged a validation line.

- **Levels switch per whole 3.66 m square, so thresholds group into steps.** At 480
  lines, 32–128 px² pick the same squares as the 2 m rule: the 2×2 block around the
  camera. Only 256 px² at FNV drops two of them. At 675–696 lines, 32–64 px² add three
  or four squares and 128–256 px² match the 2 m rule again.
- **More fine squares cost and buy nothing measurable here.** At 675–696 lines the extra
  squares from 32–64 px² cost up to 0.9 ms (FNV graze). **128 px²** reproduces 4.1's
  selection at the bench resolution and is the default. Unlike the 2 m rule, it scales
  with the render height and `fov_y`. A lower render height or a wider field of view
  drops squares to coarse by itself.
- **All coarse is the floor**: 0.1–0.35 ms below the 2-level pool at 480 lines. That
  is the price of the fine lattice near the camera, and only screenshots, not timing,
  can say whether it is worth paying.
- **At 675–696 lines even all-coarse costs 0.6–1.5 ms**, against 0.4–0.7 ms at 480. The
  patches-off frame itself grows by half (FNV 5.3 → 7.8 ms looking down). The
  remaining cost follows the patch pixels, and a lattice selector cannot remove it.
  §8.3 traced it to the shadow and GI rays those pixels fire. That puts the 1.0 ms
  bar, set at the bench's 480 lines, in question: a per-resolution bar or a fraction
  of the frame would hold across resolutions. **Decision for review.**
- This data does not justify the third, coarser level (§7.3). The only lattice saving
  measured is the near squares going fine → coarse (the all-coarse column). What is
  left follows the patch pixels, not lattice density. Not measured: a coarser lattice
  on the far squares themselves.

### 8.6 Residual parallax on patches (2026-11-22)
§7.3 step 4, built. The harness is §8.4's, with 6 interleaved rounds × 4 samples. Each
position ran two binaries at identical poses: the new one, which marches the residual
depth on patch draws, and a saved build of the previous one, which marched the full
depth. Each run covered three states: patches off, patches with their march, and
patches with `parallax off`. The cells are `main_render` deltas over patches off, in ms:

| position | pose | full depth: march / no march | residual: march / no march |
|---|---|---|---|
| FNV Mojave | down | +0.94 / +0.67 | +0.98 / +0.83 |
| | eye | +1.33 / +1.02 | +1.22 / +0.95 |
| | graze | +0.53 / +0.61 | +0.76 / +0.77 |
| Skyrim Riverwood | down | +0.45 / +0.34 | +0.42 / +0.31 |
| | eye | +0.60 / +0.27 | +0.43 / +0.41 |
| | graze | +0.26 / +0.29 | +0.27 / +0.15 |

The sd per state is 0.14–0.43 ms. No run logged a validation line.

- **The cost does not change**, as expected. The march's step count follows the view
  angle, not the depth (6 looking down, 20 at grazing). So the march costs 0.0–0.3 ms
  over no march at either depth, inside §8.3's bound.
- **The picture does.** At FNV, where the stony slope fills the frame, the two
  binaries' no-march frames match within the frame-to-frame noise in the lower half of
  the screen (8-bit level, mean |Δ| 1.3–5.0, the same as each run's own repeat shot).
  So the two runs see the same scene. The residual frame differs from the full-depth
  frame by a mean 14–19 levels, and from no march by 20–25.
  - The full depth stacked 8 cm of march onto 3 cm of real relief, and its near stones
    smear along the view direction.
  - The residual shows rounder stones with clean occlusion.
  - No march flattens them to streaks.

  At Riverwood the river and the grass move between frames (noise floor 6–21 levels),
  so no difference there clears the noise. That position is not evidence either way.
- **Default**: the residual march stays on. `terrain.patches parallax off` remains the
  A/B switch.

### 8.7 Patch range sweep (2026-11-23)
Is the new range setting (§7.3 step 5) a cost lever, either for meeting the bar at FNV
or for the quality presets? Same harness as §8.6, one release binary at HEAD
`f217a4619`. States were patches off and the range at 6 (the default), 5 and 4 m,
interleaved in one run per position at 853×480. Each cell is the median
`main_render_ms` change from patches off; the sd is in brackets.

| position | pose | off (ms) | 6 m | 5 m | 4 m |
|---|---|---|---|---|---|
| FNV Mojave | down | 5.39 | +0.90 (0.12) | +1.00 (0.19) | +0.93 (0.12) |
| | eye | 7.12 | +1.08 (0.22) | +1.20 (0.32) | +1.16 (0.40) |
| | graze | 7.40 | +0.49 (0.45) | +0.59 (0.46) | +0.44 (0.47) |
| Skyrim Riverwood | down | 2.01 | +0.39 (0.11) | +0.40 (0.18) | +0.41 (0.10) |
| | eye | 2.03 | +0.44 (0.12) | +0.43 (0.10) | +0.43 (0.09) |
| | graze | 2.70 | +0.24 (0.09) | +0.13 (0.02) | +0.15 (0.02) |

The setting did take effect. Mean per-pixel difference from the first 6 m frame, in
levels, with the share of pixels over 8 levels:

| FNV pose | second 6 m shot (noise) | 5 m | 4 m | off |
|---|---|---|---|---|
| down | 1.6 / 2.6 % | 1.2 / 1.4 % | 2.3 / 4.3 % | 17.8 / 55 % |
| eye | 2.6 / 7.5 % | 5.0 / 16 % | 10.0 / 30 % | 20.8 / 57 % |
| graze | 4.2 / 14 % | 6.4 / 22 % | 9.8 / 31 % | 14.9 / 42 % |

At Riverwood the noise shot already differs by 6–14 levels, as in §8.6, so the images
there can't be judged. No run logged a validation line.

- **Range isn't a cost lever.** From 6 m down to 4 m no pose moves by more than its sd,
  even where the frame visibly loses relief (FNV eye, 30 % of pixels). The squares
  dropped at 4–6 m are a small share of the patch pixels at these poses, and they are
  coarse, so they cost little. The premium sits in the squares nearest the camera.
  The default stays at 6 m.
- **FNV stays over the bar** at the eye pose: +1.08 ms at 6 m. That repeats §8.4. The
  levers measured so far:
  - turning the patches off;
  - forcing all squares coarse, which saves 0.1–0.35 ms at 480 lines (§8.5).
    A threshold of 512 px² does that at 480 lines. **Correction (§8.9):** not at
    higher render heights. The selector measures each square from its nearest
    point, which for the squares around the camera's feet is closer than the 1 m
    this bullet first assumed. At 696 lines 512 px² still leaves 2 squares fine at
    FNV and 4 at Riverwood.

  Neither is a free default: all-coarse looks flatter at eye level (§8.2).
- **For the presets**, the measured trade is patches off (amplitude 0), which saves the
  whole 0.4–1.1 ms. **Low now sets it**, and Medium, High and Ultra name the default
  amplitude so a switch away from Low restores it. All coarse (threshold 512) sits in
  between, but its look has only been judged by eye (§8.2); §8.8 measures it. A
  shorter range buys nothing measurable.

### 8.8 Coarse-only patches with residual parallax (2026-11-23)
§8.2 judged all-coarse patches flatter at eye level, but that was before §8.6 cut the
parallax march to the residual depth. Does a threshold of 512 px² (every square coarse
at 480 lines; not above it, §8.9) still lose visible relief? Same harness and binary as
§8.7, at 853×480. States were patches off, the default 128 px², and 512 px². The
status line confirmed the split each time:
- FNV: 2 fine and 14 coarse at 128 px²;
- Riverwood: 4 fine and 12 coarse;
- every square coarse at 512 px², at both positions.

A first Riverwood run drifted as the weather and the time of day moved. Its patches-off
frame went from 5.5 to 11 ms, so it was discarded. The run below has the clock pinned
at noon and paused (`time.set 12:00`, `time.pause`). Its sky readout (`sky.dump`)
matched at the start and end, and its patches-off frame is the §8.7 one again.

| position | pose | off (ms) | 128 px² (default) | 512 px² (coarse only) |
|---|---|---|---|---|
| FNV Mojave | down | 4.97 | +0.75 (0.31) | +0.36 (0.18) |
| | eye | 5.48 | +0.83 (0.25) | +0.51 (0.31) |
| | graze | 5.06 | +0.60 (0.15) | +0.45 (0.13) |
| Skyrim Riverwood | down | 1.95 | +0.42 (0.13) | +0.33 (0.11) |
| | eye | 2.02 | +0.43 (0.31) | +0.34 (0.13) |
| | graze | 2.74 | +0.14 (0.15) | +0.28 (0.23) |

Image difference from the first 128 px² frame (mean levels / share over 8 levels):

| position | pose | second 128 px² shot (noise) | 512 px² | off |
|---|---|---|---|---|
| FNV | down | 1.7 / 2.7 % | 3.5 / 9.7 % | 17.6 / 55 % |
| | eye | 2.6 / 7.3 % | 4.1 / 13 % | 20.5 / 56 % |
| | graze | 3.6 / 12 % | 4.4 / 14 % | 14.0 / 40 % |
| Riverwood | down | 6.2 / 23 % | 6.8 / 26 % | 7.9 / 31 % |
| | eye | 13.4 / 37 % | 13.5 / 38 % | 16.9 / 49 % |
| | graze | 7.5 / 21 % | 6.9 / 20 % | 10.2 / 32 % |

No run logged a validation line.

- **Coarse only saves 0.15–0.4 ms at FNV**, and up to about 0.1 ms at Riverwood,
  which is within the spread there. That matches §8.5.
- **No visible loss at either position.** At FNV, coarse only sits just above the
  noise floor and far from patches off. Side-by-side crops of the near ground keep
  the stones' shapes and shadows. The §8.2 flatness doesn't reproduce with the
  residual march. Riverwood's moving cloud shadows, falling flakes and river shimmer
  hide anything smaller than its noise floor, so it only rules out a large loss.
- **Medium now uses 512 px².** The default stays at 128 px². Everything here was
  measured at 480 render lines. At twice the render height each coarse triangle covers
  four times the pixels, which is where the fine mesh should start to show. 128 px²
  scales with the render height by itself. Changing the default needs a visual check
  at a higher render height first (§8.9).
- `weather.force` refused a bare `0010E1F2` (SkyrimClear_A) because it parsed the
  FormID decimal-first. That is fixed separately, so a future run can pin the
  weather too.

### 8.9 The "960-line" runs rendered at 696 (2026-11-23)
A run meant to repeat §8.8 at 960 render lines found that the bench machine can't
produce them with a window. Its monitors are 1920×1080, so a requested 2560×1440
window is clamped to 1920×1045, which FSR Quality renders at 1280×696. Every
`render_extent` the bench printed confirms it:

| run | render extent |
|---|---|
| §8.5 FNV "960" | 1280×696 |
| §8.5 tundra "960" | 1235×675 |
| this run, FNV and Riverwood | 1280×696 |

§8.5's rows are relabelled. A render height above about 700 lines needs native
rendering at the largest window the monitor allows (`--upscaler taa` at 1920×1080,
about 1045 lines).

**At 696 lines**, same harness and binary as §8.8. The clock was pinned at noon at
both positions and Riverwood's weather was locked (`weather.force 0x0010E1F2 lock`,
SkyrimClear_A). The sky readout matched at the start and end, and no run logged a
validation line. Each cell is the median `main_render_ms` change from patches off,
with the sd in brackets and the fine/coarse split of the 16 resident squares:

| position | pose | off (ms) | 128 px² (default) | 512 px² |
|---|---|---|---|---|
| FNV Mojave | down | 8.86 | +1.38 (0.20) [4/12] | +1.45 (0.23) [2/14] |
| | eye | 7.70 | +1.27 (1.19) | +1.10 (1.01) |
| | graze | 7.62 | +0.89 (0.29) | +1.05 (0.40) |
| Skyrim Riverwood | down | 7.95 | +1.45 (0.11) [4/12] | +1.45 (0.18) [4/12] |
| | eye | 8.35 | +1.62 (0.18) | +1.62 (0.21) |
| | graze | 10.54 | +0.87 (0.97) | +0.87 (1.09) |

- **512 px² is not "every square coarse" above 480 lines.** The selector measures each
  square from its nearest point, and for the squares around the camera's feet that
  point is closer than the 1 m §8.7 assumed. At 696 lines 512 px² still leaves 2 fine
  squares at FNV and the same 4 as the default at Riverwood. Medium therefore means
  "every patch coarse at 480 lines, all but the nearest above it". The
  descriptions in §7.3, the preset file and `launcher.md` now say so.
- **512 px² saves nothing measurable at 696 lines.** At FNV it moves only 2 squares,
  and the differences are within the spread. At Riverwood the two states select the
  same squares and measure the same, a check on the harness itself.
- **The premium grows with the render height**: 0.9–1.6 ms at 696 lines, against
  0.4–1.1 ms at 480 (§8.7, §8.8). That bears on the per-resolution bar (§8.5).
- **Images**: at FNV the two states differ by no more than two default shots do (for
  example 2.6 vs 2.8 levels at eye). Riverwood's down pose is unusable: two shots of
  the same state differ by 50 levels.

The visual check that decides the default runs natively at about 1045 lines (§8.10).

### 8.10 Native ~1045-line run (2026-11-23)
The run §8.9 called for, with native TAA (`--upscaler taa`) at the largest window the
monitor allows. It rendered at 1853×1013 at FNV and 1920×1045 at Riverwood; FNV's window
came out a little smaller. Same harness, binary and pins as §8.9. The sky readout
matched at the start and end, and no run logged a validation line. States were patches
off, the default 128 px² and 512 px²:

| position | pose | off (ms) | 128 px² | 512 px² |
|---|---|---|---|---|
| FNV Mojave | down | 9.54 | −3.62 (4.81) | −3.74 (4.85) |
| | eye | 6.65 | +0.63 (0.10) | +0.67 (0.13) |
| | graze | 7.54 | +0.61 (1.34) | +0.60 (0.09) |
| Skyrim Riverwood | down | 6.77 | +1.43 (0.20) | +1.57 (0.34) |
| | eye | 7.94 | +1.27 (1.90) | +1.27 (0.07) |
| | graze | 11.04 | +0.77 (0.09) | +0.76 (0.12) |

- **At ~1000 lines, 512 px² selects exactly the squares the default does**: 4 fine and
  12 coarse at every pose and both positions. So the two columns measure the same
  state twice, and their agreement is a check on the harness. The slider tops out at
  512 px², so no threshold forces all-coarse at this height: **Medium is identical to
  High here**, and its saving exists only near the 480-line default render height.
- **The premium at ~1000 native lines** is +0.6 ms at FNV (eye, graze) and +0.8–1.6 ms
  at Riverwood. It does not compare directly with §8.9's 696 lines, because the
  upscaler differs (TAA vs FSR), and that changes the patches-off frame too. FNV's down
  pose is unusable: its patches-off frame swung by ±4.8 ms.
- **The visual question is still open.** It needs a forced all-coarse state (the
  console's `fine 0`), not a threshold, at this height. If coarse-only shows no
  visible loss there, a setting that means "all coarse" at any resolution would let
  Medium save at every height. The options are a higher slider cap or a separate
  coarse-only switch.

## 9. Review decisions (Matías, 2026-11-21)
1. **Upward-only displacement, base mesh kept (§3.3)**: accepted. No holes, no zero-mean
   relief.
2. **Measurement spike first (§8)**: approved as written, with the 1.0 ms total / 0.5 ms
   refit bar. Tracked as #7150.
3. **Defaults**: 3 cm amplitude and 6 m radius as the starting points.
4. **BTXT-only tile allocation (risk 5)**: fixed first, as its own issue, #7149.
5. **Phase 4 is still wanted**, for the centimetre-scale silhouettes and RT shadows that
   parallax cannot give.
