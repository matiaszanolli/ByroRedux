# #3910: REN-2026-09-05-D7-03: the #2712 shader-consumption guard covers 9 of 13 sampled supplemental lanes — the four newest are unguarded

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3910 --json state`).*

---

**Audit**: `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (narrowed run — dims 6 & 7) · **Severity**: LOW

## Description

The #2712 shader-consumption guard now covers **9 of 13** sampled supplemental lanes. The four newest are unguarded: the glass-optics lane plus the soft, rim and back-lighting masks.

## Impact

Coverage gap, not a live defect — the four unguarded lanes are correct today. #2712 exists because a supplemental lane that stops being sampled (or starts being sampled from the wrong slot) is otherwise invisible to `cargo test`. The newest four are the most likely to move and the least protected.

## Suggested Fix

Extend the #2712 guard to the remaining four lanes so all 13 are pinned, and make the guard's own list exhaustive against the supplemental set rather than hand-maintained — otherwise the same gap reopens with lane 14.

## Completeness Checks
- [ ] **TESTS**: All 13 sampled supplemental lanes covered by the guard
- [ ] **SIBLING**: The guard's lane list is derived, not hand-written

## Related
- #2712 (the guard)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
