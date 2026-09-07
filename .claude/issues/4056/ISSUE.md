# #4056 — EXAL ground cover Phase 3: LOD chain, tier 3 first, plus ground-colour coupling

**State**: OPEN · **Labels**: enhancement renderer performance terrain-exterior shaders 

Split from #3807 (item 4). Design:
[`exal-groundcover.md`](../blob/main/docs/engine/exal-groundcover.md) §6, §12.3.

**Blocked by #4055** (Phase 2 blades).

## Build tier 3 first — it does three jobs, not one

§6 already says the always-on terrain detail layer should land first within this
phase, so later tiers are authored against a correct backdrop. The §12.6 review
found two more reasons, and together they make tier 3 the highest-value item in
the whole LOD chain:

1. **The LOD floor.** Grass geometry is drawn on top of a surface that already
   shows the right colour, variation and density, so when the last geometry tier
   fades there is nothing to pop *to*.
2. **The cheap reflection representation.** Ground cover has no TLAS presence, so
   a reflection ray cannot hit a blade — but it hits terrain, and terrain already
   carries the grass's colour. A ray sampling a lake shore gets correctly
   grass-coloured ground.
3. **The cheap GI representation**, for the same reason.

2 and 3 are most of why the Phase 4 RT proxy shell stopped being load-bearing.

## Scope

- **Tier 3** — terrain fragment shader modulates albedo and normal with a
  ground-cover detail texture whose strength is the same density field, at every
  distance including zero.
- **§12.3 ground-colour coupling** — blend the species gradient toward terrain
  albedo at the blade base, by a per-species weight. Lands here rather than in
  Phase 6 because it is the same idea as tier 3 from the opposite end: one makes
  the ground look like the grass, the other makes the grass look like the ground.
  Built apart they will disagree, and the disagreement is visible at exactly the
  hand-off distance. Adds a coupling weight to `GroundCoverSpecies`.
- **Tiers 1 and 2** — 1-segment blades with compensating width; clump cards from
  a baked atlas.
- **Stochastic density fade** — the acceptance threshold rises smoothly with
  distance while survivors widen, so the population thins without the silhouette
  thinning.

## Two things the review changed

**Widening keys off projected pixel size, not distance.** A blade narrower than a
pixel does not antialias, it flickers, and thin high-contrast geometry is the
case TAA handles worst — the existing resolve will not save it. Hold a floor of
roughly one pixel. Keyed to distance instead, the floor is only correct at the
resolution it was tuned at: the same scene shimmers at 4K and is stable at 1080p,
which reads as a hardware fault and is correspondingly hard to track down.

**Generate the tier-2 clump-card atlas after Phase 6, not during this phase.**
The cards are the only ground cover with real texture space, so they should be
authored with the §12.1/§12.5/§12.6 shading terms already applied. Baking them
first means baking them twice.

## Open question this phase must answer

§11.4 — the tier distances in §6 are unvalidated starting points. Skyrim tundra
and the FNV Mojave have very different sight lines and will likely want
per-worldspace scaling.
