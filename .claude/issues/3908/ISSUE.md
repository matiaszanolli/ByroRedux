# #3908: REN-2026-09-05-D6-01: "captured, not yet shaded" is stale for seven now-wired canonical Material fields

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3908 --json state`).*

---

**Audit**: `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (narrowed run — dims 6 & 7) · **Severity**: LOW

## Description

The comment "captured, not yet shaded" is **stale for seven now-wired canonical `Material` fields**, across two code sites. The fields are captured *and* shaded; the comment says otherwise.

## Impact

Documentation drift at the NIFAL consumption boundary. A reader looking for unwired material fields is told seven wired ones are unwired, which both hides real progress and invites redundant re-plumbing.

## Suggested Fix

Update both comment sites to reflect which fields remain genuinely unshaded (if any), or remove the qualifier where all listed fields are now consumed.

## Completeness Checks
- [ ] **SIBLING**: Both comment sites updated, and any third copy located
- [ ] **CANONICAL-BOUNDARY**: The audit of what is wired stays accurate at the boundary. See `/audit-nifal`.

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
