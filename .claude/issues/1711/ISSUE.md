# SPT-NEW-03: OBND-derived bs_bound is computed then discarded on the cell-loader route (loose --tree route keeps it)

**Issue**: #1711
**Source audit**: `docs/audits/AUDIT_SPEEDTREE_2026-06-23.md`
**Severity**: LOW · **Labels**: low, tech-debt, bug
**Dimension**: Per-Game Variants & Route Divergence
**Location**: `crates/spt/src/import/mod.rs:168-178` (computes `bs_bound`); `byroredux/src/cell_loader/nif_import_registry.rs:34` (`CachedNifImport` has no `bs_bound` field — re-mapped from the report's `references.rs` path after the cell_loader split); `byroredux/src/scene/nif_loader.rs:1029` (loose route consumes it)

## Description

`import_spt_scene` produces a Y-up `bs_bound` AABB from TREE.OBND (#995 fix). The loose `--tree` route consumes it (`nif_loader.rs:1029` attaches a `BSBound` component). The cell-loader route does not — `CachedNifImport` carries no `bs_bound` field, so the AABB is dropped. The cell path seeds a `LocalBound` sphere instead (valid for culling), but the precise OBND AABB is computed and thrown away, and the two routes attach different bound components for the same placeholder.

## Evidence

`CachedNifImport` (verified at `nif_import_registry.rs:34`) has no bounds field. The `bs_bound` local in `import_spt_scene` reaches the loose route only.

## Impact

Not a correctness bug — per-mesh `LocalBound` sphere is valid. Minor: wasted OBND→AABB computation on every cell-spawned `.spt`; route divergence (a `BSBound` appears on `--tree` trees but not cell-loaded ones).

## Suggested Fix

(a) thread the AABB through `CachedNifImport` and attach `BSBound` in `spawn.rs` for parity (matches `bsx_flags` round-trip); or (b) drop the `bs_bound` computation since `LocalBound` already covers the cell path.
