# Investigation — #800

**Domain**: renderer (composite shader)

## Code path

`crates/renderer/shaders/composite.frag:184` gates the sun disc on `cos_angle > sun_edge_start` only — no `dir.y > 0` check. Below-horizon directions (`elevation < 0`) are handled by the ground-tint mix at L107-110, but the disc still adds at L217 over those pixels.

`elevation = dir.y` (L91). Cloud-layer gates at L130, L147, L158, L169 all use `elevation > 0.0` — established convention. Match it.

## Fix

Add `elevation > 0.0` to the disc gate. One-token change. Matches Option 1 in the issue description and the cloud-layer convention.

## Recompile

`composite.frag.spv` is `include_bytes!`'d into the binary, so it must be recompiled with `glslangValidator -V composite.frag -o composite.frag.spv` from `crates/renderer/shaders/`.

## Scope

1 GLSL file + 1 SPIR-V binary.
