# REN-D21-01: MAT_FLAG_TRANSLUCENCY is flag-reachable but scalar-unreachable via mat.set/Cornell

Labels: low, renderer, bug

## Description

`MAT_FLAG_TRANSLUCENCY` is flag-reachable but **scalar-unreachable**: `mat.set … material_flags 64` sets the bit, but there are no `mat.set` arms for `translucency_subsurface_color` / `_transmissive_scale` / `_turbulence` and no Cornell probe authors them, so they sit at `Material::default()` (`[0;3]` / `0.0` / `0.0`) — and the shader branch terminates in `* mat.translucencyTransmissiveScale`, making the whole term zero regardless of the flag. A regression isolated to the #1147 Phase-2b SSS lobe bisects **clean** against Cornell and only reproduces on FO4+ BGSM content. Same false-all-clear gap #2477/#2514 closed for the Disney lobe and #2249 closed for `ior`.

## Location

`byroredux/src/commands/scene.rs` (`mat.set` field arms), `byroredux/src/cornell.rs`

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D21-01).

https://github.com/matiaszanolli/ByroRedux/issues/2823
