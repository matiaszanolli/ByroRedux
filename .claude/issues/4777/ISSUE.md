# #4777: REN-D8-2026-09-23-05: volumetrics doc rot — skip semantics, pass position, grid size, HG default, pipeline label, shader-pipeline.md rows

**Severity**: LOW
**Labels**: low, renderer, documentation, doc-rot
**Source**: docs/audits/AUDIT_RENDERER_2026-09-23.md (REN-D8-2026-09-23-05)

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location / Evidence**:
  - `context/post_passes.rs` `record_volumetrics_pass` doc says "composite reads the prior frame's integrated volume, which retains its last valid contents". `volumetrics.rs` `write_tlas` doc says "composite will reuse the prior frame's integrated volume". Both are false since #3685/#3647: a skip records `record_neutral_frame` (a neutral clear) through `skip_clear_decision`, and the code's own comment in the same function says "Never let a prior cell's integrated fog hang over a frame…".
  - `volumetrics.rs` `dispatch` doc: "Natural slot: between caustic and TAA in `draw.rs`". It is actually caustic → volumetrics → SSAO → composite, in `post_passes.rs`.
  - `context/resize.rs` (`volumetrics_cleared_on_skip` reset): "the froxel volume isn't resize-dependent (fixed grid, not screen-sized)". It is render-extent-derived and recreated in this very resize (`recreate_bloom_and_volumetrics`).
  - `context/init.rs` (composite volumetric views): "The 14 MiB × 2 / slot 3D-image allocation". The real figure is 44 B/froxel/slot; at 1080p (240×135×64) that is about 91 MB per slot.
  - `volumetrics_inject.comp` `henyey_greenstein` comment: "Current host default g = 0.4". `DEFAULT_PHASE_G` is 0.8.
  - `volumetrics/init.rs`: the inject pipeline is labelled `"Volumetrics clear"`, which becomes the error context "Volumetrics clear compute pipeline".
  - `docs/engine/shader-pipeline.md`:
    - the `volumetrics_inject.comp` row says "Inject sun-light into froxel grid", omitting clustered local lights, authored fog volumes and combustion transport;
    - binding 12 is described as a "transported emission field" when it is the scalar emissive-source share;
    - binding 13 is described as "for semi-Lagrangian backtrace" when it is used for temporal reprojection;
    - the `composite.frag` row still says "bloom add", although bloom is applied in place by `bloom_apply.comp` after composite.
- **Status**: NEW
- **Suggested Fix**: Correct the prose; relabel the pipeline "Volumetrics injection".

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
