# REN-D19-04: perturbNormal Path 1 can produce NaN when tangent is parallel to normal

Labels: low, renderer, bug

## Description

The Path-1 gate proves the incoming tangent is non-zero, not that it is non-parallel to `N`; when `T ∥ N` the projection is the zero vector and `normalize()` on it is undefined (0/0 → NaN), propagating through `mat3(T,B,N)` into the shaded normal, the `octEncode(N)` G-buffer write, and every RT ray origin built from it. All three sibling TBN builders in the same tree guard the post-projection length (`parallaxDisplaceUV`, `getRayHitTangentFrame`, and Path 2 by construction), so this is a local omission, not house style. Known producer is REN-D19-03; the guard is what keeps a future importer regression from becoming NaN pixels.

## Location

`crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormal` Path 1)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D19-04).

https://github.com/matiaszanolli/ByroRedux/issues/2815
