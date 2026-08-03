# SF2D2-03: External .mesh resolve failure is completely silent — the exact #1292 failure mode has no log signal

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2357
**Labels**: bug,nif-parser,medium,legacy-compat

---

**Severity**: MEDIUM
**Dimension**: 2 — BSGeometry Mesh Extraction (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:66-103`
**Status**: NEW, CONFIRMED against current code

## Description

Stage B (external `.mesh` resolve) has three distinct "no geometry found" exits and **none of them logs anything**:
1. `let resolver = resolver?;` — no resolver supplied, silent early return.
2. The per-slot resolve loop (`resolver.resolve(&canonical)` returning `None`) — archive-resolve miss, no log.
3. `let (tri_size, num_verts, data) = found?;` — every slot exhausted (all resolved but parsed empty/errored), no log at this final point.

Only the rarer sub-failure cases *inside* a successful resolve (parse error, sentinel body) log, and only at `debug!`.

## Evidence

Read `bs_geometry.rs:66-103` directly: confirmed all three exit points return `None` with no `log::` call anywhere on those lines. The sentinel-slot and parse-error sub-cases (lines ~80-95) do log at `debug!`, but that's a different, already-covered failure mode.

## Impact

A future archive-set misconfiguration, missing archive, or path-convention drift reproduces the #1292 symptom (near-total mesh-spawn collapse across all vanilla Starfield content — 288,231 of 320,483 `Meshes01.ba2` entries are `.mesh` companions) with an empty log. Recovering the diagnosis last time (#1292) required a dedicated investigation session.

## Suggested Fix

Add `log::debug!` on the resolve miss (naming the canonical path attempted) and `log::warn!` when every slot is exhausted (naming the shape/mesh name). Consider a dropped-`BSGeometry` counter surfaced via `byro-dbg`.

## Completeness Checks
- [ ] **SIBLING**: Check other external-resource resolve paths (texture resolve, BGSM/BGEM resolve) for the same silent-miss pattern
- [ ] **CANONICAL-BOUNDARY**: Not applicable — this is a NIF-import diagnosability fix, not a material-translation boundary change
- [ ] **TESTS**: A regression test pins the new log lines firing on each of the three exit paths
