# #3912: REN-2026-09-05-D6-02: render-side glass defaults bypass DEFAULT_GLASS_REFRACTION_SCALE / DEFAULT_GLASS_BLUR_SCALE as bare literals in three sites

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3912 --json state`).*

---

**Audit**: `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (narrowed run — dims 6 & 7) · **Severity**: LOW

## Description

Render-side glass defaults bypass the named canonical constants. `DEFAULT_GLASS_REFRACTION_SCALE` and `DEFAULT_GLASS_BLUR_SCALE` exist, but three sites carry the values as **bare literals** instead of referencing them.

## Impact

Magic-number duplication across three sites for a value that has a named home. Changing the canonical constant silently fails to change behaviour at the three literal sites, and the divergence is invisible until someone renders glass and wonders why the constant did nothing.

## Suggested Fix

Replace the three bare literals with references to the named constants.

## Completeness Checks
- [ ] **SIBLING**: All three literal sites replaced, and no fourth copy exists
- [ ] **CANONICAL-BOUNDARY**: The canonical constants stay the single source. See `/audit-nifal`.

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
