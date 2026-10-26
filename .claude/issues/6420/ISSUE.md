# EXT-D5-2026-10-26-03: CPU shore direction diverges from the mesh's baked direction near cell borders; the parity test pins depth only

Labels: low,water,physics,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW (static trace, not confirmed on screen)
- **Dimension**: Water translation (WATAL) / physics seam
- **Tier Violated**: n/a (CPU/GPU coherence)
- **Game Affected**: exterior cell water planes over LAND (all games with LAND water)
- **Location**: `byroredux/src/cell_loader/water.rs:244-262` (bake) vs `:621-629` and `crates/core/.../water/bed_field.rs:77-107` (physics sample); test `the_physics_bed_field_agrees_with_the_baked_vertices` (`:2043-2062`)
- **Status**: NEW
- **Description**: `53df201ea` zeroes `uphill` on the mesh's border vertices so neighbouring cells do not disagree and crack. The physics `WaterBedField` is built from the same function but keeps the full bilinear gradient at every point, including the border strip. The GPU also interpolates per-vertex forward-difference gradients across triangles, while the CPU evaluates the in-cell bilinear gradient. So in the outermost 128-unit strip of a cell, `water.vert` refracts the swell and the ripple pair by a fading or zero shore direction, while `authored_wave_height_over_bed` refracts them fully. The module doc claims the visible and buoyancy crest "cannot disagree about how deep the water is". That holds for depth, and the test only asserts depth (`sample.depth` against the baked `r` channel). Shoreward direction is unchecked.
- **Evidence**: Test body compares only `vertex.color[0] * WATER_BED_DEPTH_MAX`. `bed_vertex_color` and the border zeroing are in the mesh bake path only.
- **Impact**: Buoyancy, camera submersion and the player see a slightly different crest phase from the rendered one within shallow border strips (swell amplitude up to 28 BU, shallow-capped). Small and local, since refraction only applies where the bed is shallower than 900 BU.
- **Related**: `53df201ea`, `7dd294ee3`.
- **Suggested Fix**: Either make `WaterBedField::sample` take an `edge_zero` flag mirroring the bake, or add a test that compares `uphill` as well and document the border case. The first is preferred because the mesh and CPU then share one rule.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
