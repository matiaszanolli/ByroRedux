# #1877 — TD1-2026-07-05-02: byroredux/src/cell_loader/references.rs crossed 2000 LOC (2078, new)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1877
**Labels**: bug, import-pipeline, low, tech-debt
**Filed via**: /audit-publish docs/audits/AUDIT_TECH_DEBT_2026-07-05.md

---

- **Severity**: LOW
- **Dimension**: 1 (File Complexity)
- **Location**: `byroredux/src/cell_loader/references.rs`
- **Status**: NEW
- **Age**: crossed via `9107dfa1` (door-spawn-point selection) landing on top of prior growth; first appearance in the >2000 set.

## Description

`load_references` and its per-REFR placement / collision / light / SCOL-child
handling now exceed the Session-34 split threshold (2078 LOC). No open split
issue. Part of the `cell_loader/` dispatcher family already split into
per-feature submodules; this one re-bloated.

## Evidence

`wc -l byroredux/src/cell_loader/references.rs` → 2078.

## Impact

The REFR-placement hot path — touched by cell-load, precombine absorption,
and spawn-point work — taxes every edit.

## Related

Sibling of the `cell_loader/` per-feature submodule split (load / unload /
exterior / spawn / partial / refr / terrain / …). No open issue.

## Suggested Fix

Extract by responsibility — e.g. REFR placement + transform composition vs
SCOL child expansion vs spawn-point selection (the `door_pos` precedence
block) vs the per-REFR light/collision attach. The spawn-point block is a
natural first cut: self-contained, recently added, and independently
testable.

## Completeness Checks
- [ ] **SIBLING**: `load_references` entry point + `RefLoadResult` return contract preserved so `cell_loader/load.rs` is unaffected
- [ ] **TESTS**: any `*_tests.rs` siblings move with their code and still pass (`cargo test -p byroredux`)
