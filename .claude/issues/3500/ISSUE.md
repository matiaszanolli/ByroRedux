# Issue #3500 — SAVE-D3-2026-08-27-05: `save.info` still reports every exterior save as `<none — loose/exterior save>` and never prints its `CurrentExteriorContext`

Source audit: `docs/audits/AUDIT_SAVE_2026-08-27.md`
Filed: 2026-08-27 (HEAD `969d81c8`)
Labels: low, save-load, bug

---

Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D3-2026-08-27-05)
Severity: **LOW** · Dimension 3 — Disk Format & Durability (operator diagnostics)
Data-Loss Class: none

## Location
`byroredux/src/save_io.rs:869-878` (`SaveInfoCommand::execute`)

## Description
`SaveInfoCommand::execute` was not updated when exterior save/load shipped (`0a847910`, EX-09/17 item 4):

```rust
match snapshot_cell_context(&snap) {
    Some(ctx) => lines.push(format!("  cell: {} (esm {}, {} master(s))", …)),
    None => lines.push("  cell: <none — loose/exterior save>".to_string()),
}
```

`snapshot_exterior_context` exists (`save_io.rs:464-471`) and `LoadCommand` already uses it to build a `"worldspace '{}' @ ({},{})"` destination label — `save.info` is the one consumer that never got the second arm. An operator inspecting an exterior quicksave is told it is a loose save that cannot be live-loaded, which is the opposite of the truth.

## Evidence
`byroredux/src/save_io.rs:869-878` (quoted) versus `:1022-1037` (`LoadCommand`'s three-arm `match (snapshot_cell_context(…), snapshot_exterior_context(…))`). `grep -rn "snapshot_exterior_context" byroredux/src/save_io.rs` returns three sites — the definition at `:464`, `LoadCommand` at `:1027`, and the load drain at `:1343` — none in `SaveInfoCommand`. The resource *is* listed later by the generic `for name in snap.resources.keys()` loop, but only as a bare `resource CurrentExteriorContext` line with no worldspace or grid.

## Impact
Diagnostic only — no save or load behaviour changes. It matters because `save.info` is the operator's only pre-load inspection tool over `byro-dbg`, and it now actively contradicts `load`'s own classification of the same file.

## Related
`0a847910` (EX-09/17 item 4); SAVE-D6-2026-08-24-02 / #3028, the doc-side instance of the same omission, now fixed in `5458522d`.

## Suggested Fix
Mirror `LoadCommand`'s three-arm match — print the worldspace key, grid, and load radius for the exterior arm, and reserve `<none — loose save>` for a snapshot carrying neither context.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `snapshot_cell_context` consumer that predates exterior save/load
- [ ] **TESTS**: A regression test pins this specific fix (`save.info` on an exterior snapshot prints the worldspace/grid)
