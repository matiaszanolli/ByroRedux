# FO4-D4-003: PKIN expansion does not recurse into SCOL or LVLI children

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: ESM Architecture Records
**Severity**: LOW (edge case in vanilla; documented open gap)

## Observation

`byroredux/src/cell_loader/refr.rs:307-310`:

```
Children that resolve to a SCOL or LVLI stay single-level — those
expansions live in `expand_scol_placements` (#585) and an unimplemented
LVLI helper (#386).
```

A PKIN whose `contents` list includes a SCOL form ID emits a synthetic placement with the SCOL base form ID but does not further expand the SCOL's children. The LVLI expansion path is unimplemented (tracked as #386).

## Why bug

Edge case in vanilla — most vanilla PKINs reference plain statics. Mod-content or future DLC PKINs that include SCOL or LVLI children will silently emit base-form-only placements instead of expanding to the full child tree.

## Fix

After PKIN's children are resolved, run a second resolution pass that checks each child against `EsmCellIndex.scols` / `EsmCellIndex.leveled_items` and recurses. Bound by `MAX_PKIN_DEPTH` to prevent author-loop hangs. The LVLI side depends on #386; SCOL recursion is independent and could land first.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: check `expand_scol_placements` for the symmetric "SCOL contains PKIN" case (FO4-D4-005 covers SCOL-of-SCOL)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic PKIN containing a SCOL form ID asserts the recursion fans out to the SCOL's ONAM-anchored placements

## Related

- #585 — `expand_scol_placements`
- #386 — LVLI expansion helper (unimplemented)
- FO4-D4-005 — sibling SCOL-of-SCOL recursion gap
