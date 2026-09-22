# EXT-D2-2026-09-21-01: Splat-layer budget pin re-derives production arithmetic and is unreachable from real terrain spawning tests

**Issue**: #4732
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (test quality; the bound itself is correct)
**Dimension**: Terrain, splatting
**Game Affected**: all LAND games
**Location**: `byroredux/src/cell_loader/terrain.rs:1647-1668` (test), `:852-856` (`debug_assert!` inside `spawn_terrain_mesh`), `:173` (production `authored_budget = 8 - base_transitions.len()`)

## Description
The test re-derives the production formula itself (`8 - transitions.len()`, `min(20, budget)`), so `transitions + capped <= 8` holds by construction and cannot fail if the production formula changes. No unit test reaches `spawn_terrain_mesh`, so the `debug_assert!` fires only in a debug engine run, not in `cargo test`.

## Evidence
Source reading of both functions; `cargo test -p byroredux terrain` gives 73 passed, 2 ignored — none exercising `spawn_terrain_mesh`.

## Impact
A future budget edit can compile and pass CI, then index `splat1[i - 4]` out of bounds in the first exterior cell with more than 8 layers.

## Suggested Fix
Extract the budget and truncation step into a pure function that `build_cell_splat_layers` calls, and pin it with a test that does NOT re-derive the formula (more than 8 authored layers plus 3 transitions, asserting a fixed expected output).

## Related
#4496 (closed)

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D2-2026-09-21-01)
