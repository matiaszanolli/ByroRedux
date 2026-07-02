# FNV-D1-03: Exterior terrain/water spawn results dropped without cell-level acknowledgement

**Source audit**: `docs/audits/AUDIT_FNV_2026-07-02.md` (finding FNV-D1-03)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1855
**Labels**: low, import-pipeline, legacy-compat, bug

**Severity**: LOW
**Dimension**: Cell Loading
**Location**: `byroredux/src/cell_loader/exterior.rs:309, 335`
**Status**: NEW

## Description

`terrain::spawn_terrain_mesh(...)` and `water::spawn_water_plane(...)` both return `Option<usize>` but are called with `let _ = ...`. The callees log internally on failure (terrain warns on upload failure; `water.rs:175` warns on mesh-upload failure), so a failure is not fully silent, but the exterior cell-load site gives no correlated "cell (gx,gy) had no terrain/water" signal, making a partial-cell-render harder to diagnose from the cell's own log line.

## Evidence

`exterior.rs:309` `let _ = terrain::spawn_terrain_mesh(...)`; `exterior.rs:335` `let _ = water::spawn_water_plane(...)`.

## Impact

Observability only; no correctness effect. Callee-side warnings already exist — mild redundancy gap, not a silent failure.

## Suggested Fix

Optional — `if ....is_none() { log::warn!("cell ({gx},{gy}): terrain/water spawn failed") }` for per-cell correlation.

## Completeness Checks
- [ ] **SIBLING**: Check other `spawn_*` call sites in the cell-loader (interior route, object/precombined spawn) for the same discard-without-cell-context pattern
