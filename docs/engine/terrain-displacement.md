# Terrain displacement near the camera — TSD-4 design note (#6198)

**Status**: DRAFT for review — no code until this is signed off (the issue's acceptance
criterion). 2026-10-22. Follows TSD-1 (#6195 height maps), TSD-2 (#6196 height blend) and
TSD-3 (#6197 parallax), which are shipped.

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
| Vertices are **104 B**; raster and RT hit shading both read the *global vertex pool* (cap 4 M verts = 416 MB, 16 M indices). Mesh handles are **never reused**; every upload goes through `pending_vertices` and a global geometry-SSBO (re)build with an idle-reclaim path. | `mesh.rs`, `mesh/geometry_ssbo.rs` |
| Global vertex buffer usage = `STORAGE_BUFFER \| VERTEX_BUFFER` (+ AS build input) — compute can write it. | `geometry_ssbo.rs:345` |
| Skinned meshes already run **compute → position buffer → per-frame BLAS refit** (`skin_vertices.comp`, `blas_skinned.rs`; rebuild after 600 refits to reset bounds). | `skin_compute.rs`, `acceleration/constants.rs` |
| Cell crossings are a measured hot path: single-hash apply up to ~300 ms, batched BLAS submission 20–55 ms, residual frame spikes "tied to global geometry work". | `exterior-readiness-plan.md` [anchor] |
| `terrain.seams` audits **LAND height agreement between adjacent resident cells on the CPU**; it does not look at GPU geometry. | `TerrainSeamStats`, `terrain_seam::check_seam` |
| TSD-3 parallax and any splat tile exist only for cells **with splat layers**; BTXT-only cells have no tile slot, so their generated base height map is not bound. | `terrain.rs` ("Allocate a terrain tile slot only when…"), #6195 status comment |

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
cap of 4 M vertices (416 MB) it is **2.5 % of the vertex pool**. The 5.7 cm lattice is the
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
5. **BTXT-only cells** have no splat tile, so they get no patches (and, honestly, no TSD-3
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

## 8. The spike, and its go/no-go bar (proposed — adjust it)
Measure on the 4070 Ti, release build, at the five TSD positions: (a) refit time for 197 k
triangles of a displaced grid, (b) compute displace time for 101 k vertices, (c) added raster
time, (d) the extra `main_render` with patches resident.
**Go** if the total added GPU time at defaults is ≤ **1.0 ms** (the bar TSD-2/3 held) and
refit alone ≤ 0.5 ms. **Otherwise** fall back to static patches (risk 1) or drop Phase 4 and
keep parallax.

## 9. Decisions I need from you
1. **Upward-only displacement, base mesh kept (§3.3)** — acceptable, or do you want holes /
   zero-mean relief despite the cost?
2. **Go ahead with the measurement spike (§8)** as the next step, with that bar?
3. Geometry amplitude default (3 cm) and radius (6 m) — fine as starting points?
4. Fix BTXT-only tile allocation first (risk 5), since it also repairs a TSD-3 gap?
5. Is Phase 4 still wanted, given that you judged parallax-on to look better and the
   remaining visual differences are silhouettes and RT shadows at centimetre scale?
