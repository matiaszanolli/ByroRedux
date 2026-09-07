# #4054 — EXAL ground cover Phase 1: scatter, chunking and the density field

**State**: OPEN · **Labels**: enhancement renderer memory terrain-exterior shaders 

Split from #3807 (items 2 and 7). Design:
[`exal-groundcover.md`](../blob/main/docs/engine/exal-groundcover.md) §3, §4, §9.

**Blocked by #4052** — §11.1's sampling-path measurement decides how the density
field reads terrain, and §4's blade-record layout depends on the same numbers.
Do not start the compute pass before those exist.

## Scope

1. **Chunking** — each exterior cell subdivides into 8×8 chunks of 512 units;
   the chunk is the unit of dispatch, culling and LOD selection. Size is a
   starting guess (§11.2), wants a sweep.
2. **The density field, in GLSL only** (§3). Rust owns every parameter and emits
   them into `shader_constants.glsl` via `build.rs`; GLSL owns the formula. A
   Rust mirror would be a second source of truth for something that must match
   exactly.
3. **`groundcover_scatter.comp`** — one workgroup per visible chunk, mirroring
   `cluster_cull.comp`'s shape.
4. **Debug point rendering** of accepted candidates over real terrain. This is
   where the distribution is judged, before any blade exists.
5. **`OwnershipTracker` classes** for the blade and chunk buffers (#3807 item 7).
   None exist today, so the EX-08 soak currently cannot tell a ground-cover leak
   from anything else. Lands here because this is where those buffers are first
   allocated.

## Two things the design review changed — read before implementing

**`d_ground` vs `d_draw` (§3).** The field is two quantities, not one:

```
d_ground = affinity × slope_gate × moisture × shelter × clump
d_draw   = d_ground × distance_fade(view)
```

Only the scatter's accept/reject test may read `d_draw`. The value written into
the blade record must be `d_ground`, because §12.1, §12.3 and §12.5 all consume
it and a view-faded value makes the shadow under a meadow lighten as the camera
retreats.

**The chunk slice can overflow, and the sequence must be progressive (§4).** A
chunk on rich flat ground will hit its cap. The atomic append must saturate, not
wrap, and the frame must not depend on which threads won the race. That rules
out a blue-noise tile consumed in order: truncating one leaves whatever the first
N entries happen to be, which clumps directionally in exactly the densest chunks.
Use a progressive low-discrepancy sequence, whose every prefix is well
distributed.

## Two traps

- **`clump(noise)` is load-bearing, not decorative.** Splat authority is a 17×17
  alpha grid per 2048-unit quadrant — Bethesda's own resolution — so bilinear
  sampling fixes the hard step but cannot manufacture detail below ~128 units.
  Noise is the only term with authority above that frequency. Stub it to 1.0
  "for now" and the result reproduces the vanilla patch look exactly.
- **`moisture` must resolve to 1.0 where there is no water plane**, not 0.0 and
  not an undefined distance. In a pure product one undefined factor takes the
  whole field, and the symptom is an entire worldspace with no ground cover and
  nothing in the log.

## Explicitly not in scope

The `ExcludedFromTlas` refactor §5 lists under this phase moved to #4053. Blades
are never ECS entities (§2), so nothing here would carry the marker.

## Done when

Debug points render over real terrain in at least two worldspaces, the
distribution reads as organic rather than patchy, and the buffers are tracked.
