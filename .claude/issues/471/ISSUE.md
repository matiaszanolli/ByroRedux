# Issue #471

FNV-3-M2: EnableParent::default_disabled over-hides every XESP-gated REFR regardless of parent state

---

## Severity: Medium

**Location**: `byroredux/src/cell_loader.rs:796-801`; predicate at `crates/plugin/src/esm/cell.rs:191-201`

## Problem

`EnableParent::default_disabled()` returns true for any non-zero parent + non-inverted flag. XESP semantics require the **parent's** own "initial disabled" flag (REFR bit 0x0800).

Most XESP chains link to persistent, always-enabled actors/statics. The child SHOULD render unless the parent was authored disabled. The current predicate assumes all XESP children start hidden, which is backwards.

The docstring admits the heuristic over-hides ("long-term fix is a state machine"). Issue #349 closed on the assumption that default-disabled parents are the common case — verified wrong on vanilla FNV via UESP + GECK.

## Impact

Large classes of XESP-gated REFRs silently invisible: quest-enabled clutter that is in the world by default, shop inventory references, patrol markers, landscape decorators. Visually missing content on nearly every cell.

## Reference

GECK wiki "XESP" field; UESP "Mod File Format/REFR" REFR flag bit 0x0800.

## Fix

**Long-term**: resolve parent form ID against REFR/ACHR table, consult parent's own flag bit 0x0800. Requires a two-pass loader (first pass builds REFR flag table, second pass applies XESP gating).

**Interim**: invert the default — render XESP children rather than hide. False negatives on "supposed to be hidden" XESP chains is visually less bad than wholesale invisibility. Two-line change at the callsite.

## Completeness Checks

- [ ] **TESTS**: Load a cell with known XESP chain (quest-enabled clutter), assert child REFR renders
- [ ] **SIBLING**: Interim fix should not regress FO3/Skyrim/FO4 cells — same XESP layout but different authoring patterns
- [ ] **DOCS**: Update `EnableParent` doc comment with the two-pass requirement; reference #349 for the regression context

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-3-M2)
