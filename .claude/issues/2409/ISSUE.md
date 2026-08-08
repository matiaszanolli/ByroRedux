# TD1-006: cell_loader/references/mod.rs crossed 2000 LOC — load_references_budgeted (cc 58/25) and spawn_synth_child

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2409
**Finding ID**: TD1-006 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `byroredux/src/cell_loader/references/mod.rs:200-922` (`load_references_budgeted`), `:1099-1586` (`spawn_synth_child`)
**Status**: NEW

## Description
`byroredux/src/cell_loader/references/mod.rs` newly crossed 2000 LOC (2161 total). `load_references_budgeted` is 723 LOC at cognitive complexity 58/25 — the busiest touch point in cell loading, with 5 feature commits in the last week. `spawn_synth_child` is 488 LOC at complexity 31/25. Also flags a `clippy::type_complexity` hit on `current_ref_synth`'s type at `:70`.

## Related
Same subsystem/week as the `cell_loader/spawn.rs` finding.

## Suggested Fix
Split per-REFR-type dispatch arms in `load_references_budgeted` into named `dispatch_*` helpers over `&mut RefLoadAccum`/`&CellLoadCtx`; extract `spawn_synth_child` to its own file. Consider a type alias for `current_ref_synth` to address the clippy hit.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the resumable-load behavior across the split
- [ ] **SIBLING**: Check `cell_loader/spawn.rs`'s equivalent dispatch shape for the same pattern
