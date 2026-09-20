# EXT-D2-2026-09-19-03: three stale docs contradict deliberate terrain behavior (VNML signedness, tile stride, terrain tangent)

- **ID**: EXT-D2-2026-09-19-03
- **Labels**: low,terrain-exterior,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4495

**Severity**: LOW (doc rot) · **Dimension**: Terrain/splatting · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D2-2026-09-19-03)

**Location**: `crates/plugin/src/esm/cell/mod.rs:180-182`; `crates/renderer/shaders/include/bindings.glsl:473-475`; `crates/renderer/src/vertex.rs:56-58` (field doc) vs `:178` (`new_terrain`)

**Description**
Three stale docs contradict deliberate, correct code: (1) `LandscapeData.normals` still teaches the pre-#4059 *unsigned* VNML decode that `terrain.rs::decode_vnml_normal` exists to correct (signed `i8`/127). (2) `bindings.glsl`'s `GpuTerrainTile` comment says stride 144 — stale since the Tier-3 atlas tail; the real 160 is what the guard test and shipped SPIR-V pin. (3) `Vertex.tangent`'s doc says terrain is zero-tangent while `new_terrain` deliberately emits `[1,0,0,-1]` (#2474: zero tangent forced LAND fragments down the derivative-fallback path → faceting; #2822 fixed the bitangent sign). Item 3 is more than cosmetic: an auditor "restoring" zero per the doc would silently flip every terrain normal map's green channel back to the pre-#2822 wrong orientation.

**Related**: #4059, #4056/#4057, #2474, #2822

**Suggested Fix**
Three one-line doc corrections; for the tangent, state the deliberate `[1,0,0,-1]` choice in the field doc with #2474/#2822 references.

## Completeness Checks
- [ ] **TESTS**: Pair with the tangent-constant pin (EXT-D2-2026-09-19-04)
