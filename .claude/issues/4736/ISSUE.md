# EXT-D6-2026-09-21-01: FO76 .bto census in object_lod.rs/lod_bands.rs is stale against the installed archives (3,056 .bto over two BA2s, incl. 590 level-8 quads)

**Issue**: #4736
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (doc and census; behaviour is availability-driven and correct)
**Dimension**: Distant LOD and trees
**Tier Violated**: no-fabrication (a data claim in code that the data contradicts)
**Game Affected**: FO76
**Location**: `byroredux/src/cell_loader/object_lod.rs:658-662` (scheme comment), `:1138-1142` (test message); `byroredux/src/cell_loader/lod_bands.rs:182-191` (ladder comment)

## Description
The code says FO76 ships 1,007 `.bto` in `GeneratedMeshes01` (L4/L16/L32, "no level 8"), with the missing level-8 band riding the #3502 coarsen-to-available escape. The FO76 archives were rewritten 2026-09-20; a name-table census today finds the `.bto` family split across two archives (`GeneratedMeshes01.ba2`: 1,001 files L4/L16/L32; `GeneratedMeshes02.ba2`: 2,055 files L4/L8) — 3,056 total including 590 level-8 quads. Behaviour is fine (selection is availability-driven and the two-digit series auto-loads), but the code comments' premise is wrong, and the census method (checking only the `…01` archive) repeats the #3321 shape.

## Evidence
BA2 name-table lister run over both archives; archive mtimes 2026-09-20.

## Impact
A future FO76 ladder or refine tune would be justified by a missing level-8 band that actually exists.

## Suggested Fix
Replace the three comments with the two-archive census and drop the "no level 8" reasoning. Update the `archive.rs` sibling doc for two-digit series (`open_with_numeric_siblings`).

## Related
#4488 (closed), #3502, #3321

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D6-2026-09-21-01)
