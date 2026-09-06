# #3899: FO4-2026-09-05-D2-02: peek_magic re-extracts and re-inflates the whole material file on every merge, bypassing both material caches

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3899 --json state`).*

---

**Audit**: `docs/audits/AUDIT_FO4_2026-09-05.md` (suite preset `texture-roles-deep`)
**Severity**: MEDIUM · **Dimension**: 2 (BGSM/BGEM)

## Description

`peek_magic` performs a full archive extract **plus zlib inflate** of the whole material file on every merge, purely to read its magic bytes and decide whether the file is a BGSM or a BGEM. It bypasses `bgsm_cache`, `bgem_cache` and `failed_paths` entirely, so a material already parsed and cached is still re-extracted and re-inflated on each subsequent reference.

## Evidence

`byroredux/src/asset_provider/material.rs` — `peek_magic` calls the archive extract path directly rather than consulting the two material caches or the negative `failed_paths` set that every other consumer in the file checks first.

Because the extract takes the archive's file mutex, this also serialises material resolution on the streaming path, where many REFRs reference the same handful of shared materials.

## Impact

Redundant CPU work and I/O proportional to REFR count rather than distinct-material count, on the cell-streaming hot path — precisely where frame-time spikes are user-visible as hitching during cell transitions. Not a correctness defect: the result is right, it is just recomputed.

## Suggested Fix

Consult `bgsm_cache` / `bgem_cache` / `failed_paths` before extracting; on a cache hit the magic is already implied by which cache answered. If a peek is genuinely needed for an uncached path, cache the magic (or the decoded material) so the second reference is free.

## Completeness Checks
- [ ] **LOCK_ORDER**: If the archive mutex scope changes, ordering against the material caches is preserved
- [ ] **SIBLING**: Other direct `extract` callers in `material.rs` checked for the same cache bypass
- [ ] **TESTS**: A regression test pins that a repeated material reference does not re-extract

## Related
- #3659 (`Ba2Archive::extract` holds its `Mutex<File>` across inflate — adjacent contention on the same mutex, distinct cause; fixing both compounds)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
