# MAT-D3-02: Three exterior draw populations never reach translate_material — LAND terrain, terrain LOD, and object LOD carry no Material at all

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2444
**Finding ID**: MAT-D3-02 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 3 — Material translation boundary (NIFAL reference slice)
**Location**: `byroredux/src/cell_loader/terrain.rs:589-624`; `terrain_lod.rs:672-684`; `object_lod.rs:319-333`; consumed at `render/static_meshes.rs:323-338`
**Status**: NEW (adjacent to open #2371, which doesn't mention the missing `Material`)

## Description
All three spawners insert `Transform`/`GlobalTransform`/`MeshHandle`/`TextureHandle`/`RenderLayer` but never a `Material`, so their draws fall into `static_meshes.rs`'s `else` arm and get an 11-tuple of hardcoded literals (`roughness 0.5`, `metalness 0.0`, etc.) — a second materialization site living in the render path, outside the documented single source of truth.

## Evidence
Confirmed directly: zero `Material` component insertions in `terrain.rs`, `terrain_lod.rs`, or `object_lod.rs`; `render/static_meshes.rs:325-336` hardcodes the fallback tuple `(0.5, 0.0, DEFAULT_DIELECTRIC_IOR, 0.0, [0.0; 3], 1.0, [1.0; 3], [1.0; 3], [1.0; 3], 0.0, 0u32)` in the `else` (no-`Material`) arm.

## Impact
(a) Exterior landscape shades with a markedly tighter/brighter GGX lobe than the stone/dirt statics on it (0.5 vs the classifier's 0.85), a visible mismatch at every ground-meets-architecture seam. (b) Object LOD imposters carry roughness 0.5 while the full models they swap to carry the classifier value — a shading pop on top of the geometric LOD pop. (c) The NIFAL invariant "every drawn surface's canonical material is produced at one boundary" is false for the entire outdoors.

## Related
#2371 (OPEN, EX-10/11 — adjacent scope, doesn't call out the missing `Material` specifically).

## Suggested Fix
Give the three spawners a canonical `Material` — for LAND, feed the resolved base-layer texture path through `Material{..}` + `resolve_pbr()` (reuses the existing classifier); for object LOD, carry the source record's material through the imposter. If a flat default is deliberately preferred for LOD, insert an explicit `Material::default()` component so it's owned and visible to `mat.*` tooling.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New `Material` construction routes through `translate_material`/`resolve_pbr()`, not a fourth ad hoc materialization site
- [ ] **TESTS**: A regression test confirms terrain/LOD entities carry a real `Material` component after the fix
