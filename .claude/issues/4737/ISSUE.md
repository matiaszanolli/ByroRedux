# EXT-D6-2026-09-21-02: probe_lod_corpus opens BA2s (#4502) but still counts none of the LOD family BA2 games ship

**Issue**: #4737
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (tooling)
**Dimension**: Distant LOD and trees
**Tier Violated**: no-fabrication (the anti-fabrication tool reports a false zero)
**Game Affected**: Skyrim, FO4, FO76
**Location**: `crates/bsa/examples/probe_lod_corpus.rs:55-118` (matchers at `:78-88`)

## Description
`probe_lod_corpus` recognises only `_far.nif`, `distantlod\` and `landscape\lod\`. The Creation family used by Skyrim/FO4/FO76 — `meshes\terrain\<ws>\<ws>.<L>.<x>.<y>.btr` and `meshes\terrain\<ws>\objects\*.bto` — is never matched, so the tool prints zero for exactly the games #4502 was filed to fix. Confirmed unchanged at HEAD `ee6d3fb39`.

## Evidence
Ran the tool on both FO76 `GeneratedMeshes` archives and `Fallout4 - Meshes.ba2`: each prints all-zero totals. Actual contents: FO76 3,056 `.bto`; `Fallout4 - Meshes.ba2` 6,205 `.btr` + 497 `.bto`; FO4 total (incl. DLCCoast/DLCNukaWorld) 8,271 `.btr` + 802 `.bto`.

## Impact
Anyone following the "census before touching `object_lod_scheme`" workflow gets a false "none" for exactly the games #4502 targeted.

## Suggested Fix
Add a fourth matcher for the `meshes\terrain\` family (`<ws>.<L>.<x>.<y>.btr` and `objects\*.bto`), per worldspace and per level. Normalise `/` to `\`.

## Related
#4502 (closed; outcome persists), #3321

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D6-2026-09-21-02)
