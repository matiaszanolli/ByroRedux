# PHYS-D6-2026-09-21-01: The census's "DROPPED IN TRANSLATION" verdict runs on process-lifetime registry totals, not the probed cell or column

**Issue**: #4684
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Queries & Diagnostics
**Location**: `crates/physics/src/sync.rs:456-479`, `:789-815`; `byroredux/src/cell_loader/nif_import_registry.rs:487-509`, `:395-449`; callers `byroredux/src/commands/physics.rs:139-141`, `byroredux/src/scene/character_spawn.rs:581-583`

## Description
Verdict: `entries.is_empty() && a.classic + a.new_physics + a.phantom > 0 ⇒ "DROPPED IN TRANSLATION"`. `entries` is the probed ±radius column; `a` is the sum over every cached NIF from every cell visited in the process, up to 2048. The registry-wide-zero arm needs a total no real session reaches, and the inference skips checking placements (#2202 design intent).

## Evidence
Both callers pass `registry.collision_authoring_totals()`, which sums `self.core.keys()` (process-lifetime) with `saturating_add`, no per-cell/column scoping.

## Impact
`phys.census` — the command run after falling through a floor — reports every empty column as a translation drop, including genuinely empty ones (terrain gap, unstreamed cell, void outside an interior shell). Same misdirection class as #3965–#3967, opposite direction.

## Related
#2874, #3966, #2202.

## Suggested Fix
Scope totals to the column (sum `CollisionAuthoringSummary` over placement roots whose bounds intersect it), or at least to the current cell. Relabel whatever stays registry-wide. Add a test with a populated registry and an empty column that must not say "DROPPED".
