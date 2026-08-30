# #3699 — ECS-2026-08-30-D2-01: sparse_set.rs's two storage-population figures are both stale, and disagree with each other (122 vs 147/11; actual 167/10)

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Storage Correctness (doc-rot)
**Location**: `crates/core/src/ecs/sparse_set.rs` (the `EMPTY` sentinel doc, ~:11-25; the `remove_entities_erased` early-out rationale, ~:172-186)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D2-01)

## Description

Two rationale comments in one file quantify the sparse-vs-packed population differently, and neither matches the tree.

- The `EMPTY` sentinel doc (#2148) says the 4-byte slot "halves that floor across all **122** `SparseSetStorage` component declarations".
- The `remove_entities_erased` early-out (#2397) says "**147** sparse component types vs. **11** packed".

Current counts are **167** `type Storage = SparseSetStorage` declarations and **10** `type Storage = PackedStorage` (of which only four are production components — `Transform`, `GlobalTransform`, `WorldBound`, `SceneFlags`; the other six are test fixtures).

## Evidence

```
$ grep -rn 'type Storage = SparseSetStorage' --include='*.rs' . | wc -l   # -> 167
$ grep -rn 'type Storage = PackedStorage'    --include='*.rs' . | wc -l   # -> 10
```

## Impact

Documentation only — both comments justify decisions that remain correct (and more so, since the ratio has widened). But these are the numbers a future maintainer would use to re-derive whether the sentinel and the early-out still earn their complexity, and two contradictory figures in one file mean at least one is guaranteed wrong.

## Suggested Fix

Replace both hard-coded counts with the ratio they are arguing for, or state them once with the `grep`/`rg` incantation that regenerates them.

## Completeness Checks
- [ ] **SIBLING**: Other files quoting a storage-population count checked for the same drift
- [ ] **TESTS**: If a count is kept literal, a source-scanning contract test pins it
