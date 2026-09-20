# EXT-D2-2026-09-19-04: splat packer 8-lane bound and the terrain tangent constant are unpinned

- **ID**: EXT-D2-2026-09-19-04
- **Labels**: low,terrain-exterior,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4496

**Severity**: LOW (test-gap) · **Dimension**: Terrain/splatting · **Game Affected**: all LAND games
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D2-2026-09-19-04)

**Location**: `byroredux/src/cell_loader/terrain.rs:848-855` (packer, `splat1[i - 4]`), `:173-184,415-433` (cap arithmetic); `crates/renderer/src/vertex.rs:146-168` (terrain tangent)

**Description**
(a) `splat1[i - 4] = w` panics for `i ≥ 8`; the only protection is arithmetic spread across two functions (`base_transitions ≤ 4` from a 4-quadrant BTreeMap + `authored_budget = 8 - base_transitions.len()` truncation). Traced airtight at HEAD, but no `debug_assert!(layers.len() <= 8)` and no end-to-end pin of `build_cell_splat_layers`' output length exists — a future "fifth base transition" or budget edit panics every exterior cell. (b) No test pins `new_terrain`'s synthetic tangent constant `[1,0,0,-1]` — the one value selecting Path-1 TBN for LAND normal splats — while its field doc says the opposite; #2822 corrected it once with no regression pin.

**Suggested Fix**
`debug_assert!(splat_layers.layers.len() <= 8, …)` at the packer, and a one-line test asserting `Vertex::new_terrain(…)` tangent == `[1.0, 0.0, 0.0, -1.0]` with a #2822 reference.

## Completeness Checks
- [ ] **TESTS**: Both pins are themselves the tests
