# Terrain displacement near the camera — TSD-4 design note (#6198)

**Status**: REVIEWED and approved by Matías on 2026-11-21 (#6198); the decisions are in §9.
Drafted 2026-10-22. Follows TSD-1 (#6195 height maps), TSD-2 (#6196 height blend) and
TSD-3 (#6197 parallax), which are shipped. #7149 (BTXT-only tile slots) and the §8 spike
(#7150, result in §8.1: go) are done. Stage 4.1 (#7152) is built (§7.1) and measured
(§8.2), now with a two-level lattice (fine within 2 m, coarse beyond); it awaits review
before 4.2.

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

## 9. Review decisions (Matías, 2026-11-21)
1. **Upward-only displacement, base mesh kept (§3.3)**: accepted. No holes, no zero-mean
   relief.
2. **Measurement spike first (§8)**: approved as written, with the 1.0 ms total / 0.5 ms
   refit bar. Tracked as #7150.
3. **Defaults**: 3 cm amplitude and 6 m radius as the starting points.
4. **BTXT-only tile allocation (risk 5)**: fixed first, as its own issue, #7149.
5. **Phase 4 is still wanted**, for the centimetre-scale silhouettes and RT shadows that
   parallax cannot give.
