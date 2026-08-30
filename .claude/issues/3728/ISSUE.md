# #3728 — ESM-2026-08-30-D5-01: a REFR's persistent / temporary / visible-distant group membership is discarded at parse time

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: CELL / WRLD Walkers
**Record / Sub-record**: `CELL` child GRUPs 8/9/10 — `REFR`/`ACHR`/`ACRE`
**Location**: `crates/plugin/src/esm/cell/walkers.rs` (`parse_cell_group_inner`, ~:141-159; `parse_refr_group_inner`, ~:672-700); `crates/plugin/src/esm/cell/mod.rs` (`PlacedRef`, ~:372)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D5-01)

**Not a duplicate of #3542** — that issue covers PGRE/PHZD *record types* being dropped. This one is about the *group-type membership* of records that are already parsed.

## Description

`parse_cell_group_inner` matches `6 | 8 | 9` with **one arm** and passes the same `&mut refs` for all of them; `parse_wrld_children_inner` does the same; `parse_refr_group_inner` recurses into any nested group with no record of which one it entered. `PlacedRef` has no field for it — `is_persistent` does not exist anywhere in the workspace (verified by grep at HEAD).

```rust
// Cell children groups (6=temporary, 8=persistent, 9=visible distant).
6 | 8 | 9 => {
    // ... one `parse_refr_group(reader, sub_end, &mut refs, ...)` for all three
}
```

## Impact

No consumer is wrong today (the engine's only "persistent" concept is the worldspace-level persistent CELL, `byroredux/src/streaming.rs:608`), which is why this is LOW. The gap is structural: the persistent/temporary split is what a streaming system keeps resident across cell transitions and what a save system must distinguish on restore, and the information is thrown away at parse time.

## Suggested Fix

Carry a `group_type: u8` on `PlacedRef`, threaded through `parse_refr_group`'s recursion the same way `depth` already is.

## Related

Cross-audit: `/audit-physics` — this is placement metadata a collision/streaming consumer would want.

## Completeness Checks
- [ ] **SIBLING**: Both the CELL and WRLD walkers thread the group type, not just one
- [ ] **TESTS**: A regression test asserts a persistent REFR and a temporary REFR in the same cell are distinguishable
