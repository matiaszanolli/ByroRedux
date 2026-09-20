# EXT-D5-2026-09-19-04: LOD render-only guard is a whitespace-sensitive source-substring scan

- **ID**: EXT-D5-2026-09-19-04
- **Labels**: low,water,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4500

**Severity**: LOW (guard quality; underlying behavior verified sound) · **Dimension**: WATAL · **Game Affected**: FO3+ (NAM3/NAM4 eras)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D5-2026-09-19-04)

**Location**: `byroredux/src/cell_loader/water.rs:1200-1213`

**Description**
The guard `lod_water_is_render_only_and_cannot_create_false_submersion` works by `find`ing the function text between two markers and asserting the body does not contain the exact string `"world.insert(\n        entity,\n        WaterVolume"`. Any reformat (different indentation, single-line insert) or an insert through a helper (`world.insert(entity, make_volume())`) silently voids it while the code regresses. The behavior itself is verified correct independently: `submersion_system` gates on the `WaterPlane`+`WaterVolume` pair (`systems/water.rs:154/164`, #2792), `spawn_lod_water_plane` attaches no `WaterVolume`, and physics contacts arise only from `WaterVolume` overlap.

**Impact**
Guard-only; current behavior correct.

**Suggested Fix**
Replace with a structural assert (scan the body for `WaterVolume`/`WaterCurrentVolume` tokens via `world.insert` argument extraction), or — when a headless spawn harness exists — spawn and assert `world.get::<WaterVolume>(lod_entity).is_none()`.

## Completeness Checks
- [ ] **TESTS**: The replacement is the test
