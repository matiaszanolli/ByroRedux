# REN-COMP-H1: Fog applied in triangle.frag leaks into SVGF history — ghosting on transitions

**Issue**: #428 — https://github.com/matiaszanolli/ByroRedux/issues/428
**Labels**: bug, renderer, high

---

## Finding

`CompositeParams` declares `fog_color` / `fog_params` fields at `crates/renderer/src/vulkan/composite.rs:44-46` and the shader declares them at `crates/renderer/shaders/composite.frag:24-25`, but these fields are **never referenced** in composite.frag below the struct declaration.

Fog is actually computed in `crates/renderer/shaders/triangle.frag:974-984` and baked into both `directLight` (line 980, via `mix`) AND `indirectLight` (line 983, `(1-fogFactor)`) before the G-buffer write.

Two separate issues that compound:

1. **Dead UBO bandwidth**: bytes at `CompositeParams` offsets 0..32 are uploaded every frame but never read by the GPU. Wasted (minor).

2. **SVGF ghosting on fog transitions** (more serious): because indirect in the G-buffer is already fog-attenuated, SVGF accumulates that fogged indirect into its history. When fog parameters change (cell load, weather transition, scripted fog), the history carries the **previous frame's fog values** until α=0.2 accumulation washes them out (~5-10 frames).

Also contradicts the audit checklist premise "fog applied to direct, not indirect" — fog is applied to both.

## Impact

- **Cell-load ghosting**: when a player transitions from an interior to an exterior (or vice versa) with different fog, the SVGF indirect-light history shows the old fog modulating the new scene's indirect GI for ~5-10 frames.
- **Weather-change ghosting**: scripted fog transitions (rain rolling in, spell effects) produce the same trail.
- Imperceptible on slowly-changing fog; visible on discrete fog transitions.

## Fix

Two options:

**(a) Remove the dead fields + document** (quick, no behavior change):
- Delete `fog_color` / `fog_params` from `CompositeParams` in both the Rust struct (`composite.rs:44-46`) and the shader struct (`composite.frag:24-25`).
- Add a comment: `// Fog is applied during the geometry pass (triangle.frag:974-984), not in composite. See COMP-H1.`

**(b) Move fog into composite** (long-term correct, preferred per triangle.frag:972-973 comment):
- Remove fog math from `triangle.frag:974-984`.
- Apply fog in `composite.frag` after the `combined = direct + indirect*albedo + caustic` reassembly (line 172) but before `aces()` tone mapping.
- SVGF history no longer carries fog → no ghosting on transitions.
- Eliminates the fog re-run on reprojected history entirely.

Option (b) is the architecturally right fix. Option (a) is the minimum to stop the UBO waste. Pick (b).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check that moving fog doesn't break any other consumer — grep for `fogFactor`, `fog`, `fog_params` across shaders and Rust. Also check `caustic_splat.comp` doesn't expect fog-attenuated input.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Transition test — step the camera from an interior cell to an exterior cell with markedly different fog. Before fix: visible multi-frame ghosting. After fix (b): clean transition.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 10 H1.
