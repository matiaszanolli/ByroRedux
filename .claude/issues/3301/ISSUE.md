# EX-16 items 1+5 remainder: REGN incidental spatial emitter + non-Sound RDAT kind selectors

**Labels**: terrain-exterior,esm-plugin,audio

Plan: EX-16 items 1 + 5 remainder (parent #2372).

## Problem
REGN-driven ambient audio is wired end-to-end for exactly one field:
`RegionDataKind::Sound`'s `music` FormID
(`select_active_region_sound` → `RegionAmbientRes` → `dispatch_region_ambient_music`
→ `AudioWorld::play_music`, 3-second crossfade). Two things remain
deliberately unbuilt from the original REGN/audio scope:

### 1. `incidental` (RDSI, FNV-only) spatial ambient emitter
`RegionAmbientRes.incidental` is resolved but has zero consumer.
Unlike `music` (a non-spatial background track — the right fit for
`AudioWorld::play_music`), `incidental` genuinely wants a **spatial,
looping `AudioEmitter`** — a real design decision on emitter placement
(entity position? camera-following? a fixed point per region volume?)
and attenuation, not just dispatch plumbing reusing the `music` shape.

### 2. Non-`Sound` RDAT kinds have no selection logic at all
`RegionDataKind::{Weather, Map, Landscape, Objects, Grass, Imposter}` are
fully parsed (`crates/plugin/src/esm/records/misc/world.rs:452-780`,
`entries_by_priority`) but only `Sound` has a runtime selector
(`select_active_region_sound`). In particular:
- **Weather** selection would overlap with the existing worldspace-level
  climate/weather system and needs its own design pass — this is NOT a
  drop-in generalization of `select_active_region_sound`'s shape, per the
  plan doc's own explicit warning.
- **Grass** (ground cover density/placement) and **Objects** overlap with
  EX-14/15's (#2369) ground-cover streaming work — coordinate rather than
  duplicate.
- **Map**/**Landscape**/**Imposter** are smaller, more isolated selectors.

Also unbuilt: the RDAT `sounds: Vec<RegionSound>` chance-based ambient-loop
list (separate from the single `music`/`incidental` FormIDs) — its
`chance_raw` selection probability has an unresolved fixed-point scale, so
picking one without a verified scale would be guessed-threshold work this
project's conventions forbid.

## Suggested scoping
Given the Weather/Grass/Objects overlaps with other in-flight systems, this
issue should probably be split further once picked up — but file it as one
entry point so the remaining REGN consumption work doesn't get lost. A
reasonable order: Map/Landscape/Imposter selectors first (smallest, most
isolated), then coordinate Weather with the climate system and
Grass/Objects with #2369, then `incidental`'s spatial-emitter design, then
(only once the fixed-point scale is verified against real ESM data) the
`sounds` chance-based ambient-loop list.

## Acceptance
- At minimum: `incidental` reaches a real spatial `AudioEmitter` consumer
  with the same change-guarded crossfade-on-cross-cell discipline `music`
  already has, with tests mirroring `dispatch_region_ambient_music`'s
  failure-path coverage.
- Each additional RDAT kind that gets a selector lands with its own tests
  and its own note on how it coordinates with the overlapping system
  (climate, ground cover) rather than duplicating it.
- `chance_raw`'s fixed-point scale is verified against real ESM data
  (not guessed) before the `sounds` list is wired to anything.

## Related
- Parent epic: #2372 (EX-16).
- Coordinates with: #2369 (EX-14/15 ground cover — Grass/Objects overlap).
- `docs/engine/exterior-readiness-plan.md` §EX-16 items 1 and 5.
