# SPT-D2-2026-08-16-01: the .spt tag-4003 leaf-texture fallback is unresolvable on 100% of vanilla content

**Issue**: #3077
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md` (Dimension 2 — TLV walk).

**Location**: `crates/spt/src/import/mod.rs`:129-137

## Description

The `.spt` tag-4003 leaf-texture fallback is **unresolvable on 100% of vanilla content** — the path it constructs matches nothing in any shipped archive.

## Impact

The fallback exists to give a placeholder billboard a leaf texture when the primary lookup fails. Since it never resolves on vanilla data, the billboard renders untextured (or with the checker placeholder) in exactly the cases the fallback was written for.

Combined with #3076 (billboards never face the camera), the `.spt` placeholder path is doubly degraded: wrong orientation and, on the fallback branch, wrong texture.

## Suggested Fix

Measure what tag-4003 actually references on vanilla `.spt` files before changing the path construction — the current form was evidently derived from an assumption rather than from shipped data. If no resolvable texture exists, remove the fallback and record why, rather than leaving a branch that cannot succeed.

**Do not guess a path transform** — extract the real strings from the corpus first.

## Related

- #3076 (SPT-D3-01), #3078 (SPT-D1-01) — the other two live `.spt` placeholder defects

## Completeness Checks
- [ ] **NO-GUESSING**: The corrected path comes from measured vanilla `.spt` content
- [ ] **NO-DEAD-BRANCH**: If unresolvable by nature, the fallback is removed with a recorded reason
- [ ] **NOT-SILENT**: A failed leaf-texture resolve is observable (`tex.missing` or a counter)
- [ ] **TESTS**: A regression test resolves a real vanilla `.spt` leaf texture

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3077 --json state` when live state is needed.*
