# #6201: Ground cover: per-species average blade length with per-blade height variation
STATE: OPEN
LABELS: enhancement,renderer,terrain-exterior

## Request (Matías, 2026-10-20)

> A nice addition to the grass would be having an average length and variable height per blade.

Raised during the terrain-surface-detail review (#6195–#6198): the per-layer height maps from #6195 are the single height source the grass system consumes ("one source of height"), and the blade ground-colour coupling will follow the height-adjusted splat weights from #6196.

## Current state

`groundcover_blade.vert:380`:

```glsl
float height = mix(sp.sizeRange.x, sp.sizeRange.y, gcSeedStream(seed, 0u))
    * clumpHeight * chunk.entryProgress;
```

- Each blade already draws its height **uniformly** from the species `height_range` (`GpuGroundCoverSpecies.size_range.xy`), multiplied by a per-clump scale (`GROUNDCOVER_CLUMP_HEIGHT_VARIATION`).
- There is no species *average* the distribution is centred on, and no separate notion of blade *length* (along the blade) versus standing *height* (after bend/bow). The 2026-09-13 proportion research noted that tuft height and blade length differ in real grasses (culms overtop leaves).

## To settle before implementing
- Whether "average length" means a species mean blade length with a per-blade distribution around it (e.g. normal or log-normal with a variance), replacing the uniform `[min, max]` draw.
- Whether "variable height" means standing height decoupled from length (bend / bow making a long blade stand lower), or simply wider per-blade variation.
- Sources for the distribution parameters (no-guessing policy; cite in `docs/engine/exal-groundcover.md` §13).

## Acceptance Criteria
- Each grass species carries an average blade length and a per-blade height variation, with the distribution and its parameters cited.
- Blade standing height varies per blade around that average, and the existing clump scale still applies on top.
- Before/after captures at the Whiterun tundra position, reviewed by Matías.

