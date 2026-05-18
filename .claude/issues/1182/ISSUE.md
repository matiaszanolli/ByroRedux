# FO4-D4-005: SCOL expansion is single-level; SCOL-of-SCOL nesting not recursed

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: ESM Architecture Records
**Severity**: LOW (forward risk for mod content; vanilla clean)

## Observation

`byroredux/src/cell_loader/refr.rs:380-386`:

```
Vanilla FO4 ships 2616 / 2617 SCOLs with a cached `CM*.NIF` in
`statics[base].model_path`, so the normal path runs for those.
Mod-added SCOLs (and vanilla SCOLs whose CM file is absent under a
previsibine-bypass loadout) hit the expansion branch. Single-level
only — vanilla FO4 has no SCOL-of-SCOL nesting. See #585.
```

`expand_scol_placements` does not recurse if a placement target is itself a SCOL.

## Why bug

If a mod authors a SCOL whose children include another SCOL form ID, the inner SCOL's placements will not expand — they will be treated as opaque base forms and produce `statics-miss` logging. Vanilla is clean (no SCOL-of-SCOL).

## Fix

Recursively resolve each SCOL placement target against `EsmCellIndex.scols` up to a depth cap (mirror `MAX_PKIN_DEPTH`). The composition step (parent transform × child transform) follows the same pattern already used for PKIN-of-PKIN.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: FO4-D4-003 covers the symmetric PKIN→SCOL gap; ensure the SCOL→SCOL and PKIN→SCOL fixes share a helper or at least the depth-cap constant
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic SCOL whose placements target another SCOL asserts the inner SCOL's children fan out, with parent-times-child transforms composed correctly

## Related

- #585 — `expand_scol_placements`
- FO4-D4-003 — sibling PKIN→SCOL/LVLI recursion gap
