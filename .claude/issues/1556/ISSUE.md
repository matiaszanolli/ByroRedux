# Issue #1556: OBL-D7-NEW-02: 'Exterior blocked on TES4 worldspace + LAND wiring' framing stale

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: 7 (Doc Staleness) — tech-debt / documentation · **Status**: NEW

**Location**: `ROADMAP.md:120`, `ROADMAP.md:197`, `ROADMAP.md:272`, `docs/feature-matrix.md:24-25` (+ the `✗` cells at `docs/feature-matrix.md:19-20`)

## Description
The docs frame the remaining Oblivion-exterior work as a TES4 worldspace + LAND wiring task. That wiring is implemented and game-agnostic (verified: 31,795 LAND-bearing exterior cells parse from `Oblivion.esm`); the true remaining step is an on-device render bench.

## Evidence
Blocker-chain trace in the audit report — steps 1–5 implemented with file:line evidence:
- TES4 worldspace parse — `crates/plugin/src/esm/cell/wrld.rs:15-183`
- LAND heightmap parse — `crates/plugin/src/esm/cell/walkers.rs:954-1082`
- CELL exterior REFR placement — `wrld.rs:204-216` + `byroredux/src/cell_loader/exterior.rs:371-385`
- Terrain mesh + splat spawn — `byroredux/src/cell_loader/terrain.rs:307`
- Grid dispatch — `byroredux/src/scene.rs:216-250` (no Oblivion exclusion)

## Impact
Mis-frames the remaining work; a contributor could spend effort re-implementing wiring that already exists.

## Suggested Fix
Update the four sites to read "parse + load ✓, exterior render bench pending".

## Completeness Checks
- [ ] **SIBLING**: All four doc sites (3 ROADMAP + feature-matrix, incl. the `✗` cells at lines 19-20) updated consistently
- [ ] **TESTS**: N/A (doc-only change)
