# EXT-D6-2026-09-19-06: coarse-band boundary safety unpinned for the FO3/FNV legacy ladder at radius 7

- **ID**: EXT-D6-2026-09-19-06
- **Labels**: low,terrain-exterior,bug,test-gap,game:fo3,game:fnv
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4505

**Severity**: LOW · **Dimension**: Distant LOD (guard coverage) · **Game Affected**: FO3, FNV
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D6-2026-09-19-06)

**Location**: `byroredux/src/cell_loader/terrain_lod.rs:1179-1203` (`coarse_terrain_bands_never_reach_the_full_detail_region` — game list `[Skyrim, Fallout4]` at radius 6); `lod_bands.rs:122` (`FALLOUT_LEGACY_REFINE_BU`)

**Description**
Coarse bands draw with hole mask 0 (`terrain_lod.rs:415-419`), so their never-overlap guarantee rests entirely on band thresholds. For the legacy ladder (refine(8)=12), an emitted, resident level-8 quad persists down to nearest-cell distance 8 — exactly `radius_unload` when `--radius 7`. Tracing the hysteresis geometry shows no reachable overlap (a cell only becomes full-detail-resident at distance 8 by the player retreating, which simultaneously drops the quad's `d_min` to ≤ 7 where the sticky rules force subdivision). Correct — by **one cell of margin** — and no test pins it for the legacy ladder; the existing reach guard covers Skyrim/FO4 only.

**Impact**
A future threshold retune (e.g. raising `FALLOUT_LEGACY_REFINE_BU[0]`) could silently put baked/synth coarse geometry over hysteresis-band full-detail cells on FO3/FNV with no failing guard. `lod_coverage::find_terrain_full_detail_overlaps` is the runtime backstop.

**Suggested Fix**
Extend the reach guard to `GameKind::Fallout3NV` with `radius_unload` swept to 8, mirroring the Skyrim/FO4 loop.

## Completeness Checks
- [ ] **TESTS**: The extended guard is the fix
