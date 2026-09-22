# ESM-2026-09-21-D2-03: LTEX.GNAM is an array of grasses but decoded into a one-grass-per-LTEX map (last wins)

**Issue**: #4642
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Sub-Record Byte Accounting / CELL-WRLD support data
**Location**: `crates/plugin/src/esm/cell/support.rs:506-516` (`ltex_to_grass.insert`), `crates/plugin/src/esm/cell/mod.rs:1209` (`landscape_grasses: HashMap<u32, u32>`), `docs/engine/plugin-loading.md:251`

## Description
xEdit defines LTEX grasses as an array (`wbRArrayS('Grasses', wbFormIDCk(GNAM, 'Grass', [GRAS]))`). Redux's decoder overwrites on each `GNAM`, keeping only the last grass.

## Evidence
GNAM count per grass-bearing LTEX: Oblivion 10/43/18/4 (1/2/3/4 grasses), FO3 2/–/7/–, FNV 2/4/14/–, Skyrim 5/11/4/–, FO4 6/26/16/4. 151 of 184 grass-bearing LTEX records lose 1-3 grasses to last-wins overwrite.

## Impact
Latent today. Feeds `authored_grass: [Option<u32>; 8]` (`byroredux/src/cell_loader/terrain.rs:347-358`) for the authored-card ground-cover tier (#4413, open). The data model can't represent multi-species landscape textures — every multi-grass LTEX would spawn a single species once that consumer lands.

## Suggested Fix
Change to `landscape_grasses: HashMap<u32, Vec<u32>>` in authored order; carry the per-lane list through `authored_grass_for_splat_layers`; fix the doc row.

## Related
#4413 (open, consumer tier), #3807

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D2-03)
