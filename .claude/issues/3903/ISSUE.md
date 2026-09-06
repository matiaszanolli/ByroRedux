# #3903: record_external_texture_sources is the fifth hand-written role walk and the only one with no exhaustiveness guard

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3903 --json state`).*

---

**Audit**: found **independently by three audits** in the `texture-roles-deep` suite — `AUDIT_NIFAL` (D8-02), `AUDIT_FO4` (D2-03), `AUDIT_STARFIELD` (D9-01). Filed once.
**Severity**: LOW

## Description

`record_external_texture_sources` (`byroredux/src/asset_provider/material.rs:1053`) hand-lists all 22 canonical `MaterialTextureSet` roles with **no exhaustiveness guard**. It is the fifth hand-written role walk in the codebase and the only one still unguarded — its three siblings were given lockstep guards by #3349, #3734 and #2697.

## Evidence

The four guarded walks use the `map_ref` / `roles()` / `values()` cross-check idiom introduced precisely to prevent this drift class. This one does not: adding a 23rd role compiles clean and silently omits it here.

Independently rediscovered by three separate audits in one suite run, which is itself evidence the omission is visible from several directions.

## Impact

Diagnostics-only blast radius today — a missed role yields wrong provenance in `mat.dump` and `tex.missing`, not wrong rendering. The walk is **complete and correct as of this audit**; the defect is that nothing keeps it that way. Given the role set has grown 18 → 22 since July, further growth is likely.

## Suggested Fix

Apply the same `map_ref`-based exhaustiveness cross-check the three sibling walks use (#3349 / #3734 / #2697 are the templates), so a new role fails the build here rather than silently dropping out of provenance reporting.

## Completeness Checks
- [ ] **SIBLING**: All five role walks confirmed guarded after the change
- [ ] **TESTS**: A test fails if a role is added to `MaterialTextureSet` without appearing in this walk

## Related
- #3349, #3734, #2697 (the three siblings already guarded)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite — merges NIFAL-2026-09-05-D8-02, FO4-2026-09-05-D2-03, SF-2026-09-05-D9-01.
