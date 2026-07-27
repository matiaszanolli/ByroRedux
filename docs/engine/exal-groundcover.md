# EXAL — Ground Cover (procedural grass)

A focused sub-document of [`exal.md`](exal.md), in the same relation to it as
the `charal-*-ruleset.md` files are to [`charal.md`](charal.md). EXAL owns the
outdoors environment; **ground cover** is the vegetation stratum that sits on
the terrain surface — grass, ferns, moss, low scrub.

**Status**: PROPOSED (design, 2026-07-26). Implementation rolls out per §8.

**Goal**: grass that reads as an *organic, continuous ground stratum* rather
than a set of authored patches, generated procedurally from terrain-derived
signals, costing no per-blade CPU work and no ray-tracing budget in its first
form.

**Deliberate divergence from Gamebryo / Creation Engine.** This is the one
exterior category where we are *not* reproducing the source engines' model.
Everywhere else EXAL's job is to translate per-game data into a canonical form
faithfully; here the per-game data is the *cause* of the artifact we want to
eliminate, so it is demoted to a palette hint (§7) and the placement authority
moves into the engine.

---

## 1. Why not the Creation Engine model

Bethesda's grass is placed by mapping a `GRAS` record onto an `LTEX` landscape
texture and scattering it on a fixed per-cell grid wherever that texture is
painted. The characteristic look — grass in *patches* — is not an artistic
choice, it is three mechanical properties of that scheme:

1. **Density is keyed to texture identity, not to ground conditions.** A cell
   quadrant either paints `LTEX_Grass` or it doesn't, so density changes in a
   hard step exactly where one splat layer stops and another begins. The
   boundary of a texture becomes the boundary of the vegetation.
