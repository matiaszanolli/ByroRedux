# #4305: REN-2026-09-14-D8-01: #4046 renamed only `next_svgf_temporal_alpha`'s parameter — the `params.w` contract is still documented as the bare camera-static flag in four places, one of which now states the opposite of the code

- **Labels**: low,renderer,shaders,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4305
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**:
  - `crates/renderer/src/vulkan/context/post_passes.rs` (`record_post_passes`, doc on `caustic_history_valid`)
  - `crates/renderer/src/vulkan/svgf.rs` (`SvgfTemporalParams::params` doc, `SvgfPipeline::upload_params`)
  - `crates/renderer/shaders/svgf_temporal.comp` (`SvgfTemporalParams` UBO comment, `hasHistory` branch comment)
- **Status**: NEW (residual of closed #4046; the behavioural fix holds)
- **Description**:
  - Since `5ee1c150`, `params.w` is `caustic_history_valid && !recovering`, i.e. camera parked AND light rig / caustic sources / rigid instances / skinned poses unchanged, with no recovery window open.
  - #4046 renamed the parameter of `next_svgf_temporal_alpha` to `scene_static`. It left every downstream description on the old camera-only meaning, in four places:
    1. `record_post_passes`'s parameter doc says: "The caustic accumulator's EMA is the only consumer down here; SVGF and TAA reject stale history per pixel and keep the camera-only flag, which they read at their own upload sites in `draw.rs`." That is now false on two counts:
       - SVGF consumes `caustic_history_valid`, not the camera-only flag.
       - Its upload site is `build_and_upload_instances.rs`, not `draw.rs`.
    2. `SvgfPipeline::upload_params` still names its parameter `camera_static`. Its inline comment says the flag is set "When the camera is static (view-proj unchanged frame-to-frame) … reverts to the floored EMA the moment the camera moves".
    3. `SvgfTemporalParams::params` doc: "w = camera_static flag".
    4. `svgf_temporal.comp`:
       - The UBO comment reads "w = camera_static".
       - The `hasHistory` comment still records the pre-#4046 trade-off: "dynamic *lighting* on a static surface converges slowly (~N frames) while parked — acceptable". #4046 removed exactly that behaviour.
       - It also says "the raw value still feeds the caustic-history gate". The value fed here *is* the caustic-history gate now.
- **Evidence**: `grep -n camera_static crates/renderer/src/vulkan/svgf.rs crates/renderer/shaders/svgf_temporal.comp` → the SVGF upload fn param and `params` docs. `build_and_upload_instances.rs` calls `next_svgf_temporal_alpha(self.svgf_recovery_frames, caustic_history_valid)`.
- **Impact**: No runtime effect. Anyone tuning the parked-camera GI convergence, or re-deriving `params.w`, is told it is camera-only, and would "re-fix" a light-rig lag that no longer exists or misattribute the extra history drops. #3995's own comment in the shader warns against re-deriving this flag.
- **Related**: #4046 (closed), #3995 (closed).
- **Suggested Fix**:
  - Rename `upload_params`'s parameter to `scene_static` (or `progressive_accumulation`).
  - Reword the four comments to "camera parked AND light rig/scene unchanged, with no recovery window open".
  - Correct `record_post_passes`'s doc to say SVGF consumes `caustic_history_valid` from `build_and_upload_instances.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
