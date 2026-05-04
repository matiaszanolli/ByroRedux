# #792 — E-N3: stamp_cell_root inner-loop docstring is grammatically muddled

**Severity:** NIT (doc-only)
**Audit:** `docs/audits/AUDIT_ECS_2026-05-03.md` (Dim 7)
**URL:** https://github.com/matiaszanolli/ByroRedux/issues/792

## Location
`byroredux/src/cell_loader.rs:80-82`

## Description

Existing comment:

> entities that were never given any component never created a CellRoot storage entry — the row just stays in the sparse set for lookup

This is muddled. What actually happens: `world.insert(eid, CellRoot(cell_root))` always succeeds for spawned `eid`s in the range, regardless of whether they have other components. Entities that received no other components still get a `CellRoot` row added, which is fine. The comment seems to be defending against a case that doesn't apply.

## Impact

None — pure doc clarity.

## Fix Strategy

Replace with something like:

```rust
// `insert` is overwrite-safe; every spawned entity in `first..last`
// gets a `CellRoot` row regardless of whether it received any other
// components. The unload path filters `CellRoot` storage by
// `cell_root`, so this stamp is what makes the entity reachable
// from `unload_cell`.
```

## Completeness Checks
- [ ] (no completeness checks apply — doc-only change)

## Next step
```
/fix-issue 792
```
