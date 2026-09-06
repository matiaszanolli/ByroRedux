# #3911: REN-2026-09-05-D7-04: the supplemental write block is pinned for arity but not for role↔slot correspondence — a transposition passes silently

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3911 --json state`).*

---

**Audit**: `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (narrowed run — dims 6 & 7) · **Severity**: LOW

## Description

The supplemental write block is pinned for **arity** but not for **role↔slot correspondence**. The existing test asserts the right *number* of supplemental indices are written; nothing asserts each role lands in its own slot.

## Impact

A transposition — two roles swapping slots — passes the arity pin silently. That is precisely the failure mode the role vocabulary was introduced to eliminate, and it would surface as a per-game visual defect (the wrong map sampled for the wrong purpose) rather than a test failure.

Not a live defect: correspondence is correct today.

## Suggested Fix

Strengthen the pin to assert role→slot correspondence per entry, not just the count. The fixture should use distinct per-slot values so the assertion cannot pass vacuously — the hash-lockstep fixture in the same area already does this and is a good template.

## Completeness Checks
- [ ] **TESTS**: The pin asserts per-role slot correspondence with distinct per-slot values
- [ ] **SIBLING**: Other arity-only pins in the material upload path reviewed for the same weakness

## Related
- #3814 (`supplemental_texture_indices` role pinning; CLOSED)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
