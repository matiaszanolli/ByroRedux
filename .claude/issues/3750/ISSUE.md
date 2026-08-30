# #3750: SPT-2026-08-30-D3-02: the import cache is keyed by model path but SptImportParams is per-TREE-record, so records sharing one .spt all inherit the first one's size and texture

**Labels**: bug, low, game:oblivion, terrain-exterior, speedtree
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-30.md` · **Severity**: LOW · **Dimension**: 3 (TREE→Billboard Wiring)
**Game affected**: Oblivion (vanilla yield nil), all `.spt` games for mod content

## Location
- `byroredux/src/cell_loader/references/synth_child.rs` — `cache_key = canonical_model_path_key(&stat.model_path)`
- `byroredux/src/cell_loader/nif_import_registry.rs` — `canonical_model_path_key`
- `byroredux/src/cell_loader/references/import.rs`

## Description
`parse_and_import_spt` bakes the TREE record's ICON, OBND, MODB and BNAM into the returned `CachedNifImport`, but that value is stored under `canonical_model_path_key(&stat.model_path)` — **the model path alone**. It runs once per unique `.spt`, on whichever TREE record is placed first. Every other TREE record pointing at the same `.spt` silently receives the first record's leaf texture and billboard size.

For NIF content the cache key is sound because the parse depends only on the file; for `.spt` the import depends on **per-record metadata that the key does not capture**.

## Evidence
Vanilla Oblivion has 142 `.spt` TREE records across 139 unique MODL values — exactly three collisions, and all three diverge:

```
\ShrubVineMapleSU.spt        0x232db ShrubVineMapleSU        MODB  531.13  BNAM  440.00 x  440.00
                             0x017fc TestToddTree03          MODB  466.66  BNAM  550.36 x  368.66
\TreeSilverBirchForestSU.spt 0x009fe TreeSilverBirchForestSU MODB 1931.37  BNAM 1600.00 x 1600.00
                             0x00898 TestToddTree            MODB 2182.98  BNAM 1576.07 x 1808.44
\TreeSugarMapleForestSU.spt  0x0089b TreeSugarMapleForestSU  MODB 1714.09  BNAM 1420.00 x 1420.00
                             0x00899 TestToddTree02          MODB 2139.71  BNAM 1600.14 x 1772.59
```

ICON is identical within each pair, so only sizing diverges. FNV (3/3) and FO3 (9/9) have no collisions at all.

## Impact
**Vanilla yield is effectively nil** — all three second records are `TestToddTree*` developer stubs.

The mechanism matters for mod content: the documented rationale for ICON-winning-over-tag-4003 is *"mods retexture trees by re-pointing TREE ICON without rewriting the `.spt`"* (`crates/spt/src/import/mod.rs`), which is precisely the N-records-one-file shape this cache collapses. A retexture mod would see one of its variants take over the others' leaves.

## Related
The D3-01 MODL-resolution issue filed alongside this one (both live on the `extract_mesh`/cache seam); #3038 (the last cache-key normalisation fix); #523.

## Suggested Fix
For the `.spt` branch only, extend the cache key with the TREE form id (or with a hash of the four consumed fields) so records that differ get distinct entries while identical records still share one parse. **Leave the NIF key alone.**

## Completeness Checks
- [ ] **SIBLING**: #3038 normalised this key once already — confirm the `.spt` extension does not reintroduce the case/separator drift it fixed
- [ ] **TESTS**: a regression test that two TREE records sharing one `.spt` but differing in ICON/BNAM produce distinct cache entries
