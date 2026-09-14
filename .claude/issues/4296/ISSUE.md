# #4296: REN-2026-09-14-D23-01: ground-cover blades are rasterized without the projection jitter every other scene pipeline applies — the largest thin-geometry population in an exterior frame is fed to FSR (and TAA) unjittered

- **Labels**: medium,renderer,shaders,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4296
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation (TAA)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert` (`main`, both `gl_Position = pc.viewProj * …` sites); `byroredux/src/app_frame.rs` (the `GroundCoverFrame { view_proj: frame.view_proj, … }` construction); `crates/renderer/src/vulkan/groundcover.rs` (`BladePush.view_proj`)
- **Status**: NEW
- **Description**:
  - FSR requires the whole colour input to carry the per-frame jitter handed to the SDK (`jitter_offset`), and TAA's supersampling relies on the same offset.
  - The main and water pipelines add it in the vertex shader: `currClip.xy += jitter.xy * currClip.w` in `triangle.vert`, `clip.xy += jitter.xy * clip.w` in `water.vert`.
  - The blade pipeline instead projects with its own push-constant `viewProj`, set from `frame.view_proj`, the un-jittered relative matrix built in `assemble_camera`. It never reads `GpuCamera.jitter`: `groundcover_blade.vert` has no `jitter` reference other than per-blade colour jitter.
  - Blades therefore land at the same sub-pixel position every frame while their terrain, the depth they are tested against, and the colour FSR dejitters all move by the jitter.
  - The same push matrix also skips the DOF-effective view-projection. That only matters in TAA mode, since DOF is gated off under FSR.
- **Evidence**:
  - `grep -n jitter crates/renderer/shaders/groundcover_blade.vert` matches only the colour-jitter comments.
  - `triangle.vert` and `water.vert` each have the `jitter.xy * clip.w` line.
  - `app_frame.rs` passes `view_proj: frame.view_proj` on both the enabled and `--groundcover-off` branches.
  - `docs/engine/exal-groundcover.md` §6 says of blade widening that "thin high-contrast geometry is the canonical case TAA handles worst", a design that assumes the reconstruction supersamples the blades. That needs the jitter.
- **Impact**: Visual. In the default FSR Quality path, FSR shifts unjittered blade colour by minus the jitter each frame. Together with D23-02's reactive = 1.0, which suppresses history, the likely result is sub-pixel temporal wobble and un-antialiased blade edges, plus depth-edge flicker where jittered terrain meets unjittered blade bases. In TAA mode the blades get no supersampling. Magnitude is not measured (no engine launch). Needs an A/B capture with a jittered blade VP.
- **Related**: #2772 (TAA/FSR jitter sign convention), #2518 (single jitter predicate), D11-01..03 (other ground-cover pipeline gaps), D23-02.
- **Suggested Fix**: Apply the frame's jitter in `groundcover_blade.vert` the way `water.vert` does: add `jitter.xy * clip.w`, either from the camera UBO or from a jitter lane in the push block. Alternatively hand `prepare_groundcover` the uploaded, jittered DOF-effective VP. Validate visually; this cannot be confirmed by `cargo test`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