2. **The placement authority is coarse.** The per-quadrant alpha grid is 17×17
   over a 2048-unit quadrant — one authority sample per ~120 units, roughly one
   per full-detail terrain quad
   ([`terrain.rs:59`](../../byroredux/src/cell_loader/terrain.rs#L59)). Detail
   finer than that cannot exist, so clumping is quantised to the paint grid.
3. **There is a hard cull radius.** Grass exists inside it and does not exist
   outside it, so the boundary sweeps across the world with the camera and the
   transition is a pop, which the eye reads as "the grass is a layer that was
   switched on" rather than as part of the ground.

Each of the three has a direct counter, and together they *are* the design:

| Cause | Counter (this document) |
|---|---|
| Density keyed to texture identity | Continuous **density field** from ground conditions; splat weights are one input among several, contributing a smooth affinity rather than a boolean (§3) |
| Coarse placement authority | Placement authority is **procedural noise** evaluated per candidate point at arbitrary resolution; terrain only supplies the low-frequency term (§3) |
| Hard cull radius | A **continuous LOD chain** ending in a terrain detail layer that is present at *all* distances, so geometry fades into something already showing the same colour and density (§6) |

---

## 2. The substrate that already exists

No new engine subsystems are required. Ground cover consumes what the exterior
pipeline already builds:

- **Terrain geometry** — each exterior cell is a 33×33 vertex grid over
  4096 units (128-unit spacing), spawned as a mesh entity
  ([`terrain.rs`](../../byroredux/src/cell_loader/terrain.rs)). Positions and
  normals live in the renderer's global vertex SSBO.
- **Splat weights** — up to 8 layers per cell, resolved per vertex and packed
  into the `splat0` / `splat1` `RGBA8` vertex attributes, with layer texture
  indices in the terrain-tile SSBO
  (`INSTANCE_FLAG_TERRAIN_SPLAT`, [`constants.rs:205`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs#L205)).
- **GPU-driven dispatch precedent** — `cluster_cull.comp` already runs a
  3456-workgroup compute pass whose results feed the frame, and
  `vkCmdDrawIndexedIndirect` with `drawCount > 1` is enabled
  ([`device.rs:616`](../../crates/renderer/src/vulkan/device.rs#L616)).
- **TLAS exclusion precedent** — distant LOD terrain builds no BLAS and spawns
  with `IsLodTerrain` so the renderer keeps it out of the TLAS
  ([`terrain_lod.rs:10-12`](../../byroredux/src/cell_loader/terrain_lod.rs#L10-L12)).
  Ground cover needs the same exclusion (§5).
- **Wind** — WTHR's `wind_speed` byte is already parsed and currently drives
  only cloud scroll (`WeatherDataRes::wind_speed`,
  [`components.rs:914`](../../byroredux/src/components.rs#L914)).
- **Water height** — WATAL resolves the per-cell water plane, which the density
  field needs for shoreline behaviour ([`watal.md`](watal.md)).

### The binding constraint

`MAX_INSTANCES` is `0x40000` (262144) **for the entire scene**, and
`instance_custom_index` is a 24-bit field
([`constants.rs:135-143`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs#L135-L143)).
Blades therefore can never be scene instances or TLAS entries — not as a matter
of cost but of structure. Everything below follows from that: blades exist only
as a GPU-side point list consumed by an indirect draw, and they are never ECS
entities, never `GpuInstance` rows, never BLAS builds.

---

## 3. The density field

The core of the design. A single scalar `d ∈ [0,1]` evaluated **per candidate
point**, on the GPU, inside the scatter pass — never precomputed per cell,
never stored per blade.

```
d = affinity(splat)                   // what the ground is made of
  × slope_gate(normal)                // grass does not grow on cliffs
  × moisture(height_above_water)      // shoreline lush, ridgeline sparse
  × shelter(curvature)                // clumps settle into concavities
  × clump(noise)                      // the organic term
  × distance_fade(view)               // LOD, §6
```

Each factor and where it comes from:

- **`affinity(splat)`** — the 8 splat weights bilinearly sampled from the
  surrounding terrain vertices, dotted with a per-layer canonical
  `cover_affinity: f32`. This is the key reframing: a layer does not *enable*
  grass, it *weights* it. A dirt layer at 0.15 and a grass layer at 0.9 blend
  into a continuous gradient wherever the painter feathered them, and the
  vegetation boundary stops coinciding with the texture boundary.
- **`slope_gate(normal)`** — smoothstep on the terrain normal's Y component.
  Ground cover thins out and then stops on steep faces. This alone removes the
  single most artificial thing about vanilla grass, which happily carpets
  cliff faces wherever the texture was painted.
- **`moisture(height_above_water)`** — signed distance to the WATAL water plane,
  falling off with altitude above it. Produces lush shorelines and sparse high
  ground for free, and reads as a reason for the distribution rather than a
  rule.
- **`shelter(curvature)`** — discrete Laplacian of the heightfield over the
  terrain grid. Concave ground accumulates; convex ground sheds. Gives the
  distribution a relationship with the landform.
- **`clump(noise)`** — the organic term. Two octaves: a Worley/cellular field
  at ~600 units for clump structure, times a low-amplitude fBm at ~4000 units
  for regional variation. This is what replaces the 17×17 paint grid as the
  high-frequency authority, and it is scale-free, so density detail is
  available at whatever resolution the scatter asks for.

### Where the evaluation lives

**In GLSL only, once.** A Rust mirror of this function would be a second source
of truth for a formula that must match exactly, and this codebase has already
been bitten by exactly that shape (`feedback_shader_struct_sync`: `GpuInstance`
duplicated across four shader files, all required to stay in lockstep).

Instead: every *parameter* — affinity table, slope thresholds, noise
frequencies, falloff constants — is canonical in Rust and emitted into the
generated `shaders/include/shader_constants.glsl` by `build.rs`, the mechanism
the renderer already uses. Rust owns the numbers, GLSL owns the formula,
and there is exactly one copy of each.

The cost is that the field is not unit-testable in the usual sense. That is
accepted and handled in §9 by pinning it with density-histogram telemetry over
real cells rather than pretending a mirrored Rust function is a test.

---

## 4. Scatter and draw

Per frame, for terrain chunks inside the ground-cover radius:

1. **Chunking.** Each exterior cell subdivides into an 8×8 grid of 512-unit
   chunks. The chunk is the unit of dispatch, culling and LOD selection.
2. **Compute scatter** (`groundcover_scatter.comp`) — one workgroup per visible
   chunk, mirroring `cluster_cull.comp`'s shape. Each thread draws candidate
   points from a scrambled blue-noise tile (chunk hash as the scramble seed, so
   placement is stable frame to frame and across sessions — a blade does not
   move when the camera does), evaluates `d` at each, and stochastically accepts.
   Accepted points atomically append to a per-chunk slice of the blade buffer
   and bump the chunk's `VkDrawIndexedIndirectCommand` instance count.
3. **Blade record.** ~16 bytes: packed chunk-relative position, a seed word, a
   species index, and the evaluated density (reused downstream for width
   compensation and RT proxy opacity). Not a `GpuInstance` — a separate, much
   smaller SSBO that no other pass reads.
4. **Draw.** One `vkCmdDrawIndexedIndirect` over the chunk's draw list, with a
   shared static index buffer describing one blade topology.

### Blade geometry

Generated in the vertex shader from the seed — there is no blade mesh and no
per-blade vertex data. A blade is a quadratic Bezier ribbon: base point, a
control point displaced by the bend, and a tip. `gl_VertexIndex` selects the
segment and the side; height, width, bend stiffness, twist and colour jitter
are all derived from the seed word. Segment count comes from the LOD tier (§6),
so the *same* shader emits a 3-segment near blade and a 1-segment far blade
with no branch on anything but a per-chunk constant.

This is where "efficient" actually comes from: no vertex fetch, no per-blade CPU
touch, no instance-buffer growth, and the entire visible grass population is a
handful of indirect draws.

---

## 5. Ray-tracing boundary

Decided: **receive-only first, coarse proxy second.**

### Phase 1 — receive-only

Grass rasterizes in the main geometry pass and traces the existing shadow ray
and GI sample like any other fragment, so it is correctly lit, shadowed by the
world, and colour-bled into by nearby surfaces. It contributes nothing back: no
BLAS, no TLAS entry, no ray-budget cost.

This needs a TLAS-exclusion marker. `IsLodTerrain` already does exactly this job
for distant terrain, so rather than adding a second special case the renderer's
TLAS query should generalise to a marker component (working name
`ExcludedFromTlas`) that both `IsLodTerrain` and ground cover carry. That
refactor is small and belongs in Phase 1 rather than being deferred — a third
ad-hoc exclusion is how this becomes a per-feature `if` chain.

### Phase 2 — the proxy shell

Receive-only grass casts no shadow, and grass that casts no shadow reads as
pasted onto the terrain — which is the exact failure this whole document exists
to avoid. The fix is not to put blades in the TLAS but to put a *shell* there:
per chunk, a low-poly sheet following the terrain surface displaced upward by
the local mean blade height, carrying the chunk's mean density.

In the ray hit path the shell is treated as a stochastically transparent
medium: a ray hitting it is absorbed with probability proportional to the local
density, otherwise it passes through. That yields soft, correctly-shaped grass
shadows and a plausible GI contribution at the cost of one coarse BLAS per
chunk — a rounding error against the blade count it stands in for, and the
shell can be refit rather than rebuilt as density changes.

### Not viable, for the record

Blades as TLAS instances. At `MAX_INSTANCES = 262144` a single mid-density
chunk would exhaust the scene budget, and BLAS memory would exceed the whole
4 GB VRAM target (`feedback_vram_baseline`) by orders of magnitude. This is
recorded so it is not re-proposed.

---

## 6. The LOD chain

Four tiers, each blending into the next. The distances are starting points to
be tuned against real worldspaces, not derived constants:

| Tier | Range (units) | Form |
|---|---|---|
| 0 | 0 – 2 000 | 3-segment blades, full density |
| 1 | 2 000 – 6 000 | 1-segment blades, reduced density, compensating width |
| 2 | 6 000 – 15 000 | clump cards — a few crossed quads per clump, from a baked atlas |
| 3 | 15 000+ | terrain detail layer only |

Two rules make this continuous rather than three visible thresholds:

**Density fades stochastically, and survivors widen.** Blades do not vanish at a
boundary; the acceptance threshold in the scatter rises smoothly with distance
while accepted blades grow slightly wider, holding total coverage roughly
constant. The population thins without the silhouette thinning.

**The terrain detail layer is always on.** The terrain fragment shader modulates
its albedo and normal with a ground-cover detail texture whose strength is *the
same density field*, at every distance including zero. Grass geometry is drawn
**on top of** a surface that already shows the right colour, the right
variation, and the right density. So when the last geometry tier fades out,
what remains underneath is already a match — there is nothing to pop *to*.

This is the single most important item in the document for the stated goal.
Tier 3 is not "where grass stops"; it is where grass stops being geometry.

---

## 7. Per-game translation — palette only

Placement is entirely engine-authored. Per-game data enters at exactly one
point, the EXAL translate boundary, and only to populate the **species palette**:

```
GroundCoverSpecies {
    height_range:    (f32, f32),
    width_range:     (f32, f32),
    colour_gradient: [Rgb; 2],   // base → tip
    bend_stiffness:  f32,
    cover_affinity:  f32,        // weight into the affinity term, §3
    climate_weight:  ClimateWeights,
}
```

A worldspace resolves to a palette of species. The density field picks among
them per point by weight × local conditions, so species transitions are
gradients rather than boundaries — the same anti-patch principle applied one
level up.

Sources, in precedence order, all optional:

1. **`GRAS` records** — today parsed only as a `MinimalEsmRecord` stub in the
   long-tail index ([`index.rs:271`](../../crates/plugin/src/esm/records/index.rs#L271))
   with no consumer, so nothing is being unwound. A `GRAS` yields one species:
   its model's texture drives the colour gradient, its dimensions the size
   ranges. Its *density and placement fields are ignored* — that is the whole
   point of this design.
2. **`LTEX` names** — a keyword table maps landscape texture names to
   `cover_affinity`, so a worldspace with no `GRAS` data still gets sensible
   per-layer weighting.
3. **Default palette** — a built-in temperate-grass species. Guarantees that a
   game with no vegetation data at all still renders organic ground cover, which
   is the "generic" requirement stated for this feature.

Per the format-translation doctrine (`feedback_format_translation`), all three
resolve at the parser→canonical boundary. Nothing downstream of the boundary —
no shader, no scatter pass, no LOD tier — ever branches on which game supplied
the palette, or on whether one did.

Oblivion's WTHR `grass_dimmer`
([`weather.rs:147-149`](../../crates/plugin/src/esm/records/weather.rs#L147-L149))
is already parsed and folds naturally into the palette's colour gradient as a
per-weather multiplier.

---

## 8. Wind

`WeatherDataRes::wind_speed` translates into a canonical `WindField`:

```
WindField { direction: Vec2, speed: f32, gust_amplitude: f32, gust_frequency: f32 }
```

The blade vertex shader samples a 2D flow-noise field at the blade base,
advected along `direction` at `speed`, and bends the Bezier control point
accordingly. Because neighbouring blades sample a *continuous* field at nearby
points, they bend together — you get travelling gust waves crossing a meadow
rather than per-blade jitter, which is most of what sells grass as a living
surface. A per-blade phase offset from the seed keeps the response from being
perfectly lockstep.

Wind is deliberately not simulated and not collided in this design; §9 records
interaction as out of scope for now.

---

## 9. Rollout order

Each phase is independently useful and independently reviewable.

- **Phase 0 — canonical types + boundary.** `GroundCoverSpecies`, palette,
  `WindField`, the affinity table, the EXAL translate site, and the `LTEX`
  keyword map. Pure CPU, fully unit-testable, no rendering.
- **Phase 1 — scatter.** `ExcludedFromTlas` generalisation, chunking, the
  density field in GLSL, `groundcover_scatter.comp`, and debug point rendering
  of accepted candidates over real terrain. This is where the distribution is
  judged — before any blade exists.
- **Phase 2 — blades + wind.** Vertex-shader Bezier ribbons, the wind field,
  the near tier only.
- **Phase 3 — LOD chain.** Tiers 1–3, the stochastic density fade, and the
  always-on terrain detail layer. The tier-3 layer should land *first* within
  this phase, so every later tier is authored against a correct backdrop.
- **Phase 4 — RT proxy shell.** Per-chunk shell, stochastic-absorption hit
  handling, refit on density change.
- **Phase 5 — per-game palette.** `GRAS` → species, `grass_dimmer`, and the
  per-worldspace palette resolution.

---

## 10. What stays out of scope

- **Trees.** SpeedTree `.spt` content and the distant tree LOD ring are a
  separate concern with a separate authority (`exal.md` §5); ground cover stops
  at low scrub.
- **Harvestable flora.** `FLOR` records are gameplay entities with inventories
  and activation, not ground cover. They stay ordinary placed references.
- **Grass interaction.** Blades reacting to the player, NPCs, or physics bodies.
  Wants a displacement texture rendered from nearby dynamic entities; deferrable
  without changing anything above, and worth doing only once Phase 4 lands.
- **Seasonal / snow variation.** The palette has the room for it
  (`climate_weight`), but driving it needs a canonical season concept EXAL does
  not currently have.
- **Authored placement.** No mechanism for hand-placing or hand-suppressing
  grass in a region. If it turns out to be needed, it belongs as an extra
  multiplicative term in §3, not as an escape hatch around the field.

---

## 11. Open questions requiring real-data verification

Not answerable from source reading; each needs a `--bench-hold` session against
real worldspaces before the phase that depends on it.

1. **Terrain attribute sampling path.** The scatter pass needs height, normal
   and splat weights at arbitrary points. Reading the global vertex SSBO
   directly (via a base-vertex offset carried on the terrain-tile record) avoids
   baking anything and stays automatically in lockstep with the terrain — but the
   indirection cost per candidate point is unmeasured. Fallback is a baked
   per-cell attribute texture. **Measure before Phase 1.**
2. **Chunk size.** 512 units (8×8 per cell) is a starting guess balancing
   dispatch count against per-chunk culling granularity. Wants a sweep.
3. **Density-field calibration.** The affinity table and the noise frequencies
   need tuning against real cells. Pin with a density histogram captured over a
   fixed camera path per game — the telemetry-baseline shape `/audit-runtime`
   already uses — rather than a unit test, since the formula lives in GLSL (§3).
4. **Tier distances.** The §6 table is unvalidated. Skyrim tundra and the FNV
   Mojave have very different sight lines and will likely want per-worldspace
   scaling.
5. **Proxy shell opacity.** Whether mean chunk density is a good enough stand-in
   for the true blade distribution in the shadow term, or whether the shell needs
   a per-texel density map. Only answerable once Phase 4 renders.
