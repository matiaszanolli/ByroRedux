# #3905: NIFAL-2026-09-05-D1-01: #3639's neutral-roughness fallback gates on an authored path while the shader escape gates on a resolved bindless index

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3905 --json state`).*

---

**Audit**: `docs/audits/AUDIT_NIFAL_2026-09-05.md` · **Severity**: LOW · **Dimension**: 1 (Material)

## Description

#3639's neutral-roughness fallback is gated on an authored **path**, while the shader escape it exists to restore is gated on a resolved **bindless index**:

- boundary decides on `textures.smooth_spec.is_none()` — was a gloss map *authored*
- shader acts on `glossMapIndex != 0u` — did a gloss map *resolve*

An authored-but-unresolvable gloss map (missing from the archive, failed load) satisfies the first and fails the second, so it stays pinned at the 0.04 floor instead of taking the neutral fallback the fix was written to provide.

## Impact

A material with a broken gloss-map reference renders with a wrong (over-dark) roughness rather than the intended neutral. Population size is **unknown** — this audit did not measure how many authored gloss paths fail to resolve, and is deliberately not estimating it.

Defence-in-depth: the boundary decides from a weaker signal than the consumer acts on.

## Suggested Fix

Gate the fallback on the resolved state rather than the authored path, so boundary and shader agree on the same predicate. The structural home for this already exists and already exempts exactly this population.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The decision stays at the NIFAL boundary, not re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test covers an authored-but-unresolvable gloss map

## Related
- #3639 (CLOSED by `1ff5fae4`)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
