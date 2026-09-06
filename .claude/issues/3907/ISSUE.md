# #3907: SF-2026-09-05-D8-02: docs/engine/nifal.md says the two BGEM glass roles aren't routed, contradicting its own role count and the shipped code

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3907 --json state`).*

---

**Audit**: `docs/audits/AUDIT_STARFIELD_2026-09-05.md` · **Severity**: LOW · **Dimension**: 8

## Description

`docs/engine/nifal.md:522` states the two BGEM glass roles are **not** routed into `MaterialTextureSet`. This contradicts:

1. its own role count 33 lines earlier at `:489`, and
2. the shipped code, which does route them (#2109, CLOSED).

## Impact

Documentation drift in the NIFAL spec — the document an implementer consults before touching the material boundary. A reader trusting `:522` would re-implement routing that already exists, or treat the existing routing as a bug.

## Suggested Fix

Delete or correct the `:522` claim to match `:489` and the shipped behaviour.

## Completeness Checks
- [ ] **SIBLING**: The rest of `nifal.md` checked for other claims predating #2109

## Related
- #2109 (CLOSED — routed the BGEM glass roles)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
