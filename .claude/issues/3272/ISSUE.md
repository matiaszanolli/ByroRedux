# 3272: UI-D4-04: docs/engine/ui.md's test count drifted again - 48+2 documented, 51+2 actual

**Severity**: LOW · **Report**: `docs/audits/AUDIT_UI_2026-08-24.md` (UI-D4-04)

## Description

`docs/engine/ui.md:514` reads "48 default tests plus 2 ignored" (measured 2026-08-21). `4e1afcbe` (2026-08-24) added three new tests without touching this line.

## Location

`docs/engine/ui.md:514`

## Evidence

```
$ cargo test -q -p byroredux-ui
test result: ok. 51 passed; 0 failed; 2 ignored;
```
48 + 3 new = 51.

## Impact

Cosmetic — same drift class `#2730`/`#3088`/`#3153`.

## Related

#3153 (fixed the catalog-count half of this drift class).

## Suggested Fix

One-line edit: "51 default tests plus 2 ignored (measured 2026-08-24)".

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
