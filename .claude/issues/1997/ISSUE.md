# REN-D15-01: Procedural water-normal fallback hashes absolute world coords — bands on distant exterior water, and it IS the default path

**Filed**: 2026-07-15 · **Source**: `docs/audits/AUDIT_RENDERER_2026-07-15_DIM15.md` (Dimension 15: Water) · **Labels**: `medium,renderer,bug`

## Description

`sampleScrollingNormal`'s procedural branch (taken when `normalMapIndex == 0xFFFFFFFFu`) carries a `#1502` precision-bound comment noting it feeds absolute-world-XZ coordinates into `hash21`, which loses precision and visibly bands past a documented threshold — with the caveat "if procedural foam/noise ever becomes a default path, feed the hash render-origin-relative coordinates." That condition is already met today:

- `WaterMaterial::default()` sets `normal_map_index: u32::MAX`.
- `resolve_water_material` only overrides that default when a cell has a resolvable `XCWT` water-type ref (Skyrim-only field — FNV/FO3/Oblivion cells never populate it) and the referenced WATR record has a non-empty `texture_path`.
- A WATR record with an empty `texture_path` (e.g. a lava pool — confirmed by the existing `resolve_water_material_transfers_reflection_color` test's own `LavaPool01` fixture) also falls through to the procedural default.

So the procedural path is the default for a large class of real content, not an edge case.

## Evidence

```rust
// crates/core/src/ecs/components/water.rs — WaterMaterial::default()
normal_map_index: u32::MAX,
```

```glsl
// crates/renderer/shaders/water.frag
if (normalMapIndex == 0xFFFFFFFFu) {
    // PRECISION BOUND (#1502): uvBase here is absolute world XZ, never rebased
    vec2 uv = uvBase * scale + scroll * time;
}
```

## Impact

Visual-only banding on distant exterior water lacking a bound normal map. Reachable at real magnitudes (Skyrim Tamriel ±233k units, FNV Mojave far cells at `grid*4096`), both past the ~176k-unit band-onset. No crash, no CPU-side corruption.

## Related

- #1502 — origin of the precision-bound comment, never actually wired to a coordinate rebase.

## Suggested Fix

Rebase the procedural branch's input to `vWorldPos.xz - renderOrigin.xz` before hashing, matching the render-origin-relative convention used elsewhere. Fix the stale "never a default path" comment regardless.

## Completeness Checks
- [ ] **SIBLING**: Check other procedural/noise-driven shading inputs for the same absolute-coordinate assumption
- [ ] **TESTS**: Consider a CPU-side test pinning `resolve_water_material`'s procedural-vs-textured classification

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1997
