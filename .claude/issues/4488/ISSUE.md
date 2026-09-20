# EXT-D6-2026-09-19-02: FO76 ships 1,007 .bto object-LOD files the scheme table calls "none"

- **ID**: EXT-D6-2026-09-19-02
- **Labels**: medium,terrain-exterior,bug,game:fo76
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4488

**Severity**: MEDIUM · **Dimension**: Distant LOD · **Tier Violated**: no-fabrication (inverse: assets exist, scheme claims none) · **Game Affected**: FO76
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D6-2026-09-19-02)

**Location**: `byroredux/src/cell_loader/object_lod.rs:653-662` (`object_lod_scheme` → `_ => None`); corpus: `SeventySix - GeneratedMeshes01.ba2`

**Description**
The table's own comment says "FO76/Starfield: not yet exercised — add an arm with archive evidence rather than by lineage". The archive evidence now exists: **1007** `meshes\terrain\appalachia\objects\appalachia.<L>.<x>.<y>.bto` (level4 ×795 / level16 ×164 / level32 ×48; **no level 8**) — the exact `BakedBto` naming family Skyrim/FO4 use. FO76 distant objects therefore never render from baked LOD: the #3321 false-premise shape, one game over. The skipped level-8 band would exercise the #3502 coarsen path on a mixed 4/16/32 ladder if wired; `LodBandLadder::for_game(FO76) = None` and the `!combined_lod_supported(FO76)` pin (`lod_support.rs:398`) need a joint decision.

**Impact**
FO76 horizons lack all distant objects from baked LOD. Feature gap, not a crash.

**Related**: #3321 (FNV precedent, closed); #3502 (coarsen escape — implemented, would be exercised)

**Suggested Fix**
Decide the ladder+scheme shape (FO4-style `BakedBto` objects + explicit ladder decision), then add the arm with this census as the archive evidence. Starfield "none" re-verified correct (0 `.bto`/`.btr` in `LODMeshes.ba2`).

## Completeness Checks
- [ ] **SIBLING**: Revisit `combined_lod_supported(FO76)` + the lod_support pin in the same change
- [ ] **TESTS**: Scheme⇒ladder coherence test extended to the new arm; corpus counts pinned
