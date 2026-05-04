# #791 — E-N2: unload_cell victim collection scans every loaded CellRoot entity

**Severity:** LOW
**Audit:** `docs/audits/AUDIT_ECS_2026-05-03.md` (Dim 7)
**URL:** https://github.com/matiaszanolli/ByroRedux/issues/791

## Location
`byroredux/src/cell_loader.rs:104-116`

## Description

`unload_cell` filters all `CellRoot` rows on every unload via `q.iter().filter(|(_, root)| root.0 == cell_root)`. With a 3×3 streaming grid (~13500 entities), an unload scans ~13500 rows to find the ~1500 belonging to the unloading cell. SparseSetStorage iteration is fast in absolute terms, but the cost scales as `cells × cell size`.

## Impact

Invisible at radius 1. Measurable at radius 3 (default exterior streaming, 49 cells). Future radius bumps or streaming + interior coexistence will surface this.

## Fix Strategy

Maintain a `HashMap<EntityId, Vec<EntityId>>` resource keyed by `cell_root → owned entities`, populated by `stamp_cell_root` (`cell_loader.rs:77-85`). Unload becomes:

```rust
let victims = world
    .resource_mut::<CellRootIndex>()
    .map
    .remove(&cell_root)
    .unwrap_or_default();
```

Memory: ~8 B per cell-owned entity, dwarfed by the component data.

## Completeness Checks
- [ ] SIBLING: Interior cell unload reuses the same `unload_cell` so this fix covers both
- [ ] LOCK_ORDER: New `CellRootIndex` resource — verify TypeId-sorted ordering against any same-stage resource writes
- [ ] TESTS: Regression: load 5 cells, unload one, assert remaining 4 cell entity counts unchanged AND assert unload runtime independent of loaded-cell count

## Next step
```
/fix-issue 791
```
