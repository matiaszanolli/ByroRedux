# FO3-D4-02: FO3/FNV baked terrain-LOD tree unconsumed; baked_lod_supported's doc denies it exists

**Issue**: #3100
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO3_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO3_2026-08-16.md` (Dimension 4 — terrain LOD).

**Location**: `byroredux/src/cell_loader/lod_support.rs`:35-52 (`baked_lod_supported` + its doc comment) · `byroredux/src/cell_loader/terrain_lod.rs`:57-94 (`LOD_TEXTURE_QUAD_CELLS`, `lod_quad_texture_path`), :752-790 (the two-tier texture priority)

## Description

FO3/FNV ship a **complete baked terrain-LOD asset tree that no code path consumes**, and `baked_lod_supported`'s doc **asserts it doesn't exist**.

## Impact

Distant terrain on FO3/FNV is generated rather than using the shipped baked LOD, so it diverges from what the games actually look like at distance — and the shipped assets are simply unused.

The doc comment is the compounding half: it records the absence as a fact about the games, so a reader concludes there is nothing to consume. That is the same false-premise-in-a-comment shape as #3037 (the FNV female-body inference) and #3068 (the Skyrim glow-bit claim) — three instances this sweep of a wrong justification preventing a fix.

Distant-object LOD is also flagged in `_audit-common.md`'s EXAL notes as *"the big gap + user must-have"*.

## Suggested Fix

Correct the `baked_lod_supported` doc comment first — it is the load-bearing false claim. Then wire the FO3/FNV baked LOD tree through the existing `lod_quad_texture_path` / two-tier priority machinery, which already exists for other games.

Measure what the tree actually contains before wiring — do not assume it mirrors another game's layout.

## Related

- #3037, #3068 (the same false-premise-in-a-comment shape this sweep)
- `docs/engine/exal.md` (EXAL — the exterior layer that owns distant LOD)

## Completeness Checks
- [ ] **FALSE-PREMISE**: The `baked_lod_supported` doc claim is corrected so it cannot be re-derived
- [ ] **NO-GUESSING**: The FO3/FNV LOD tree layout is measured, not assumed
- [ ] **SIBLING**: Oblivion checked for the same unused baked tree
- [ ] **EXAL-BOUNDARY**: Wiring goes through `env_translate.rs`, not around it
- [ ] **TESTS**: A regression test asserts a baked LOD texture resolves on FNV

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3100 --json state` when live state is needed.*
