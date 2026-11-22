# #7150: TSD-4.0: displacement measurement spike (BLAS refit, compute, raster) against the 1.0 ms bar

Labels: enhancement,renderer,terrain-exterior

## Goal
A throwaway measurement spike for TSD-4, as section 8 of `docs/engine/terrain-displacement.md` describes. Approved in the #6198 review (2026-11-21). This is measurement only, not the feature.

## Measure
On the 4070 Ti, release build, at the five TSD test positions:

- (a) BLAS refit time for 197 k triangles of a displaced grid
- (b) compute displacement time for 101 k vertices
- (c) added raster time
- (d) the extra `main_render` time with patches resident

Use the defaults: 3 cm amplitude, 6 m radius, 5.7 cm lattice, 24 patch slots.

## Go/no-go bar
- **Go:** total added GPU time at the defaults ≤ 1.0 ms, and refit alone ≤ 0.5 ms.
- **Otherwise:** fall back to static patches (risk 1 in the note), or drop Phase 4 and keep parallax.

## Decisions already made (#6198 review)
- Displacement is upward-only; the base mesh stays as the floor (§3.3).
- Phase 4 is still wanted.
- #7149 (BTXT-only cells get no tile slot) goes first.

## Acceptance
The four numbers per position, a go/no-go verdict against the bar, and a review by Matías before stage 4.1 starts.

