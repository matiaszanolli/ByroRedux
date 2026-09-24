# EXAL — Ground Cover (procedural grass)

A focused sub-document of [`exal.md`](exal.md), in the same relation to it as
the `charal-*-ruleset.md` files are to [`charal.md`](charal.md). EXAL owns the
outdoors environment; **ground cover** is the vegetation stratum that sits on
the terrain surface — grass, ferns, moss, low scrub.

**Status**: Phase 0 IMPLEMENTED (2026-08-12); Phase 5 palette resolution
IMPLEMENTED (2026-09-06, #3807); §11.1's terrain-attribute sampling question
MEASURED AND ANSWERED (2026-09-06, #4052); Phases 1–2 IMPLEMENTED (2026-09-06,
#4054 / #4055); Phase 6 IMPLEMENTED (2026-09-06, #4057) — which answered §11.6,
§11.7, §11.9 and §11.10 and closed Phase 5's `grass_dimmer` remainder; Phase 7
IMPLEMENTED (2026-09-06, #4058), which answered its own open question about
which entities feed the field. Phase 3 (#4056) tier 3 and the T0→T1
projected-pixel ribbon crossover with reduced-density widened Tier 1
IMPLEMENTED (2026-09-15); Tier 2 palette-atlas clump cards and their area fade
IMPLEMENTED (2026-09-15). Live visual acceptance evidence remains pending.
Step 7's live `GpuTerrainTile` size claims and blade-geometry literal naming
were reconciled and test-pinned (2026-09-15). Phase 4 is gated on a
demonstrated need (§5 Stage 2). Rolls out per §9.

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
  with `IsLodTerrain`
  ([`terrain_lod.rs:10-12`](../../byroredux/src/cell_loader/terrain_lod.rs#L10-L12)).
  Ground cover needs the same exclusion (§5).

  **Correction (2026-09-06):** this bullet used to say `IsLodTerrain` "keeps it
  out of the TLAS", and that has not been true for some time. A camera-local
  subset of LOD blocks *does* enter the TLAS as structure shadow casters, so
  `IsLodTerrain` is a distance-gated policy, not a boolean exclusion, and §5's
  proposed `ExcludedFromTlas` generalisation could not simply subsume it —
  collapsing the two would have silently changed LOD shadowing.

  **Resolved 2026-09-06 (#4053):** the three-term chain is now one policy
  function, `render::static_meshes::tlas_exclusion`, returning a named
  `TlasExclusion` reason rather than a composite boolean. Not a marker
  component — see §5.
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
d_ground = affinity(splat)            // what the ground is made of
         × slope_gate(normal)         // grass does not grow on cliffs
         × moisture(height_above_water)  // shoreline lush, ridgeline sparse
         × shelter(curvature)         // clumps settle into concavities
         × clump(noise)               // the organic term

d_draw   = d_ground × distance_fade(view)   // LOD only, §6
```

**These are two different quantities and conflating them is a bug** (split
2026-09-06; this was one `d` with `distance_fade` as a sixth factor).

`d_ground` is *intrinsic* — a property of the place, identical whether the
camera is standing on it or a kilometre away. `d_draw` is how much of that
ground cover we are choosing to rasterize this frame.

Only the scatter's accept/reject test may use `d_draw`. **Everything else must
read `d_ground`**: the value stored in the blade record (§4), the ambient
occlusion (§12.1), the ground-colour coupling (§12.3) and the canopy shadow
(§12.5). Feeding the faded value to those instead makes the shadow under a
meadow lighten as you walk away from it and the ground-tint fade with view
distance — a slow, whole-screen brightness change keyed to camera position,
which is both very visible and very hard to attribute once it ships.

Each factor and where it comes from:

- **`affinity(splat)`** — the 8 splat weights bilinearly sampled from the
  surrounding terrain vertices, dotted with a per-layer canonical
  `cover_affinity: f32`. This is the key reframing: a layer does not *enable*
  grass, it *weights* it. A dirt layer at 0.15 and a grass layer at 0.9 blend
  into a continuous gradient wherever the painter feathered them, and the
  vegetation boundary stops coinciding with the texture boundary.

  Worth being explicit about what this term is *not*: the splat authority is a
  17×17 alpha grid per 2048-unit quadrant
  ([`terrain.rs`](../../byroredux/src/cell_loader/terrain.rs), `per_quadrant_alpha`)
  — **precisely the resolution §1 names as cause #2**. Bilinear sampling makes
  it continuous rather than stepped, which fixes the *hard step*, but it cannot
  manufacture detail finer than ~128 units. That is why `clump(noise)` is
  load-bearing rather than decorative: it is the only term in the product with
  authority above that frequency. A build that stubs the noise to 1.0 "for
  now" will reproduce the vanilla patch look exactly, and will look like the
  design failed.
- **`slope_gate(normal)`** — smoothstep on the terrain normal's Y component.
  Ground cover thins out and then stops on steep faces. This alone removes the
  single most artificial thing about vanilla grass, which happily carpets
  cliff faces wherever the texture was painted.
- **`moisture(height_above_water)`** — signed distance to the WATAL water plane,
  falling off with altitude above it. Produces lush shorelines and sparse high
  ground for free, and reads as a reason for the distribution rather than a
  rule.

  **A cell with no water plane must resolve this term to 1.0, not to 0.0 or to
  an undefined distance.** In a pure product a single undefined factor takes
  the whole field with it, and "this worldspace has no water" is common — the
  failure would be an entire interior-adjacent or high-desert worldspace with
  no ground cover at all and nothing in the log to say why. The term expresses
  *extra* moisture near water; its absence is neutral, not hostile.
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
   points from a scrambled low-discrepancy sequence (chunk hash as the scramble
   seed, so placement is stable frame to frame and across sessions — a blade
   does not move when the camera does), evaluates `d_draw` at each, and
   stochastically accepts. Accepted points atomically append to a per-chunk
   slice of the blade buffer and bump the chunk's
   `VkDrawIndexedIndirectCommand` instance count.

   **The per-chunk slice is fixed capacity, so it can overflow, and the
   overflow policy is part of the design rather than an implementation
   detail.** A chunk on rich flat ground at full density will hit the cap;
   the atomic append must saturate rather than wrap, and the frame must not
   depend on which threads happened to win the race.

   That constrains the sequence. This said "blue-noise tile" until 2026-09-06,
   and a tile consumed in order is **not progressive**: truncating it leaves
   whatever the first N entries happen to be, which is not a well-distributed
   set and shows up as directional clumping in exactly the densest chunks. A
   progressive low-discrepancy sequence — one whose every prefix is
   well-distributed — degrades into a uniformly sparser chunk instead, which is
   the same failure mode the distance fade already produces and therefore reads
   as nothing at all.
3. **Blade record.** ~16 bytes: packed chunk-relative position, a seed word, a
   species index, and the evaluated `d_ground` (reused downstream for width
   compensation, §12.1 occlusion, §12.3 coupling and §12.5 shadowing — note
   `d_ground`, not `d_draw`; see §3). Not a `GpuInstance` — a separate, much
   smaller SSBO that no other pass reads.

   **What is deliberately absent, and why it stays absent.** The record
   carries no terrain normal and no terrain albedo, yet the blade vertex shader
   needs the normal to orient the blade to the ground, and §12.3 needs the
   albedo to tint toward it. Both are therefore re-sampled in the *raster*
   pass, which samples far more often than the scatter does — once per vertex
   per blade rather than once per candidate point.

   Storing them instead is the obvious alternative and is not free: it roughly
   doubles the record, and the blade buffer is the one structure in this design
   whose size scales with the visible blade population.

   **Settled 2026-09-06 by the §11.1 bench (#4052): re-sample.** The raster
   re-sample costs 0.0018 ns per vertex sample net of the vertex stage's own
   invocation and primitive-assembly cost — 0.014 ns per 8-vertex blade,
   ~0.014 ms per million visible blades — because every vertex of a blade
   reads the same address and the gather is a cache hit after the first.
   Storing the two terms instead would buy that back at +16 MB per million
   blades, written by the scatter and read by the raster every frame. Numbers
   and method in §11.1.
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

Decided: **receive-only, and the shadow it owes back is analytic rather
than traced** (revised 2026-09-06 — see Stage 2).

The two stages below are internal to this section. They are *not* §9's
phase numbers, which now run 0–7.

### Stage 1 — receive-only

Grass rasterizes in the main geometry pass and traces the existing shadow ray
and GI sample like any other fragment, so it is correctly lit, shadowed by the
world, and colour-bled into by nearby surfaces. It contributes nothing back: no
BLAS, no TLAS entry, no ray-budget cost.

Grass therefore needs no TLAS entry at all.

**This section used to ask for an `ExcludedFromTlas` marker component here, and
that was wrong twice over (revised 2026-09-06, #4053).**

Wrong on the mechanism: `IsLodTerrain` does not "do exactly this job". Its
exclusion is *distance-gated* — the same LOD block is in the TLAS when the
camera is near it and out when it is far (§2) — so it cannot be expressed as a
marker without inserting and removing that marker every frame, which turns a
read-only render pass into a structural mutation of the world.

Wrong on the need: §2's binding constraint makes blades a GPU-side point list,
never ECS entities, so nothing in the ground-cover path flows through the
static-mesh loop that reads such a marker. The first entity that would is the
Stage 2 proxy shell, which is now gated on a demonstrated need.

The underlying complaint was real, and was fixed on its own terms in #4053:
`render::static_meshes::tlas_exclusion` is now the single TLAS-membership
policy, with each exclusion named (`DistantLodBlock`, `DecalSurface`,
`FireRefraction`) and its gating preserved exactly. A fourth reason adds an
enum variant, not a fourth `&&`. Membership was verified unchanged across four
scenes by the `bench:` line's `state_hash`, which hashes per-draw `in_tlas`
alongside `entity_id`; the accompanying `tlas-policy:` row reports the
verdict counts by reason, so a scene proves it exercised an arm rather than
being assumed to.

### Stage 2 — the shadow grass owes back

Receive-only grass casts no shadow, and grass that casts no shadow reads as
pasted onto the terrain — which is the exact failure this whole document exists
to avoid. That premise stands. **The conclusion drawn from it here did not.**

This section used to answer it with a ray-traced proxy shell: per chunk, a
low-poly sheet following the terrain surface displaced upward by the local mean
blade height, entered into the TLAS and treated in the hit path as a
stochastically transparent medium. It buys correctly-shaped soft shadows and a
plausible GI contribution, and it costs acceleration-structure memory, a
per-chunk build and refit, TLAS instances and a hit-path branch — across a
stratum that is ankle-high and covers the entire visible world.

**A ray is not needed for the shadow that matters.** Almost all of the visual
work a grass shadow does is contact darkening: ground under and beside a clump
is darker, and darker still as the sward thickens. That is a function of the
density field and the light direction, and the shader already has both. §12.5
computes it in closed form for a handful of instructions, no BLAS, no TLAS
entry, no ray budget.

So the shell is **demoted from the answer to an optional upgrade**, and Phase 4
is gated on something demonstrating it is needed rather than assumed. What it
would still buy, and the analytic term cannot:

- grass shadowing something that is neither terrain nor another blade — a
  dropped item, a prone actor, a wheel rut;
- grass appearing in RT reflections and GI *as geometry* rather than as the
  terrain's colour (§12.6 argues that substitution is close enough at this
  scale, which is most of why the shell is no longer load-bearing).

The description above is kept rather than deleted so the shell can be picked up
as specified if either case turns out to matter.

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
| 1 | 2 000 – 6 000 | 1-segment ribbons; density thinning and compensating width follow the geometric crossover |
| 2 | 6 000 – 15 000 | clump cards — a few crossed quads per clump, from a baked atlas |
| 3 | 15 000+ | terrain detail layer only |

Two rules make this continuous rather than three visible thresholds:

**Density fades stochastically, and survivors widen.** Blades do not vanish at a
boundary; the acceptance threshold in the scatter rises smoothly with distance
while accepted blades grow slightly wider, holding total coverage roughly
constant. The population thins without the silhouette thinning.

**Drive the widening from projected pixel size, not from distance**
(2026-09-06). The two are not interchangeable, and the difference is a bug
that only appears on some machines. A blade narrower than a pixel does not
antialias — it flickers as the sub-pixel coverage changes from frame to frame,
and thin high-contrast geometry is the canonical case TAA handles worst, so
the existing resolve will not save it. Widening to hold a floor of roughly one
pixel makes the fade an antialiasing measure as well as a coverage one. Keyed
to distance instead, the floor is only correct at whatever resolution it was
tuned at: the same scene shimmers at 4K and is stable at 1080p, which reads as
a hardware problem rather than a shader one and is correspondingly hard to
track down.

**The terrain detail layer is always on.** The terrain fragment shader modulates
its albedo and normal with the active palette's generated per-species
ground-cover detail atlas, whose strength is *the same density field*, at every
distance including zero. Grass geometry is drawn
**on top of** a surface that already shows the right colour, the right
variation, and the right density. So when the last geometry tier fades out,
what remains underneath is already a match — there is nothing to pop *to*.

This is the single most important item in the document for the stated goal.
Tier 3 is not "where grass stops"; it is where grass stops being geometry.

### Current geometric LOD implementation

The first geometric crossover is now driven by projected blade height rather
than the table's indicative world-distance ranges. Its band is derived from
the existing `min_visible_half_width` screen-space floor and
`max_width_multiplier` cap—no new distance or pixel tuning knob was added.
Both representations draw from separate indirect streams over the **same fixed
blade slab** inside that band. Complementary 8×8 blue-noise coverage,
rotated by the blade seed and replayable frame serial, makes the band a
stochastic cross-fade rather than a pop. The transition writes a bounded FSR
reactive value only inside the band; its endpoints stay ordinary opaque,
depth-writing geometry with their normal velocity vectors.

Tier 1 retains one deterministic representative from each four-blade near
tuft and widens it by that same existing tuft count, bounded by the existing
screen-space width cap. It therefore reduces its vertex population by 12×
relative to Tier 0 (three segments × four blades) without adding a tuning
constant or moving a slab.

Tier 2 groups a four-by-four set of those roots into one six-vertex vertical
clump card. It samples the same palette-generated per-species atlas that Tier
3 uses for terrain detail, so the card colour and cutout cannot drift from the
underlying field. Tier 1's blue-noise coverage falls as `mid * (1 - card)`;
the card grows its width and height by `sqrt(card)` and keeps its alpha
silhouette stable. This preserves summed projected area across both handoffs,
while the reactive mask covers either crossover. The implementation is in
place; fixed-camera visual acceptance is still required before calling the
chain complete.

Scatter compacts accepted candidates in ascending candidate index within each
workgroup round before reserving a blade-slab range. This replaces the former
per-candidate atomic append: an atomic race could reorder roots between runs,
which is harmless for independent ribbons but not for a card that groups every
four-by-four roots. The retained order makes both the card representative and
its blue-noise seed reproducible across workgroup scheduling.

New residency-ring slots begin with zero blade height and advance to full
height using the existing cited interaction-field temporal response. The
progress value is carried in the chunk record, so eviction resets it while a
resident slot continues growing without moving its blade slab.

### Residency of the geometric tiers (#4338)

The fixed blade arena is divided into one equal slab per camera-centred
residency-ring slot. Ring capacity is derived from
`GROUNDCOVER_DRAW_DISTANCE / GROUNDCOVER_CHUNK_UNITS` (with the chunk bounding
radius included); it is not a second, hand-tuned visible-chunk cap. A chunk
keeps its slot—and consequently its blade slab—until it leaves the ring.

On a teleport or large turn, newly desired chunks enter in nearest-first order
through a **24 chunks/frame** placement budget (the initial value specified by
the #4338 follow-up plan). The field therefore fills over several frames rather
than hitching or silently deleting the far or west-side chunks. A vacant slot is
still uploaded as an explicit inactive chunk record and scatter writes it a
zero-instance indirect command; compacting the remaining records would move
their blade slabs and break residency. `GroundCoverStats::chunks_truncated`
now reports only an actual cell-table capacity fault; ordinary pending ring
work is not lost.

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
is a per-weather multiplier and therefore explicitly **not** a palette field —
see §8's "the per-weather path already exists". #4057 landed it as
`GroundCoverDimmer`, a small resource riding the slot `weather_system` already
writes `WindField` into.

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

The wind pose has a fundamental plus a one-third-amplitude `2.17×` harmonic,
and a one-third lateral component. Those values are the explicit starting
ratios in the credibility track's Step 6 checklist, rather than uncited blade
shape tuning. Both modes are evaluated from the blade base seed for the
current and previous shared clock samples, so they improve the visible motion
without breaking the velocity contract.

Wind deflection is quadratic in a blade's sampled height, normalised by that
species' authored maximum height. The tallest blade consequently retains the
established response while shorter siblings flex less, rather than every
height moving by the same linear proportion.

The control-point construction preserves the root-to-tip chord after **all**
horizontal displacement has been combined (wind plus the §12.4 interaction
field): its vertical reach is `height * sqrt(1 - (bend / height)^2)`. A gust
therefore changes pose instead of making a blade visibly grow, and evaluating
the same construction at the prior shared wind clock preserves that contract
for motion vectors as well.

**The per-weather path already exists, and only wind uses it.**
`weather_system` writes `WindField` on every weather change
([`systems/weather.rs:988`](../../byroredux/src/systems/weather.rs#L988)), so
wind tracks a storm rolling in without ground cover doing anything. Nothing
does the equivalent for the palette: `GroundCoverPalette` is installed once at
worldspace entry and never touched again.

That asymmetry is where Oblivion's `grass_dimmer` belongs, and it resolves the
question Phase 5 left open. Folding the dimmer into the palette's colour
gradient at resolve time would freeze it at whichever weather happened to be
active on entry; riding the slot `weather_system` already writes each frame
makes it per-weather for free, on a path that is proven and has a live
consumer. The colour multiplier is a small per-weather resource, not a palette
field.

Wind is deliberately not simulated and not collided here; the blades-react-to-
things half is §12.4 / Phase 7.

---

## 9. Rollout order

Each phase is independently useful and independently reviewable.

- **Phase 0 — canonical types + boundary.** `GroundCoverSpecies`, palette,
  `WindField`, the affinity table, the EXAL translate site, and the `LTEX`
  keyword map. Pure CPU, fully unit-testable, no rendering.
  **Done (2026-08-12, #2369)** — types in
  [`components/groundcover.rs`](../../crates/core/src/ecs/components/groundcover.rs),
  boundary in
  [`groundcover_translate.rs`](../../byroredux/src/groundcover_translate.rs).

  The keyword table was derived from the real `LTEX` corpus of the four
  installed games (386 unique records: Oblivion 229, FNV 89, Skyrim 68, FO3 51)
  by tokenising every editor ID and ranking by frequency, rather than invented.
  It resolves 98.7% of the corpus; the 5 residual names are genuinely ambiguous
  (Bravil city base terrain, an Oblivion decal symbol) and correctly take the
  low-but-nonzero default.

  That sweep surfaced three rules that source-reading alone would have missed,
  each now pinned by a regression test:

  1. **`NoGrass` suppression.** 46 corpus records carry an explicit `NoGrass`
     suffix — `CHTerrainGrass01NoGrass`, `LTundra01NoGrass`,
     `DementiaMoss01NoGrass`. They are authored variants with vegetation
     deliberately removed (worn paths, ground under buildings). A
     `contains("grass")` test scores them *highest* when they mean the exact
     opposite, so suppression is checked first and wins outright.
  2. **Worn surfaces outrank their substrate.** `LDirtPathWasteland01` is a
     trail through dirt; matching `dirt` first grows grass across the trail.
  3. **`grass` outranks a barren base.** `RootsBarrenWastesGrass01` and
     `ChemicalBarrenWastes01Grass` are barren ground with grass painted over,
     so the vegetated reading is the correct one.

  Affinity *values* remain initial estimates pending the §11.3 density-histogram
  calibration; the tests pin ordering and structure, never the scalars, so that
  calibration can move numbers without rewriting the suite.
- **Phase 1 — scatter.** Chunking, the density field in GLSL,
  `groundcover_scatter.comp`, and debug point rendering of accepted candidates
  over real terrain. This is where the distribution is judged — before any
  blade exists.

  The `ExcludedFromTlas` generalisation that used to head this list is gone:
  ground cover never carries such a marker (§5), and the TLAS-membership
  cleanup it was standing in for landed separately as #4053.

  **Ungated 2026-09-06 (#4052).** §11.1's sampling question is answered, and
  the pieces the answer needed are already in place for the scatter to use:
  `TerrainCellOrigin` (the chunk-to-instance association),
  `include/terrain_sample.glsl`'s `byroSampleTerrain`, and the
  `GROUNDCOVER_CHUNK_UNITS` / `GROUNDCOVER_CHUNKS_PER_CELL_SIDE` constants.
  The scatter reads the global vertex SSBO directly; there is no attribute
  texture to bake.
- **Phase 2 — blades + wind.** Vertex-shader Bezier ribbons, the wind field,
  the near tier only.
- **Phase 3 — LOD chain.** The always-on tier-3 terrain detail layer and the
  projected-pixel T0→T1 ribbon cross-fade are implemented; Tier 1 renders one
  widened representative per four-blade near tuft; Tier 2 groups those roots
  into palette-atlas clump cards with area-preserving fade. Tier 3 landed first
  so every later tier is authored against a correct backdrop. §12.3's
  ground-colour coupling is implemented (2026-09-24): the blade fragment
  blends the species gradient toward the terrain albedo sampled at the blade
  base, by the per-species weight, fading to zero at the tip. Fixed-camera
  visual acceptance remains pending.
- **Phase 4 — RT proxy shell.** Per-chunk shell, stochastic-absorption hit
  handling, refit on density change. **Gated, not scheduled** (revised
  2026-09-06): §12.5 supplies the shadow this phase existed to provide, at a
  fraction of the cost, so Phase 4 waits on a demonstrated need — see §5
  Stage 2 for the two cases that would constitute one.
- **Phase 5 — per-game palette.** `GRAS` → species, `grass_dimmer`, and the
  per-worldspace palette resolution.

  **Superseded 2026-09-13 (§12.12):** `GRAS` records no longer become blade
  species. The census behind that decision found every vanilla model is a card
  clump or an opaque mesh, about half the records are not grass, and grass
  cannot be told from ferns or shrubs by model content. The decode stays; the
  records move to the authored-model tier. The paragraphs below describe what
  was built in #3807.

  **Palette resolution done (2026-09-06, #3807)** — `GRAS` decodes in full
  ([`records/gras.rs`](../../crates/plugin/src/esm/records/gras.rs), replacing
  the `MinimalEsmRecord` stub), translates to species at the EXAL boundary
  (`species_from_gras` / `authored_species` in
  [`groundcover_translate.rs`](../../byroredux/src/groundcover_translate.rs)),
  and `install_ground_cover` resolves the worldspace palette from the load
  order's records. 138 of the 168 vanilla records across Oblivion / FO3 / FNV
  / Skyrim SE carry usable dimensions and become real species; the rest fall
  through to the built-in default, as does content with no `GRAS` at all.

  **`grass_dimmer` done (2026-09-06, #4057)** as `GroundCoverDimmer`, a
  per-weather resource `weather_system` cross-fades alongside `precipitation`
  and `WindField`, seeded by `install_ground_cover` at worldspace entry
  (systems only get `&World`, so entry is the one moment with `&mut World`).
  `collect_groundcover_species` multiplies it into every authored colour —
  reflected and transmitted alike, since it describes the light the weather is
  passing, not the reflectance — and leaves the `.w` lanes (bend stiffness,
  ground coupling, sheen amount) untouched. `1.0` for every game that ships no
  `HNAM`, which is all of them except Oblivion.

  One piece of this phase remains **not** done, for want of a consumer rather
  than for want of data:

  1. **Colour gradient from the model texture.** §7 sources it from the
     `GRAS` model's texture, which needs the archive-backed asset provider,
     not the record — `colour_range` is a per-instance jitter *amount*, not a
     colour, so there is nothing in the record to approximate it from.
     Species keep the climate default's gradient until then.

     **Deliberately still deferred after #4057**, and §12.3 is why: if colour
     turns out to be substantially coupled to the ground the species gradient
     is a smaller input than §7 assumed, and the size of this job should be
     decided after that coupling weight is calibrated (§11.8, Phase 3 / #4056)
     rather than before. Building the texture-sampling path first would be
     sequencing by implementation convenience — the same mistake §12.4's entry
     in §10 records.

  The corpus census behind the decode is recorded in
  [`records/gras.rs`](../../crates/plugin/src/esm/records/gras.rs)'s module
  doc. The one finding that changes this document: **`wave_period` is not a
  usable stiffness signal.** Its scale is not comparable across games
  (Oblivion 0.0001–30, Skyrim 50–600 for the same authored intent) and it
  does not correlate with plant height in any corpus (Spearman +0.11 on
  Oblivion's n=99, −0.05 on Skyrim's n=21), so `bend_stiffness` stays a
  palette-level constant rather than a per-species translation.
- **Phase 6 — the organic terms.** Contact occlusion, translucency, canopy
  shadowing and sheen (§12.1, §12.2, §12.5, §12.6). Every one of them is
  analytic and rayless. Needs blades to exist (Phase 2) but is independent of
  both the LOD chain and the RT shell, so it can land beside either. **Do not
  defer this behind Phases 3–4**: those make ground cover cheaper and push it
  further away, while this is what makes it read as a living surface at all,
  which is the stated goal.

  **Done (2026-09-06, #4057)**, together with Phase 5's `grass_dimmer`
  remainder. §12.3's ground-colour coupling moved to Phase 3 (#4056), where it
  is the same idea as §6's tier-3 detail layer from the opposite end.
- **Phase 7 — interaction.** The displacement field (§12.4).
  **Done (2026-09-06, #4058).**

---

## 10. What stays out of scope

- **Trees.** SpeedTree `.spt` content and the distant tree LOD ring are a
  separate concern with a separate authority (`exal.md` §5); ground cover stops
  at low scrub.
- **Harvestable flora.** `FLOR` records are gameplay entities with inventories
  and activation, not ground cover. They stay ordinary placed references.
- ~~**Grass interaction.**~~ **Moved into scope (2026-09-06)** as §12.4 /
  Phase 7. The claim it carried here — "worth doing only once Phase 4 lands"
  — was wrong twice over: it shares no data with the RT proxy shell and
  needs nothing from it, and it contributes more to the organic read than
  the shell does. Deferring it behind Phase 4 sequenced it by
  implementation convenience rather than by what the feature is for.
- **Seasonal / snow variation.** The palette has the room for it
  (`climate_weight`), but driving it needs a canonical season concept EXAL does
  not currently have.
- **Authored placement.** ~~No mechanism for hand-placing or hand-suppressing
  grass in a region.~~ **Revised 2026-09-06 — the mechanism exists in the
  source data and this repo already parses the record type.** `REGN` carries a
  `RegionDataKind::Grass` (`RDGS`) entry alongside `Objects` (`RDOT`)
  ([`records/misc/world.rs:664`](../../crates/plugin/src/esm/records/misc/world.rs#L664)),
  which is precisely per-region authored grass placement — level designers
  used it to put grass where the texture-keyed scheme would not, and to keep
  it out of places it would.

  The payload is not decoded and no consumer exists, so nothing is being
  unwound; but "no mechanism" was wrong, and it matters because the shape of
  the eventual fix was already stated correctly here: an extra multiplicative
  term in §3, never an escape hatch around the field. A region that suppresses
  ground cover multiplies toward zero over its polygon with a soft edge; it
  does not switch the field off.

  Coordinates with issue 3301, which owns `REGN`'s `RDAT` decode. This
  document should not grow a second `REGN` reader.

---

## 11. Open questions requiring real-data verification

Not answerable from source reading; each needs a `--bench-hold` session against
real worldspaces before the phase that depends on it.

### 11.5 Credibility reference frames (2026-09-15)

[`scripts/renderer-eval-groundcover.sh`](../../scripts/renderer-eval-groundcover.sh)
defines the four review anchors for this track. It freezes the canonical game
clock with `BYRO_HOUR=7`, runs a fixed-dt static-camera bench, and writes a
PNG, log, and `manifest.tsv` row for each named case:

| Case | Worldspace / grid | View |
|---|---|---|
| `gc-backlit-fnv` | FNV `WastelandNV` / `0,0` (Goodsprings outskirts) | camera faces the low sun |
| `gc-frontlit-fnv` | FNV `WastelandNV` / `0,0` | same root, reverse heading |
| `gc-backlit-skyrim` | Skyrim `Tamriel` / `2,-4` (Whiterun tundra) | camera faces the low sun |
| `gc-frontlit-skyrim` | Skyrim `Tamriel` / `2,-4` | same root, reverse heading |

The script's pose variables are deliberate, reviewable defaults; an operator
may supply `BYROREDUX_GC_*_CAMERA_*` only to establish a replacement pose, then
must commit that exact value before treating its output as a reference. The
manifest preserves the PNG hash plus the `bench:` and `groundcover:` rows.
Use `BYROREDUX_RENDER_EVAL_RUNNER` for a local display/Xvfb wrapper. Store an
accepted baseline or after image under `docs/audits/` with the tested commit
hash in its filename. The clean-HEAD four-case baseline is recorded in
[`gc-reference-00d4ef5d0.md`](../audits/gc-reference-00d4ef5d0.md); the
native Wayland capture runner is the supported local path.

`BYROREDUX_GROUNDCOVER_EVAL_CASES` accepts a comma-separated subset of those
four case names. Its only purpose is operational: it appends each selected
case to an existing manifest so a bounded local runner can collect the exact
same four fixed captures independently. With the variable unset, the harness
still starts a fresh full-suite manifest.

For the temporal acceptance path, set
`BYROREDUX_GROUNDCOVER_EVAL_BENCH_CAMERA=pan`. The harness then selects
`renderer-stepped`, which fixes simulation time at 1/60 s and drives the
engine's deterministic pan instead of silently combining a moving camera with
the static benchmark mode. The native-Wayland FNV backlit run at revision
`fd0cd577c` completed 180 requested frames (182 rendered), with 8,467 blades,
66 chunks and `truncated=0`; its final frame is retained as
[`gc-backlit-fnv-pan-fd0cd577c.png`](../audits/gc-backlit-fnv-pan-fd0cd577c.png).
It shows a clean moving-camera frame rather than the green reprojection trail
that the pre-motion-vector contract produced. This is a single end-of-pan
artifact, not a substitute for the Step 6 wind clip.

For projected-pixel LOD review, set
`BYROREDUX_GROUNDCOVER_EVAL_WINDOW_SIZE=1920x1080` or `3840x2160`. The harness
passes that exact native size as `--window-size`; it does not depend on Wayland
compositor scaling.

#### Wind review clip (2026-09-15)

[`gc-wind-skyrim-pan-3s-c7c9900e6.mp4`](../audits/gc-wind-skyrim-pan-3s-c7c9900e6.mp4)
is the retained three-second Skyrim backlit wind review. It contains six
fresh-process captures at 2 fps, sampled at fixed `1/60 s` simulation times
from 0 to 3 s with the deterministic `pan` path. Restarting for each sample
keeps the final pan pose constant while the shared wind clock advances, so the
clip isolates blade motion from a changing viewpoint. Its contact sheet shows
the field changing coherently across nearby roots rather than each ribbon
shimmering independently. The clip is evidence for the Step 6 checklist, not
a claim that the separate ten-second no-loop inspection has been completed.

#### Washout precheck (2026-09-19)

The harness's own baseline can be the failure it exists to catch: the
committed `gc-backlit-*` references at `00d4ef5d0` measure luminance sd
0.0092–0.0106 (≈2.3–2.7/255) — the ground-framing poses were themselves the
washout veil, while the frontlit frames measured 0.077–0.090. The protocol,
now encoded in the script instead of only in audit prose (#4482, #4509):

1. Measure luminance standard deviation first (`magick …
   %[fx:standard_deviation]`). Below ~10/255 (sd 0.039) a frame carries no
   scene contrast and cannot arbitrate any ground-cover A/B.
2. Treat only `gc-backlit-*` poses as ground-framing. A backlit frame under
   the line is stamped `washed_out=yes` in `manifest.tsv` (with the measured
   sd) and fails the run; a frontlit frame under it is stamped and warned.
3. Confirm the veil is gone — backlit sd above the line — before trusting a
   wind, palette, or cross-fade delta. An A/B over washed frames reports
   "no delta" for a real fix.
4. The `bench:` and `groundcover:` telemetry rows must be present, and a
   backlit case must not report the annihilated `chunks=0 blades=0`
   signature; otherwise a pass that scattered nothing mints a
   perfectly-formed reference row.

1. **Terrain attribute sampling path — ANSWERED 2026-09-06 (#4052).** Both
   candidates were built and measured on real terrain. **Read the global vertex
   SSBO directly (path A). Do not bake an attribute texture.** And, for §4:
   **re-sample in the vertex shader; do not store the normal and albedo on the
   blade record.**

   ### What was measured

   `--bench-groundcover-sampling` (`byroredux_renderer::vulkan::groundcover_bench`)
   crosses both paths with both consumers, plus a **control** per consumer that
   generates the identical candidate points and emits the identical primitives
   while sampling nothing. The control is what makes the rest readable: the
   raster half's per-vertex invocation and primitive assembly land inside the
   same GPU-timer bracket as its sampling, and without a no-sample run "the two
   paths are indistinguishable" cannot be told apart from "the sampling is
   buried under an overhead that dominates both". One variant per frame, one
   bracket, round-robin.

   RTX 4070 Ti, `--radius 3` (49 resident cells → 3136 chunks of 512 units),
   1024 samples per chunk, 3.21 M samples per frame. `net` is with the
   consumer's own control subtracted.

   | Variant | Skyrim tundra (2,-4) ns/sample | net | FNV Mojave (0,0) ns/sample | net |
   |---|---|---|---|---|
   | `scatter/ssbo`  | 0.0258 | **0.0244** | 0.0244 | **0.0231** |
   | `scatter/baked` | 0.0131 | **0.0118** | 0.0128 | **0.0115** |
   | `scatter/floor` | 0.0013 | — | 0.0013 | — |
   | `raster/ssbo`   | 0.0332 | **0.0018** | 0.0321 | **0.0013** |
   | `raster/baked`  | 0.0324 | **0.0010** | 0.0317 | **0.0009** |
   | `raster/floor`  | 0.0314 | — | 0.0309 | — |

   Bake: 53,361 texels (49 cells), 0.02–0.23 ms per rebake.

   The two worldspaces agree to within 6% on every row, which is the answer to
   §11.4's worry that their different sight lines would matter here: they change
   how many chunks are visible, not what a sample costs.

   A 16× working-set sweep (`--bench-groundcover-sampling 4,32` and `64,512`,
   80 M to 1.28 G samples per frame) moves the absolute numbers but not the
   conclusion: scatter net stays 0.023–0.033 (A) against 0.0115–0.0132 (B), and
   raster net stays under 0.004 for both.

   ### What the numbers say

   **The raster consumer — the larger one — cannot tell the two paths apart.**
   95% of its cost is the control: shading a vertex and assembling a primitive
   for every blade vertex. The sampling on top is 0.0018 ns (A) against 0.0010
   ns (B), so path A's entire disadvantage in the consumer that samples most is
   **0.0008 ns/sample** — 0.003 ms per frame at 3.21 M samples. Per-blade
   coherence is why: `GROUNDCOVER_BENCH_BLADE_VERTS` consecutive invocations
   read the same address, and path A's gather is a cache hit after the first.

   **The scatter consumer can, and path B wins there by ~2×** (0.0244 vs 0.0118
   net). That is a real difference and it is what you would predict from bytes
   touched: path A reads 6 of a vertex's 26 floats but pulls all 104 bytes of
   the line, four times per bilinear sample — ~416 B against path B's ~96 B
   through one hardware-filtered fetch plus two splat fetches.

   **But the scatter's whole sampling budget is 0.08 ms/frame.** Path B saves
   0.04 ms of it. On a 19 ms frame that is 0.2%.

   ### Why path A anyway

   Path B's measured win is 0.04 ms/frame on the *smaller* consumer and nothing
   on the larger one. Against that it costs:

   * **A staleness class of bug.** The bake is a snapshot of a buffer
     `MeshRegistry` compacts. The harness keys its rebake on a hash of the
     resident `(origin, vertex_offset)` set for exactly this reason; production
     would have to carry the same machinery, and get it right, forever. Path A
     has no such state — it reads what the terrain currently is.
   * **Memory that scales with the resident ring** — 1.28 MB per 49 cells here
     (33×33 × RGBA32F + 2×RGBA8 per cell), against a 4 GB total budget.
   * **A bake step** on every ring change. Cheap (0.02–0.23 ms) and not the
     reason to reject it, but not free either.

   Trading a correctness hazard and a memory line item for 0.2% of a frame is
   the wrong side of that trade.

   **Caveat worth stating: this is a hot-cache measurement.** The 4070 Ti has
   48 MB of L2, and the resident terrain vertices at `--radius 3` are 5.5 MB
   (49 × 1089 × 104 B) — path A's gather is L2-resident throughout. On a GPU
   with a much smaller L2, or at a ground-cover radius large enough to spill it,
   path A's scatter number would degrade first. The raster conclusion is more
   robust, since it rests on per-blade coherence rather than on the whole ring
   fitting in cache. Re-measure before shipping ground cover on a low-L2 target.

   ### §4's store-vs-resample, settled

   Re-sampling the terrain normal and albedo in the blade vertex shader costs
   **0.0018 ns per vertex sample, net** — 0.014 ns per 8-vertex blade, ~0.014 ms
   per million visible blades. Storing them instead roughly doubles the 16-byte
   blade record, and the blade buffer is the one structure in this design whose
   size scales with the visible population: +16 MB at a million blades, written
   by the scatter and read by the raster every frame. §4's record stays as
   designed — no terrain normal, no terrain albedo.

   ### Plumbing this settled along the way

   §11.1 originally described path A as reading "via a base-vertex offset
   carried on the terrain-tile record". No such field exists.
   `GpuTerrainTile` was then 24 texture indices and nothing else — 96 bytes of
   `uint[8] × 3`, pinned by *gpu_terrain_tile_is_96_bytes* and by
   `ArrayStride 96` in the shipped `triangle.frag.spv`. (#4057 has since grown
   it to 144 B with §12.5's terrain-receiver fields, then #4056 added the
   Tier-3 detail-atlas selector and brought it to its current 160 B — pinned
   by `gpu_terrain_tile_is_160_bytes` — and it still carries no vertex offset.)

   The locator is one record over: `GpuInstance.vertex_offset`
   ([`gpu_types.rs:109`](../../crates/renderer/src/vulkan/scene_buffer/gpu_types.rs#L109)).
   So path A is chunk → covering terrain instance → `GpuInstance.vertex_offset`
   → global vertex SSBO, and the missing piece was the **chunk-to-instance
   association**, not a field on the tile record. That association now exists:
   `TerrainCellOrigin` (`byroredux::components`) records each LAND tile's Y-up
   cell origin at spawn, and the resolved `(origin, vertex_offset)` pair is
   rebuilt every frame — never cached, because the registry compacts. Both
   paths go through it, so neither was measured with a locator the other had
   to look up.

   Also worth recording, because it set the size of the job: splat weights
   reach the fragment shader as interpolated vertex attributes
   ([`triangle.vert:21`](../../crates/renderer/shaders/triangle.vert#L21)), not
   as a lookup. Terrain attributes had never been sampled at an arbitrary point
   anywhere in this renderer, so neither candidate had existing code to extend
   and both had to be built before either could be timed.
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
   a per-texel density map. Only answerable once Phase 4 renders — and Phase 4 is
   now gated behind a demonstrated need (§5 Stage 2), so this question may never
   have to be answered at all.
6. **Occlusion strength vs. density — ANSWERED 2026-09-06 (#4057). It is not
   a free parameter.** The question assumed §12.1 needed a strength scalar and
   a height falloff of its own. It does not: §12.1 and §12.5 are the *same*
   extinction through the *same* slab, differing only in the direction light
   arrives from. §12.5 integrates along the sun (`1/cos θ`); §12.1 integrates
   over the hemisphere, and the standard two-stream approximation replaces that
   angular integral with a single diffusivity factor of 5/3 (Elsasser).

   So `byroGcSkyOcclusion` is `byroGcCanopyTransmittance` with `1/cos θ`
   swapped for `GROUNDCOVER_SKY_DIFFUSIVITY`, over the same
   `byroGcCanopyOpticalDepth`, and the falloff over blade height falls out of
   the slab geometry — a fragment at parametric height `t` has `height·(1−t)`
   units of canopy above it. Both properties are pinned by
   `occlusion_and_canopy_shadow_share_one_extinction`.

   A separate "occlusion strength" would have been a way for the two terms to
   disagree about how thick the same grass is, which is a bug with no symptom
   until someone tunes one of them.
7. **Transmission colour granularity — ANSWERED 2026-09-06 (#4057). One colour
   per species, thickness-modulated.** The along-height variation the question
   was about is real, and it comes out of the geometry rather than the palette:
   `byroGcBladeTransmittance` is Beer–Lambert through the blade's *tapered*
   width, so a tip that has narrowed to nothing transmits freely while the base
   at full width transmits ~8% — the measured band for a single green leaf in
   the visible, where chlorophyll absorbs blue and red hard. A per-height
   authored gradient would be a second, hand-tuned copy of a curve the taper
   already draws.

   The one deliberate exclusion: the vertex shader feeds this the taper
   *before* §6's projected-pixel width floor. That floor widens a far ribbon so
   it can be antialiased; letting it make the blade optically thicker would
   darken the distant field for a reason that has nothing to do with the plant.
8. **Ground-coupling weight.** How far the species gradient should be pulled
   toward terrain albedo (§12.3). At zero, vegetation and ground stay two
   surfaces; at one, every species is the colour of its dirt and the palette
   stops meaning anything. Calibrate on the §11.3 telemetry run, which already
   samples both terms.
9. **Canopy extinction coefficient — ANSWERED 2026-09-06 (#4057). It is two
   numbers, and both have units.** `k` was opaque because it bundled a
   dimensionless optical property with a density-to-leaf-area conversion.
   Split:

   * `GROUNDCOVER_CANOPY_EXTINCTION_K = 0.5` — the Beer–Lambert extinction
     coefficient for a canopy of randomly oriented leaves (spherical leaf-angle
     distribution; Monsi & Saeki 1953, still the textbook value in Campbell &
     Norman). Grass is erectophile, so its true `K` runs a little under this
     with the sun high and a little over with it low; the spherical value is
     the neutral middle of that swing, and §12.5's `1/cos θ` already carries
     the dominant part of the angular dependence.
   * `GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY = 0.4` per unit — leaf area per unit
     of canopy depth at `d_ground == 1`. A dense temperate sward has a leaf
     area index of 3–5; LAI = 4 over the ~10-unit canopy the default temperate
     species stands up gives 0.4.

   Their product is the old `k = 0.2`, so a full-density sward transmits
   `exp(−2) ≈ 0.14` of vertical sun to the ground — the measured order for
   photosynthetically active radiation reaching a dense pasture floor. The
   question's "one number between grass tints the ground and grass paints a
   black hole" is now two numbers that can each be checked against a
   measurement instead of against taste.

   `GROUNDCOVER_CANOPY_MIN_COS = 0.1` caps the path length at 10× vertical (a
   sun ~5.7° up). That one is a numerical guard and says so: an uncapped
   `1/cos θ` drives the transmittance to zero and paints the terrain black
   through the last minutes of every sunset.
10. **Sheen granularity — ANSWERED 2026-09-06 (#4057) for the dry case: one
   scalar per species.** The lobe splits into a part that is a property of
   *fibres* and a part that is a property of the *plant*, and only the second
   varies:

**Material-boundary exemption (#4304):** ground-cover blades are a drawn surface with no canonical `Material` — no `GpuMaterial` row, no `MaterialTable` intern. Shading input is the engine-canonical `GroundCoverPalette` this document specifies, which IS the parser→canonical translation for the species table (no `Imported*` tier, no renderer per-game branch). Recorded as a deliberate exemption in `nifal.md` §3 alongside Cornell and `crates/save`.

   * `GROUNDCOVER_SHEEN_F0 = 0.034` is derived, not chosen — plant cuticular
     wax has a refractive index of ~1.45, so `((1.45−1)/(1.45+1))²`. It is a
     property of the wax and therefore species-independent.
   * `GROUNDCOVER_SHEEN_ROUGHNESS = 0.3` sets the width of the Charlie lobe
     (Estevez & Kulla 2017, the fibre model `KHR_materials_sheen` adopted),
     which is a property of a ribbed, curved blade and likewise does not vary
     between grasses. This is the one number here still wanting a render.
   * `GroundCoverSpecies::sheen` scales the *amount*, which is how much intact
     cuticle there is. A dry summer grass silvers more than a green reed, and
     the shipped defaults order them that way (0.70 arid, 0.45 temperate).

   Wetness would move both, and stays deferred with §12.3's weather coupling
   exactly as this question proposed.

---

## 12. Reading as organic — the terms beyond placement

Sections 1–11 answer *where* ground cover is. That is necessary and not
sufficient: a perfect distribution of individually-lit cards still reads as
cards. The six terms below are what separate that from a living surface, and
not one of them is a placement question — which is why none appears in §1,
whose comparison is the Creation Engine.

They divide cleanly: §12.1–12.4 are what the stratum *is* (occlusion,
translucency, ground coupling, interaction); §12.5–12.6 are how it answers
light (canopy shadowing, sheen and reflection). Every one is analytic — no
ray, no probe, no bake step — which is not a compromise but the point: at
ankle height across a whole worldspace, a closed form that is nearly right
beats a traced one that cannot be afforded per blade.

That comparison is the limit of §1. Its three counters get us to *not
Bethesda*; they do not get us to organic. The reference for the stated goal is
modern AAA vegetation — RDR2, Ghost of Tsushima, Horizon — and against that
reference these four are the visible difference.

Three of them require canonical type changes (`GroundCoverSpecies` gains a
transmission colour, a ground-coupling weight and a sheen amount), which makes
them a `crates/core` and translate-boundary concern, not a renderer-local one.
Called out per term. They are deliberately **not** added ahead of their
consumers: their defaults have to be calibrated against a render, and landing
three invented scalars in a canonical type is how a placeholder becomes the
value nobody revisits.

**Status.** §12.1, §12.2, §12.5 and §12.6 landed in #4057, along with the
transmission colour and sheen amount they need; the ground-coupling weight
landed early with the type and waits for §12.3's consumer in #4056. §12.4
landed in #4058. The four light terms all live in
[`include/groundcover_light.glsl`](../../crates/renderer/shaders/include/groundcover_light.glsl)
— one header, because they are two pairs and building half of either pair has a
recognisable failure.

### 12.1 Contact occlusion

**The problem.** Light does not reach the base of a dense sward. Blades lit
evenly along their whole length read as a collection of separate objects
standing near each other, because that is exactly what the shading says they
are.

**What the design does today, and why it is not enough.** §7's
`colour_gradient` bakes a darker base into the species — the field's own doc
gives the reason ("the base is self-shadowed by the canopy above it"). But a
baked gradient is a *constant*: the sparse edge of a patch is shaded as dark
at the base as the middle of a meadow, when the whole point is that the middle
is dark *because* it is the middle.

**It has no strength parameter, and that is the answer to §11.6.** §12.1 and
§12.5 are the same extinction through the same slab; see §11.6.

**Approach.** Derive the term from the density field that already exists.
`d_ground` is evaluated per candidate point at scatter time and §4 already keeps
it in the blade record for width compensation, so the blade shader has it for
free — no new buffer, no second evaluation. Occlusion runs from full at the base
to none at the tip, scaled by `d_ground`. A blade standing alone is lit along its
length; the same blade in a thicket is not.

**`d_ground`, never `d_draw`** (§3). Scaling occlusion by the view-faded density
would make a meadow's interior brighten as the camera retreats from it.

**Consequence for §7.** Once this is computed the species gradient must stop
baking it, or the two compound and the base goes black. The gradient then
becomes what it should always have been — the plant's own colour variation,
not a stand-in for a lighting term. Done in #4057: both built-in species'
`colour_gradient[0]` was lifted to what a grass sheath actually is — slightly
paler and warmer than the lamina, not three stops darker — and
`the_default_gradients_no_longer_bake_a_self_shadow` keeps it that way.

### 12.2 Translucency

**The problem.** Grass blades are thin enough to transmit. Backlit vegetation
glows; a shader without transmission renders the same scene as silhouettes. At
low sun angles — which is most of the time anyone stops to look at a landscape
— this single term is most of the read.

**Approach.** A transmission lobe driven by the light direction *through* the
blade, modulated by thickness. Thickness is derivable from data already
present: species width, tapered by the blade's parametric height, so tips
transmit more than bases without storing anything per blade.

**The ordering subtlety.** The main pass traces a shadow ray per fragment.
Transmission must not simply be multiplied by that result: a blade shadowed by
a distant rock genuinely receives nothing and must not glow, but a blade
shadowing *itself* — the near face of a lit blade — is precisely the case that
should. The two need separating rather than folding transmission into the
existing shadow multiply, which is the shape a first implementation reaches
for and the reason backlit grass so often comes out flat.

**Type change.** `GroundCoverSpecies` gains a transmission colour. A leaf does
not transmit its own reflected colour — it transmits warmer and more saturated
— so this cannot be derived from `colour_gradient`.

### 12.3 Ground-colour coupling

**The problem.** §7 sources species colour from the `GRAS` model's texture:
per-species, and constant across the whole world. Real vegetation takes its
colour from what it grows in. A meadow on red clay is not the green of one on
peat, and when grass and ground disagree the grass reads as a layer resting on
the terrain rather than as part of it.

**Approach.** Blend the species gradient toward the terrain albedo sampled at
the blade base, by a per-species coupling weight. The input is already there:
the scatter pass samples splat weights at exactly that point for §3's affinity
term, and the terrain-tile SSBO carries the layer texture indices.

**Why this pairs with §6's tier 3.** The always-on terrain detail layer and
this term are the same idea from opposite ends — one makes the ground look
like the grass, the other makes the grass look like the ground. They share an
input and should be authored together; done separately they will disagree, and
the disagreement will be visible at exactly the distance where geometry hands
off to the detail layer.

**It also partly answers a Phase 5 question.** The colour gradient from the
`GRAS` model texture is still unimplemented (see Phase 5's note), and this term
reduces how much that matters: if colour is substantially coupled to the
ground, a per-species base gradient is a smaller input than §7 assumed. Worth
measuring before building the texture-sampling path, not after.

**Type change.** `GroundCoverSpecies` gains a coupling weight.

**IMPLEMENTED (2026-09-24, #4056).** The weight rides the GPU species
record's `tipColour.a` (already carried since the type landed; this change
added its only reader). The blade vertex shader captures the terrain sample
it already takes at the blade base — splat weights, the terrain-tile slot
added to `GroundCoverCell`, and the terrain diffuse UV rebuilt from world
position — and the fragment blends the species gradient toward the
splat-weighted average of the cell's painted layer diffuse textures, by
`coupling × (1 - blade_t)`: strongest at the root, zero at the tip. Cards
skip the term (their colour already comes from the shared palette atlas, so
they cannot drift from the ground). The UV reconstruction is pinned against
the terrain vertex authoring formula by
`groundcover_terrain_uv_matches_terrain_rs` — the row axis runs opposite
world Z, and getting that sign wrong samples a mirrored row, the failure
class `terrain_sample.glsl`'s own block warns about. The fragment fetches
with `textureGrad` on unconditionally captured varying derivatives: a 2×2
pixel quad routinely straddles two blades, so nothing in the loop is
quad-uniform (#4016's hazard class).

### 12.4 Interaction

**The problem.** Grass that does not move when something walks through it is
static scenery, whatever else is right about it. This is the term with the
largest gap between "cheap" and "sells the whole feature".

**Approach.** A displacement field, structurally the same object as §8's wind:
a small world-space texture centred on the camera, rendered each frame from the
dynamic entities near it, sampled by the blade vertex shader at the blade base
and applied as a bend away from the disturbance. Because neighbouring blades
sample a continuous field at nearby points they part *together* — a channel
through the grass, not a ring of individually-tilted blades.

**Recovery is the part that is easy to get wrong.** Blades snapping upright the
instant an entity passes reads worse than no interaction at all, because it
draws the eye straight to the boundary. The field has to decay rather than
clear, so a trail persists behind a runner and fades. That makes the texture
stateful — accumulated and decayed per frame rather than re-rendered from
scratch — which is a different and slightly larger thing than the naive
version, and the reason to say so here rather than discover it in
implementation.

**Independent of Phase 4.** It shares no data with the RT proxy shell and needs
nothing from it. See §10's revised entry.

**Out of scope within this term.** Physical simulation of blades, collision
response, and any feedback from grass back onto the entity. The field is
one-directional.

**Which entities feed it — ANSWERED 2026-09-06 (#4058): every live actor**,
the player included, meaning anything carrying `ActorValues` and a world
transform. The two alternatives the question named lose without needing a
render:

- *Player only* makes the world feel dead the moment an NPC walks past you
  through the same grass and leaves none of it moved. The point of the term is
  that the stratum reacts to the world, not to the camera.
- *All physics bodies* spends the budget on every dropped bottle and every
  settled prop — most never move again, none is being watched, and a disturber
  costs one loop iteration per field texel.

An actor's radius comes from its own character-controller capsule when it has
one and from `CharacterController::HUMAN` (36 units across) when it does not.
Carts, boulders and spell effects are additions to that list later, not
reasons to have picked a different one.

**Extent and resolution — 2048 units across, 256², so 8 units per texel.** The
extent is half an exterior cell: interaction is only legible at ankle height,
so the field does not need §6's 2000-unit draw distance, and covering it at
this resolution would cost 64× the memory to animate grass nobody can see move.
The resolution puts ~4.5 texels across a vanilla actor capsule — enough for the
blade shader's bilinear fetch to give the channel shape, and coarse enough that
neighbouring blades read almost the same value and therefore part *together*,
which is the property the whole approach rests on. 256² × 4 B × two halves =
512 KB device-local. Both numbers are the cost/quality trade the question
flagged and both still want a render to confirm.

**Implementation notes worth having up front.**

- The field is a plain SSBO, not a storage image. The blade vertex shader needs
  a bilinear read, which is six lines of arithmetic against a sampler, two
  image layouts and a transition per frame.
- The origin is **snapped to the texel grid**, which makes the frame-to-frame
  reprojection an exact integer copy. A fractional offset would need a filtered
  fetch, and a filter applied every frame to its own output is a low-pass
  running at frame rate: a crisp channel smears into nothing within a second.
- Disturbers combine by per-texel **maximum**, not by sum, so a second pass
  over the same ground refreshes the trail instead of doubling it, the field
  stays bounded without a clamp that would flatten the response near a crowd,
  and recovery stays a pure decay with nothing to unwind.
- A trodden blade also gets **shorter** (`sqrt(1 − k²)`, the vertical leg of a
  fixed-length blade whose tip has moved `k` of that length sideways). Without
  it the tip stays at full height and only slides, which reads as grass combed
  rather than walked through.
- The field is one persistent allocation shared by every frame-in-flight,
  because it is *state*; a per-slot copy would give a 2-frame pipeline two
  independent trails that alternate. The consequence is a real cross-frame
  read-after-write hazard, closed by a leading barrier in each frame's command
  buffer.

### 12.5 Canopy shadowing — the cheap one

**The premise, restated.** Grass that casts no shadow reads as pasted onto the
terrain. §5 Stage 2 used to answer that with a ray-traced proxy shell; this
section is the reason it no longer has to.

**The observation.** Almost all of the visual work a grass shadow does is
*contact darkening* — ground under and beside a clump is darker, and darker
still as the sward thickens. That is not a shape that needs a ray. It is a
function of the density field and the light direction, and every shader
involved already has both.

**Approach — extinction through a canopy slab.** Treat ground cover as a
participating slab of thickness equal to the local blade height, with optical
density proportional to `d_ground` — the intrinsic value, never the view-faded
`d_draw` (§3), or the shadow under a meadow lightens as you walk away from it.
Transmittance along the light direction is Beer–Lambert:

```
T = exp(−k · d_ground · h / max(cos θ, ε))
```

with θ the light's angle from vertical. A low sun therefore traverses more
canopy and the shadow deepens and lengthens on its own — the behaviour that
reads as a real shadow — out of a closed form, with no BLAS, no TLAS entry and
no ray budget. `k` turned out to be two physical numbers rather than one
calibration scalar — an extinction coefficient and a leaf area density; see
§11.9, which #4057 answered.

**How the terrain receiver gets `d_ground`.** It evaluates the same
`byroGcDensityGround`, off the same include, from `triangle.frag`. Splat
weights and the geometry normal are already there as interpolated vertex
attributes; the height stencil §3's shelter term needs goes through
`byroSampleTerrain` against the global vertex SSBO, which `triangle.frag`
already binds. The four remaining per-cell inputs — the affinity table, the
water plane, the cell origin and the canopy thickness — ride `GpuTerrainTile`,
which grew from 96 to 144 bytes for them and is now 160 B with the Tier-3
detail-atlas selector. They ride that record rather than a
new SSBO because that is exactly what it is: per-LAND-tile data indexed by the
tile slot `GpuInstance.flags` already carries, so there is no parallel array
and no second descriptor binding to keep in step.

`canopy_height == 0` disables the term, which is what a LOD tile, a BTXT-only
cell, an interior and a worldspace with no resolved palette all arrive as. The
attenuation attaches at `shadowableLightRadiance`'s single exit, for
directional lights only — splitting it across `triangle.frag`'s five call
sites would break the #1369 invariant that the ReSTIR estimator's shadowed
subtraction cancel bit-for-bit against its unshadowed accumulation.

**Two receivers, one term.**

- *Terrain* — multiply direct sun by `T` at the terrain fragment. This is grass
  shadowing the ground, which is the shadow anyone actually notices.
- *Blades* — evaluate `T` at the blade's own height within the slab, so a blade
  deep in the sward is shadowed by the canopy above it while one at the edge is
  not. This is what gives a patch interior depth instead of uniform brightness.

**Distinct from §12.1, and both are needed.** §12.1 is ambient occlusion — how
much of the *sky* reaches a point. This is directional — how much of the *sun*
does. They share the density input and nothing else. Implementing one and
expecting it to cover the other is a standard mistake with a recognisable
symptom: grass that is either flat under overcast or shadowless in direct sun,
depending on which half was built.

**On "baked".** There is nothing to bake a blade shadow into. §4 generates blade
geometry in the vertex shader from a seed, so there is no per-blade mesh, no UV
space and no atlas; "baked" for ground cover means *closed-form and
precomputed-parameter*, not a bake step. The one genuine exception is §6's tier-2
clump-card atlas, which is the only ground cover with real texture space — those
cards should be authored with this term already applied, and that is a reason to
generate them after Phase 6 rather than during Phase 3.

### 12.6 Sheen and reflection

**The problem.** Grass is not Lambertian, and treating it as such is why cheap
vegetation reads as painted cardboard however good the distribution is. A meadow
with the sun low and ahead of you goes silver; the same meadow with the sun
behind goes deep green. Diffuse-only misses both.

**Approach — a grazing sheen lobe.** Blades carry a waxy cuticle: near-dielectric
and strongly reflective at grazing angles. One Fresnel-weighted sheen lobe over
the diffuse response captures the silvering for a few instructions and no rays.

It pairs with §12.2 exactly as §12.5 pairs with §12.1 — sheen is the front-lit
half, transmission the back-lit half, and either one alone leaves the meadow flat
from one direction. Build them together.

**Ambient reflection without a ray.** A blade's environment is the sky, and the
sky parameters are already a resource. A Fresnel-weighted sky tint at grazing
angles is the whole of it. Grass is not a mirror; nothing is legible in its
reflection, so there is no probe, no cubemap and no reflection ray to justify.

**Grass in *other* surfaces' reflections — already solved, for free.** Ground
cover has no TLAS presence, so a reflection or GI ray cannot hit a blade. It can
and does hit the terrain, and §6's tier-3 detail layer means the terrain already
carries the grass's colour and density *at every distance including zero*. A
reflection ray sampling a lake shore gets ground that is correctly
grass-coloured, and at ankle height that substitution is very nearly right.

This is worth stating plainly because it changes what §6's tier 3 is worth. It is
not only the LOD floor: it is simultaneously the cheap **reflection**
representation and the cheap **GI** representation of ground cover, and it is
most of why §5's proxy shell stopped being load-bearing. Three jobs, one
mechanism — which is a third independent reason to build it first within Phase 3,
alongside the one §6 already gives.

**Type change.** `GroundCoverSpecies` gains a sheen amount. A dry summer grass
and a wet reed do not silver equally.

### 12.7 Ambient comes from the ground's GI, not a flat term (2026-09-13)

**The defect.** The blade pass wrote only the HDR direct attachment. The
G-buffer's albedo and raw-indirect attachments kept the terrain's values from
underneath, and composite reassembles `direct + indirect × albedo` per pixel.
So every blade pixel received the *terrain's* indirect times the *terrain's*
albedo, on top of the blade's own flat `sceneFlags` ambient: the sky counted
twice, paired with the wrong reflectance. On Skyrim tundra (`2,-4`) the blades
rendered as glowing yellow-green needles over darker ground.

**The fix (`4d54c73a`).** Blades write their own albedo, multiplied by §12.1's
sky occlusion, and leave raw indirect holding the ground's demodulated GI.
Composite's reassembly then lights the blade with the irradiance the ground
beneath it receives. At ankle height that is very nearly the blade's own, and
it is the same substitution §12.6 already argues for reflections. The flat
diffuse ambient is gone; the sheen ambient stays in the direct term.
`gpu_main_render` was unchanged (7.55–7.81 ms before and after; one 13.3 ms
sample was run-to-run noise, confirmed by a repeat run).

### 12.8 Placed geometry covers the soil (2026-09-13)

**The defect.** Grass grew through roads. On Skyrim `2,-4` a dump of every
painted landscape layer (grass, tundra, rock, snow, dirt, riverbed, pine forest)
contains no road or cobble layer at all. The cobblestone road is a **placed
mesh** lying on ground painted as grass, and §3's density field reads only the
terrain and its paint, so it cannot see it. No keyword table or affinity value
can fix that.

**The test.** After a candidate passes the accept test, the scatter traces the
TLAS straight down through the space a blade rooted there would occupy:

- **From** the root plus the palette's tallest blade. A surface lower than that
  over the root is one a blade would pierce, so the reach is a property of what
  is growing, not a tuned distance.
- **To** one representable step above the *rendered* terrain triangle, using
  the engine's existing scale-aware offset (Wächter & Binder 2019). The terrain
  sampler now also returns `meshHeight`, the height on the triangle
  `cell_loader/terrain.rs` actually emits. The bilinear `height` is off that
  triangle by up to `|h00 + h11 − h01 − h10| / 4` on a non-planar quad, which
  would read the terrain's own surface as something lying on top of it.
- **Against** `VISIBILITY_LAYER_ARCHITECTURE | VISIBILITY_LAYER_STATIC_PROP`
  only: an actor walking through grass does not uproot it, and cutout foliage
  and effect cards are not ground.

Any hit inside that span rejects the candidate. The terrain lies below the
span by construction, so the test needs no terrain-identity check and no
tolerance. Two alternatives were rejected:

- *Reading `INSTANCE_FLAG_TERRAIN_SPLAT` off the hit instance.* It is a
  rasterizer-only bit, skipped for off-frustum draws, and ray-query consumers
  are forbidden from reading it.
- *A terrain bit in `VisibilityMask`.* That mask is core light-record data, and
  when this was decided `for_legacy_projection(false)` mapped legacy lights to
  `ARCHITECTURE` alone, so moving terrain to its own bit would have silently
  stopped those lights being shadowed by terrain. (Since `b9e961eeb` the
  projection mask is `FULL` for every light, so that hazard is gone; the
  core-light-data objection still stands.)

**Measured.** Same scene and camera: `covered=1106` of 6,469 accepted blades
(17%) rejected, the road interior clear of roots. The remaining blades that
cross the road are rooted on grass at its edge and lean over it, which is the
blade-height problem, not a cover failure. The validation layers reported only
the VUID types this scene already had. The `groundcover:` bench row now ends in
`covered=`.

### 12.9 A scatter point grows a tuft, not a blade (2026-09-13)

**The defect.** One blade per accepted point read as scattered needles. The
candidate cap is 1,024 per 512-unit chunk (≈19 per m² at full density), and on
Skyrim tundra the field accepts far fewer, so the sward was a few blades per m²
where every reference is an order of magnitude or more above that.

**The change.** Each accepted point now grows `GROUNDCOVER_BLADES_PER_POINT = 4`
blades, the full-detail count from Outerra ("4 blades generated from a single
point in the canopy texture", over canopy data of "roughly 30cm"). The
scatter's candidate grid is the same scale (512 / √1024 = 16 units ≈ 23 cm), so
this is that source's density at matching point spacing, and it costs no
scatter work. Outerra does not state how the four are arranged; the blades here
take the source literally and share the point's root and species, each drawing
its own seed for height, yaw, twist, lean and colour jitter.

**Measured** on Skyrim `2,-4`, same camera, two runs each: 5,363 accepted
points; `gpu_main_render` 7.05–7.91 ms at one blade per point and 7.68–8.14 ms
at four. The blades read as tufts rather than needles. The field is still sparse
on this ground, because most candidates carry a low `d_ground`; that is §11.3's
calibration question, not this one.

### 12.10 The flat ribbon shades as a folded leaf (2026-09-13)

**The technique.** Ghost of Tsushima tilts "the normals of the grass blades
outward a bit" for "a more natural rounded look … a lot cheaper than adding more
verts" (GDC 2021, 13:45–13:54), but gives no amount. The amount here comes from
Jahrmann & Wimmer 2017, §6.3's "3D displacement": the blade's middle axis is
translated along the normal, "resulting in a 'v'-shape in its cross-section",
by Eq. 23, `d = w n (0.5 − |u − 0.5| (1 − v))`, which the prose says gives
"approximately a right angle" with the unfolded width increased by √2.

**The width convention, and a discrepancy in the draft.** The draft's Eq. 21
writes the blade edges as `c ± w·t1`, a span of `2w`, which would halve the
slope Eq. 23 produces (≈26.6° at the base, not a right angle). The authors'
demo resolves it: in `GrassDrawShader.tes` the edges are
`i1 = centre − off/2`, `i2 = centre + off/2` with `off = tcBladeDir * tcV2.w`
(lines 113–123), so `w` is the **full** width, and the displacement is exactly
Eq. 23 (line 54, commented "ca rechter winkel", roughly a right angle). Each half
therefore rises `0.5·w·(1 − v)` over a half-span of `0.5·w`: a face slope of
`(1 − v)`, a tilt of `atan(1 − v)`, 45° at the base and none at the tip.

**The implementation.** The blade vertex shader adds
`widthAxis · sideSign · (1 − t)` to the unit flat normal and renormalises,
rotating it by `atan(1 − t)` toward that vertex's own edge. The ribbon has one
vertex per edge, so interpolation across it rounds the fold. The demo shades
with the flat normal because its V is real geometry; here the geometry stays
flat, and the fold exists only in the shading. The fragment shader's two-sided
flip negates the whole vector, which turns the tilt inward on the concave back
face.

**Measured.** Skyrim `2,-4`, same camera: `gpu_main_render` 7.68–8.40 ms against
7.68–8.14 ms before, within noise. **No visible difference could be
established at that view:** the blades are 1–4 pixels wide there, and wind
animation places them differently between captures, so a normal rotation across
their width is below what the comparison can show. It ships on the strength of
the source and its zero cost. The view also shows why shading alone cannot fix
the needle look while blades up to ~280 units tall are ~1 unit wide.

### 12.11 Species are drawn by climate weight (2026-09-13)

**The defect.** §7 specifies selection "by weight × local conditions", and the
translate boundary resolves a `climate_weight` for every species. Nothing ever
read it: the scatter picked `(hash >> 24) % speciesCount`, so every species in
the load order was equally likely everywhere. On Skyrim that put the
underwater kelp (`WaterKelpGrass01`, a 279-unit model) on dry tundra as often
as tundra grass, and a kelp-height blade is most of what read as a needle.

**The change.** The host builds a 256-entry selection table from each
species' weight in the palette's climate, and the scatter indexes it with the
same 8 hash bits it already reserved for species
(`GROUNDCOVER_SPECIES_TABLE_SIZE`, scatter binding 7). A species holding `k`
entries is drawn with probability `k / 256`. The table is filled by
largest-remainder rounding after reserving one entry per species of positive
weight, so quantisation cannot drop a species the palette says belongs here.
With no positive weight it falls back to uniform, which is the old behaviour.

**Measured.** Skyrim `2,-4`, same camera: the tallest blades are gone from the
frame, the accepted count is unchanged (5,363, as expected: selection changes
which species grows, not whether a point is accepted), and `gpu_main_render` was
6.97 ms. **This is a partial fix.** Tamriel resolves to `Temperate`, and a
species whose editor ID reads as another climate keeps the translate boundary's
non-zero tail weight, so underwater species are rarer but not absent. Records
that are not grass at all are Phase A's content classifier, below.

### 12.12 Hybrid blades and authored cards (planned 2026-09-13)

A three-track research pass (published blade techniques, grass botany, and a
census of Skyrim SE's 27 vanilla `GRAS` models) found that the blade look was
built on a wrong premise:

- **Every vanilla Skyrim `GRAS` model is 3–16 alpha-tested cards; none models a
  blade.** `OBND` z is the tallest card in the clump. The painted blades are
  ~0.23–0.60 units wide, the tallest ~33–57 units, the median ~12–22.
- **Only ~14 of the 27 records are grass.** Three are opaque rock meshes, two are
  leaf decals lying flat, and the rest are fern, coral and kelp (the 279-unit
  maximum).
- **Real leaves and every renderer set width independently of height.** Flat
  meadow leaves run ~15:1–50:1 length to width (Edgar & Connor 2000; Flora of
  China), flowering culms "often well overtopping leaves", so a tuft's height is
  not a blade's length. Jahrmann & Wimmer's demo sets width and height from
  separate ranges (~1:9–1:25); Outerra and AMD widen blades as density falls.

The chosen direction is a **hybrid**: procedural blades near the camera, the
records' own authored models as well, with a handoff between them.

**Which records grow blades — decided 2026-09-13: none.** A four-game census
(184 `GRAS` records across Skyrim SE, FNV, FO3 and Oblivion) tested whether a
record's model content can say whether it is grass:

| Separates grass from | Feature | Result |
|---|---|---|
| Rock meshes | model is opaque (no alpha test or blend) | exact (Skyrim only has them) |
| Submerged plants | authored `water_distance_application == 2` ("Below – At Least", xEdit numbering; see `gras.rs`) | exact, and authored |
| Flat leaf decals | median card tilt > 55° from vertical | 31° margin on Skyrim alone, **1° pooled** |
| Ferns, shrubs, heather, gorse, sage | any measured mesh or alpha statistic (painted width, alpha coverage, tilt, proportions) | no usable cut; best ≈30% error |

A broad-leaf plant differs from grass only in its painted shape, so no rule on
the model can decide it without misjudging a large share of the corpus. So
blades stop coming from `GRAS` records at all:

- **Procedural blades are the engine's own sward**, sized from the cited
  botany and technique sources, with species set by climate. They need no
  per-record classification.
- **Every `GRAS` record draws its authored model** — grass cards, rocks,
  ferns, decals alike — placed by the density field and climate weights, with
  its authored water rule deciding dry ground from submerged.

Phases:

1. **Phase A — decouple.** The blade palette stops translating `GRAS` records
   into blade species. That also removes the kelp's 279-unit height from the
   cover test's reach. Climate-weighted selection (§12.11) is already in place.
2. **Phase B — sourced blades.** Replace the built-in species' uncited
   dimensions (6–14 units tall, 0.7–1.4 wide) with sourced ones and density
   compensation.
3. **Phase C — the authored-model tier.** Instance each record's model at
   density-field points, weighted by climate, honouring its water rule.
4. **Phase D — the handoff** between blades and authored models.

**Phase C landed (#4413).** How it is built:

- **Translate.** `groundcover_translate::resolve_authored_cover` turns the
  load order's `GRAS` records into `AuthoredCover`, installed by
  `install_ground_cover`. The climate weights come from
  `classify_species_name` / `climate_weights_for`; a record with no climate
  token is weighted as generic temperate. A record is dropped when it has no
  `DATA`, no model, zero density, or a water rule of 6/7 (no vanilla use and
  no documented meaning).
- **Templates.** Each record's model is spawned once per worldspace through
  the normal cell-loader mesh path (`cell_loader::spawn::authored_cover`),
  so its mesh, textures and NIFAL `Material` resolve exactly as a placed
  object's would. Its shape entities carry `AuthoredCoverTemplate` and are
  never drawn themselves. The static-mesh collector builds their draw
  (interning the material into the frame's table) and hands it to the tier.
  The templates hang off a pseudo cell root that `WorldStreamingState` owns
  and `drain_streaming_state` reclaims.
- **Placement** (`groundcover_models.comp`, three phases). Candidates sit on
  the game's grass grid, one per `iMinGrassSize` square, jittered by
  `position_range` and reflected back inside the chunk. Each candidate picks
  a record from a climate-weighted table and is accepted with probability
  `density × affinity × slope × shelter × clump × distance_fade`. The
  moisture factor is replaced by the record's own water rule, which is what
  lets kelp grow below the water plane. The blade tier's cover ray keeps
  models off roads. Counts and ranks are assigned in candidate order, so the
  frame is scheduling-independent and a truncated frame drops the same
  plants every time.
- **Drawing.** Placed plants are written as `GpuInstance`s into the tail of
  the main instance buffer (`GROUNDCOVER_MODEL_MAX_INSTANCES`, 32,768 slots)
  and drawn by the main geometry pipeline, one indexed indirect draw per
  shape, before the first blended batch. Cards alpha-test and rocks shade
  as rock through the shared material path. The tier is receive-only, like
  the blades: it has no TLAS entry.

**Sourced values and interpretations (§12.12 register).**

| Input | Value | Source | Status |
|---|---|---|---|
| Grid spacing `iMinGrassSize` | Oblivion, FO3, FNV 80; Skyrim SE, FO4 20 | Each executable's built-in INI default table, cross-checked where the shipped default INIs set it (`Oblivion_default.ini`, `Fallout_default.ini`, `Fallout4_Default.ini`) | Value sourced. **Reading it as the candidate grid spacing is an interpretation.** UESP only says it "changes the amount of grass per cell, so lower values means denser fields of grass" |
| `GRAS` density | percent of grid points that grow the record | Authored 1–100 in every corpus | **Interpretation** |
| `height_range` | scale `1 ± height_range` (height only unless "uniform scaling") | Authored fraction 0–0.85 | **Interpretation** of both the ± and the non-uniform axis |
| `position_range` | jitter radius around the grid point, units | Authored 7–90 | **Interpretation** |
| Water rule | xEdit zero-based "Unit from water type" 0–5 | `gras.rs` census: only 0/2/3 used; 2 is always kelp, coral or seaweed | Sourced numbering |

**Known limitation.** Selection is by climate alone, as this section
specifies, so within one worldspace every region draws from the same
weighted mix. Tamriel's Reach grass and its tundra grass both appear around
Whiterun. Vanilla keys `GRAS` to a landscape texture (`LTEX.GNAM`). Weighting
the selection by the local splat layers' own `GRAS` lists would localise
species without reintroducing the boundary artifact of §1, and is the
natural next refinement.

Until Phase B supplies sourced blade dimensions, the legacy procedural-shape
inputs `wind_flow_floor`, `wind_stiffness_attenuation`, `rest_lean`,
`terrain_normal_weight`, `twist_radians`, `min_visible_half_width`,
`max_width_multiplier`, `colour_jitter`, and the Tier-3
`groundcover_detail_normal_strength` are explicitly **uncited**. They
are named in `shader_constants_data.rs` (rather than left as shader literals)
so the pending evidence and any eventual retune are reviewable; #4378 moved
them without changing their values.

The 2026-09-19 post-#4378 literal sweep of all 15 `groundcover_*` GLSL files
found exactly one numeric arrival since: the R2 constants
`A1 = 0.7548776662466927` / `A2 = 0.5698402909980532` in
`include/groundcover_candidate.glsl` (`byroGcCandidate`). These are
**cited-math** — the Roberts 2018 generalised golden-ratio weights, cited
in-file and by §13's R2 entry — and so exempt from the uncited register above,
recorded here so the next sweep does not re-flag them. The remaining
#4378-class straggler is unchanged: the debug-only `gl_PointSize` legibility
ramp in `groundcover_blade.vert`'s `GC_DEBUG_POINTS` branch is still uncited
in the shader source — cosmetic debug-view sizing with no published source to
cite, tuning nothing a player sees.

### 12.13 Density from the candidate budget (2026-09-13)

**Measured cause.** After §12.12 Phase A, Skyrim `2,-4` accepted 5,923 of
49,152 candidates (12%) over 48 chunks, about 2,570 m²: ~2.3 points/m², ~9
blades/m² at four blades per point, ~24,000 blades in view. The candidate cap
was not the limiter: at full density the near field allowed ~76 blades/m². The
density field's acceptance was, and the factors doing the thinning — layer
affinity (0.5 for tundra), the clump floor and contrast — are uncited
estimates that §11.3 has not calibrated.

**The change.** Rather than tune uncited factors, the candidate budget rises
4×: `GROUNDCOVER_CANDIDATES_PER_THREAD` 16 → 64 (4,096 per chunk, one per 8
units), with `GROUNDCOVER_MAX_BLADES_PER_CHUNK` matching it so a fully accepted
chunk never overflows, and `GROUNDCOVER_MAX_CHUNKS` 1,024 → 256 so the blade
buffer stays 16 MB (~67 chunks can be within reach; 48 were dispatched here).
The field's relative distribution is untouched — worn ground stays thinner
than meadow — while absolute density moves toward the sourced references:
Outerra's full detail (≈44 blades/m²) and Ghost of Tsushima's ~83,000 blades
in view (GDC 2021, 1:45). At 12% acceptance the expectation is ~37 blades/m²
and ~95,000 blades in view.

**Measured.** Skyrim `2,-4`, same camera, two runs:

| | Before | After |
|---|---|---|
| Accepted points (48 chunks) | 5,923 | 23,399 (3.95×) |
| Blades in view (×4 per point) | ~24,000 | ~93,600 |
| Density over ~2,570 m² | ~9 blades/m² | ~36 blades/m² |
| Overflow | 0 | 0 |
| `gpu_main_render` | 7.46 ms | 7.64–7.88 ms |

The distant ground now reads as a continuous sward. **Up close it still reads
as sprouts:** the built-in species' blades are 6–14 units tall, so near the
camera each covers little of the screen. That is blade size — §12.12 Phase B's
sourced dimensions — not blade count.

**Second rise (2026-09-16, `7996edf61`).** The candidate budget rose another
4×: `GROUNDCOVER_CANDIDATES_PER_THREAD` 64 → 256, taking
`GROUNDCOVER_MAX_BLADES_PER_CHUNK` 4,096 → 16,384 (one candidate per 4 units)
and the blade buffer 16 → 64 MiB. `GROUNDCOVER_MAX_CHUNKS` stayed 256; the
draw distance is 3000 units (~167-chunk bound, ~1.5× headroom). The size-pin
test and `memory-budget.md` ledger row moved with it in the same commit.

### 12.14 Upscaler contract (2026-09-15)

The opaque ribbon tier is ordinary temporal geometry, not a global FSR mask
exception. `groundcover_blade.vert` evaluates the complete blade pose twice:
at the renderer's shared wind clock and at its preceding `delta_seconds`
sample. Both evaluations include the advected wind field and the matching half
of the interaction displacement ping-pong. It projects the preceding pose with
the scene's origin-corrected `prevViewProj`, then `groundcover_blade.frag`
writes `0.5 * (current_ndc - previous_ndc)` into the normal motion attachment.

The ribbon tier is depth-writing and opaque, so its transparency/composition
mask is `0.0`. Its reactive mask is not: since `fd0cd577c` (which replaced
the era when both masks were blanket `1.0`) `groundcover_blade.frag` writes
the transition-driven value
`0.9 * max(midTransition, cardTransition)` — zero at the opaque LOD
endpoints, rising only across either stochastic LOD/card handoff
(`4·w·(1−w)` for the mid-tier weight and the card weight respectively).
It is material-driven and bounded by `0.9`; it must not regress to a blanket
`1.0` mask. Pinned by
`blade_motion_and_fsr_mask_contract_stay_material_driven`
(`crates/renderer/src/vulkan/groundcover.rs`). This is the ground-cover
instantiation of
the [FSR 3.1 input contract](fsr3-upscaler-integration-plan.md#14-fsr-input-contracts),
which keeps vector sign, jitter exclusion, and material-driven masks shared
with the main geometry pass.

> **Acceptance status (2026-09-20, #4506):** rendered-mask acceptance here is
> human-only. The unit guard over these masks is shader-text `contains`
> scanning (#4297), and `m-exteriors.sh` hardcodes `--upscaler taa` in every
> bench mode, so the reactive/linear-compression masks are never exercised
> against the real upscaler on a live frame. Any change to this contract
> needs a manual FSR-mode capture.

---

## 13. References

Every external source this document or its shaders draw on. Where a detail
could not be verified against the source itself, the entry says so.

### Ground cover in games and real-time rendering

- Eric Wohllaib, "Procedural Grass in *Ghost of Tsushima*", GDC 2021. Talk
  video: <https://www.youtube.com/watch?v=Ibe1JBF5i5Y>. Slides:
  <https://archive.thedatadungeon.com/ghost_of_tsushima_2020/documents/gdc_2021/gdc_2021_procedural_grass_in_got.pdf>.
  Timestamps are from the talk's auto-transcript. Used for:
  - ~83,000 blades drawn of just over 1 million considered, ~2.5 ms (1:45)
  - Voronoi clumps driving height, shared facing and colour (6:20–7:03)
  - 15/7 vertex LODs (8:25; slides pp. 20, 23)
  - folding one short blade into two (9:32)
  - rounded normals (13:45; slide p. 29)
  - view-space thickening of edge-on blades (14:05; slides pp. 30–31)
  - distance-calmed specular (14:45; slides pp. 32–33)
  - per-clump colour (15:40)
  - base-to-tip AO and translucency gradients (16:18–17:44)
  - terrain texture at distance (20:02)

  The talk does **not** state blades per m², tile size or blade dimensions.
- Klemens Jahrmann and Michael Wimmer, "Responsive Real-Time Grass Rendering
  for General 3D Scenes", I3D 2017. Draft:
  <https://www.cg.tuwien.ac.at/research/publications/2017/JAHRMANN-2017-RRTG/JAHRMANN-2017-RRTG-draft.pdf>.
  Used for: blade counts (397,881 total, 43,128 drawn; p. 7, §7.1, Table 1),
  the orientation cull (Eq. 17), minimum-width quads (§6.3), index-based
  thinning (Eq. 20), and §12.10's folded-normal tilt from the "3D displacement"
  (§6.3, Eq. 23, p. 6). Blade size and scene area are not stated. The draft's
  Eq. 21 writes the edges as `c ± w·t1`, contradicting its own "right angle"
  prose; the demo below resolves it.
- Klemens Jahrmann, *ResponsiveGrassDemo*, the paper's open-source demo:
  <https://github.com/klejah/ResponsiveGrassDemo>.
  `ResponsiveGrassDemo/shader/Grass/GrassDrawShader.tes`, lines 54 (Eq. 23's
  displacement, commented "ca rechter winkel") and 113–123 (edges at
  `centre ± off/2`, `off = tcBladeDir * tcV2.w`, so `w` is the full blade
  width). Its `GrassDrawShader.fs` shades with the flat normal, since the demo's
  fold is real geometry.
- Outerra, "Procedural grass rendering" (2012):
  <https://outerra.blogspot.com/2012/05/procedural-grass-rendering.html>. The
  GoT talk names it as its main inspiration. Used for: 7 vertices / 5
  triangles per blade, 4 blades from one point on a ~30 cm grid (≈44 blades/m²,
  our arithmetic), halving count while doubling width per LOD.
- Gilbert Sanders, *Horizon Zero Dawn* vegetation talk, GDC 2018:
  <https://media.gdcvault.com/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf>.
  *The full title was not recorded; the URL shows only its opening words
  ("Between Tech and…").*
  Used for: grass mesh LOD triangle counts (slide 23), distance push-down and
  animation scaling (slide 30), double-sided normal handling (slide 47).
- AMD GPUOpen, "Procedural grass rendering" (mesh shaders):
  <https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/>.
  32 blades per patch, 8 vertices / 6 triangles. Read only through a
  summarising fetch, so **its details are unverified**.
- *Red Dead Redemption 2*: named by the project as the visual bar. **No
  primary source describing its grass was found.** Fabian Bauer, "Creating
  the Atmospheric World of *Red Dead Redemption 2*" (SIGGRAPH 2019 Advances in
  Real-Time Rendering) covers sky, clouds, fog and volumetrics; vegetation
  appears only as an example on slide 52. Nothing in this design is derived
  from RDR2.

- "Rendering Countless Blades of Waving Grass", *GPU Gems* (2004), chapter 7.
  Used for §12.12's card model: "The required texture has to cluster several
  blades of grass" (§7.3.1), drawn on "three intersecting quads" (§7.3.2). No
  dimensions are given. *Author not recorded by the research pass.*
- Jahrmann & Wimmer demo, blade dimension ranges:
  `ResponsiveGrassDemo/src/DemoScene.cpp` lines 700–703
  (`bladeMinHeight = 1.3f; bladeMaxHeight = 2.5f; bladeMinWidth = 0.1f;
  bladeMaxWidth = 0.14f;`) and 722–725 (0.6–0.8 / 0.06–0.08); units not
  stated. Tuft height ranges in `src/Grass.cpp` lines 555–605. The width:height
  ratios in §12.12 are our arithmetic on these, not quoted figures.
- **Skyrim SE `GRAS` model census** (2026-09-13): our own measurement, not a
  publication. The 27 records were read from `Skyrim.esm` with `EsmReader` /
  `parse_gras`. Their models came from `Skyrim - Meshes0/1.bsa` through
  `BsaArchive` and were parsed with `byroredux_nif`. Cards were split into
  connected components, and painted blade widths were measured from each
  DXT5 alpha mask at the NIF's own alpha-test threshold. The program lived in
  a session scratchpad, not in the repository; rerun it before relying on its
  figures for another game.

### Botany and canopy physics

- Edgar & Connor (2000), *Flora of New Zealand* Vol. V, Gramineae, web
  factsheets at `https://www.nzflora.info/factsheet/taxon/<Genus-species>.html`. Leaf-blade
  and tuft dimensions for *Lolium perenne*, *Dactylis glomerata*, *Poa
  pratensis*, *Agrostis capillaris*, *Anthoxanthum odoratum*, *Festuca rubra*,
  *Deschampsia cespitosa* ("culms often well overtopping leaves"), *Deschampsia
  flexuosa*, *Nardus stricta* and *Phragmites australis*.
- *Flora of China*, eFloras (`http://www.efloras.org/`, flora_id=2). Vol. 22:
  *Festuca ovina* p. 228, *Calamagrostis epigeios* p. 359, *Bouteloua gracilis*
  p. 494, *Achnatherum splendens* p. 206; Vol. 23: *Typha latifolia* p. 161.
- *Flora of the Canadian Arctic Archipelago*,
  <https://nature.ca/aaflora/data/www/poarla.htm>: *Arctagrostis latifolia*
  subsp. *latifolia*, blades 6–80 mm × 2–4 mm, plants 10–95 cm.

- Matthew, Hernández-Garay and Hodgson, *Proceedings of the New Zealand
  Grassland Association* 57 (1995):
  <https://www.nzgajournal.org.nz/index.php/ProNZGA/article/view/2190/1818>.
  Ryegrass pasture tiller density 5,000 (laxly grazed dairy pasture) to 20,000
  (hard-grazed sheep pasture) per m² (p. 1). *The paper's title was not
  recorded; see the linked page.*
- NC State Extension, "Perennial Ryegrass":
  <https://content.ces.ncsu.edu/perennial-ryegrass>. Leaf width 2–5 mm. An
  extension page, not a paper.
- M. Monsi and T. Saeki (1953): the Beer–Lambert canopy extinction coefficient
  `K = 0.5` for a spherical leaf-angle distribution (§11.9,
  `GROUNDCOVER_CANOPY_EXTINCTION_K`). G. S. Campbell and J. M. Norman, *An
  Introduction to Environmental Biophysics*: the textbook restatement. *As cited
  since #4057; full bibliographic details not re-verified in this pass.*
- W. M. Elsasser: the two-stream diffusivity factor 5/3 used for §12.1's
  hemispherical occlusion (`GROUNDCOVER_SKY_DIFFUSIVITY`). *As cited since
  #4057; not re-verified in this pass.*

### Shading and sampling

- Alejandro Conty Estevez and Christopher Kulla, "Production Friendly
  Microfacet Sheen BRDF" (Sony Pictures Imageworks, 2017): the Charlie sheen
  distribution in §12.6 (`byroGcCharlieD`), which `KHR_materials_sheen`
  adopted. *As cited since #4057; not re-verified in this pass.*
- Colin Barré-Brisebois and Marc Bouchard, "Approximating Translucency for a
  Fast, Cheap and Convincing Subsurface Scattering Look", GDC 2011: §12.2's
  transmission lobe (`byroGcTransmissionLobe`). *As cited since #4057; not
  re-verified in this pass.*
- Martin Roberts, "The Unreasonable Effectiveness of Quasirandom Sequences"
  (2018): the R2 lattice the scatter draws candidates from (`gcCandidate`).
  *As cited since #4054; not re-verified in this pass.*
- Carsten Wächter and Nikolaus Binder, "A Fast and Robust Method for Avoiding
  Self-Intersection", *Ray Tracing Gems*, ch. 6 (2019): the scale-aware ray
  origin offset (`include/ray_origin.glsl`) that bounds §12.8's trace. *As
  cited in that include; not re-verified in this pass.*
