# REN-D8-01: The underwater god-ray focus pattern is sampled in render-origin-relative space, so it jumps at every 4096-unit render-origin snap

Labels: low, renderer, shaders, water, bug
Source: docs/audits/AUDIT_RENDERER_2026-10-27.md

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-27.md` — finding `REN-D8-01` (LOW, Dim 8 Water / Camera-Relative Precision). Not covered by #6415 / #6418 / #6419 / #6420.
**Location (current)**: `crates/renderer/shaders/composite.frag` (`underwaterGodRays`, `underwaterSurfaceFocus`); `crates/renderer/src/vulkan/context/frame_params.rs` (`build_composite_params`, `camera_pos` rebase).

## Description
`underwaterGodRays` computes `p = params.camera_pos.xyz + viewDir * t` and `entry = p.xz - travel.xz * sunPath`, then evaluates value noise at `entry / 70.0` and `entry / 37.0` (`underwaterSurfaceFocus`). `CompositeParams.camera_pos` is deliberately render-origin-relative, and the render origin snaps by `RENDER_ORIGIN_SNAP` (`EXTERIOR_CELL_UNITS`, 4096). Since 4096/70 and 4096/37 are not integers, the noise lattice shifts on the frame the origin re-snaps while the water surface (which uses absolute positions in `water.vert` / `water.frag`) does not move. The hash is integer-exact, so this is an anchoring defect, not a precision one.

## Evidence
```glsl
float a = underwaterFocusNoise(entry / 70.0 + vec2(seconds * 0.35, seconds * 0.20));
float b = underwaterFocusNoise(entry / 37.0 + vec2(-seconds * 0.27, seconds * 0.31));
```
with `entry` built from the render-origin-relative `params.camera_pos`.

## Impact
Every shaft pops for one frame when an underwater camera crosses a cell-grid line in an exterior. Visual and transient. Needs a RenderDoc / on-device check (underwater camera crossing a 4096 grid line) to confirm the pop.

## Related
#6415, #6418, #6419, #6420 (exterior god-ray / shore siblings); `RENDER_ORIGIN_SNAP`.

## Suggested Fix
Ship the render origin's XZ, wrapped by a common period of both noise scales, in a spare lane (e.g. `UnderwaterView.surface.zw`) and evaluate the focus at `entry + originXZ`.

## Completeness Checks
- [ ] **SIBLING**: Other composite/water shader paths that sample world-anchored patterns from `camera_pos` (caustics, volumetrics) are checked for the same origin-snap discontinuity
- [ ] **SHADER-STRUCT**: Any new UBO lane is mirrored in the Rust struct and every standalone GLSL copy (struct-sync rule); recompile `.spv` and run `scripts/check-shader-artifacts.sh`
- [ ] **TESTS**: A test pins that the focus noise input is origin-invariant

