# REN-D17-09: material.rs presets have an unverifiable citation and an undocumented-in-code fallback role

Labels: low, renderer, documentation

## Description

(a) The module pins its values to `knightcrawler25/GLSL-PathTracer`, which the user-memory note *reference_glsl_pathtracer.md* records as cloned to `/mnt/data/src/reference/` — **it is not there**, so the Dim-17 checklist item "Disney preset constructors match documented values (cross-ref GLSL-PathTracer)" is not executable offline and every preset scalar is citable but unverifiable. Same for the four `pbr.glsl` doc references into `disney.glsl` line ranges. (b) The doc claims the presets are the "fallback when authored BGSM is absent"; **no such fallback exists** — `translate_material` never consults `presets`, and the only hits outside `material.rs` are its own tests. A documented fallback role no code implements is an invitation to wire it in and bypass the NIFAL single boundary.

## Location

`crates/renderer/src/vulkan/material.rs` (`pub mod presets`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D17-09).

https://github.com/matiaszanolli/ByroRedux/issues/2811
