# #5381: SF-2026-10-08-D6-01: CDB slot 6 (`_height`) lands in the canonical `height` role, which the renderer consumes as a parallax-occlusion map, so Starfield materials get an unauthored POM ray-march at the engine default scale

**Labels**: high,nifal,import-pipeline,bug,game:starfield,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5381

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-08.md` — `SF-2026-10-08-D6-01` (HEAD `00f580e09`)

- **Severity**: HIGH (the `_audit-severity.md` NIFAL row: a wrong Material out of the NIFAL boundary)
- **Dimension**: Material Flow (NIFAL boundary) / CDB Material Database
- **Location**: `byroredux/src/asset_provider/material/merge.rs:265-271` (`SLOT_HEIGHT => fill(&mut material.textures.height, …)` in `apply_cdb_material`); consumer `byroredux/src/render/static_meshes.rs:657` (`parallax_map_index = texture_indices.height`); `crates/renderer/shaders/triangle.frag:292-305`; defaults `crates/core/src/ecs/components/material.rs:17-21`
- **Status**: NEW. It arrived with `224a19372` / `18fce7e43` (#3398 Phase 2). A search of open and closed issues and of the 10-03 to 10-08 reports found no mention.
- **Description**: `apply_cdb_material` fills the canonical `height` role from `MRTextureFile` slot 6. The render path binds `height` to `parallaxMapIndex`, and `triangle.frag` runs parallax-occlusion displacement whenever that index is non-zero. The CDB arm authors no parallax scale or step count, so POM runs at `DEFAULT_PARALLAX_HEIGHT_SCALE = 0.04` and `DEFAULT_PARALLAX_MAX_PASSES = 4`.
  - Starfield does not use slot 6 as a POM input. In the vanilla CDB's 97-class schema, the only parallax-occlusion fields are on `BSMaterial::ProjectedDecalSettings`: `UseParallaxOcclusionMapping`, `SurfaceHeightMap: BSMaterial::TextureFile` (a nested field, not an `MRTextureFile` slot), `ParallaxOcclusionScale`, `ParallaxOcclusionShadows` and `MaxParralaxOcclusionSteps`. That is, decals only.
  - Height is consumed elsewhere by `BSMaterial::AlphaBlenderSettings` (`HeightBlendThreshold`, `HeightBlendFactor`; layer height-blending) and `BSMaterial::TerrainSettingsComponent` (`MaxDisplacement`, `DisplacementMidpoint`).
  - Ordinary layered materials have no POM toggle or scale.
- **Evidence**:
  - A read-only CLAS dump of `Starfield - Materials.ba2:materials\materialsbeta.cdb` (field lists quoted above).
  - The base CDB carries 1,521 `*_height.dds` FileName strings (394 unique), against 24,827 unique `*_color.dds`.
  - Sampled height maps are layer-blend landscape and architecture sets: `SnowScalloped01_height`, `DirtForerstRoots01_height`, `CaveRoughMacro01_height`, `NAStone01Mossy01_height`, `NATechPatternConcrete02_height`.
  - The synthetic CDB fixture carries slots 0/1/3 only (`index.rs` `synthetic_cdb_chunks`), so no test exercises the slot-6 route.
- **Impact**:
  - POM displaces `sampleUV` before every later fetch (base, normal, detail, emissive…). Each affected Starfield surface swims and distorts with view angle, from a height field authored for blending layers, not for displacing UVs.
  - This hits the landscape, cave and New Atlantis architecture materials that carry height maps.
  - It is a fabricated shading effect at the single translation boundary, the same near-miss class nifal.md forbids for the five parked kinds ("the near-miss roles are wrong, not merely imperfect").
- **Related**: #3398 (Phase 2), #4429 (the parked-kinds contract), #5283 (sibling: the CDB emissive role is zero-weighted), `docs/engine/nifal.md` § Starfield single-channel kinds.
- **Suggested Fix**:
  - Stop forwarding `SLOT_HEIGHT` into `height`. Park it with the single-channel kinds until a Starfield layer height-blend consumer exists.
  - If decal POM is wanted, translate it from `ProjectedDecalSettings` (toggle + scale + steps + `SurfaceHeightMap`).
  - Add a slot-6 fixture that pins the decision, and fix `apply_cdb_material`'s "6=height" doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CDB slots routed by `apply_cdb_material`; the parked single-channel kinds)
- [ ] **CANONICAL-BOUNDARY**: The fix stays at the NIFAL parser→`Material` boundary (`apply_cdb_material` / `translate_material`) — never pushed into `triangle.frag` or re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (synthetic CDB fixture with a slot-6 texture)
