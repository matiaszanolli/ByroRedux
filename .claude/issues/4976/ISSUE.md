# #4976: REN-D11-2026-09-27-04: the "frame parameters absent" native-blit branch hard-codes the scene image's source layout and bypasses #4538's exclusivity assert (sibling of #4592 not swept)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4976
- **Labels**: low,renderer,vulkan,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D11-2026-09-27-04**._

- **Severity**: LOW. It is correct today by TAA/FSR construction exclusivity, and the branch is unreachable while the jitter and dispatch gates share `is_fsr_dispatch_active`.
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs` `FrameUpscaler::record`, the `let Some(frame_params) = fsr_frame else { … }` arm: `record_native_blit(device, cmd, frame, inputs.scene_color, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)`.
- **Status**: NEW. Related closed issues: #4592 (the recovery branch fixed to `inputs.scene_color_layout`) and #4538 (the `debug_assert_eq!` in `record_fsr_barriers_before`).
- **Description**:
  - The bridge branch and the recovery branch both pass `inputs.scene_color_layout`.
  - The params-absent branch passes a literal `SHADER_READ_ONLY_OPTIMAL` as the source layout. It returns before `record_fsr_barriers_before`, so the #4538 `debug_assert_eq!(inputs.scene_color_layout, SHADER_READ_ONLY_OPTIMAL)` never runs on it.
  - The #4592 test (`the_recovery_blit_sources_from_the_scene_images_actual_layout`) inspects only the third call site by position, and its comment names this one ("bridge, params-absent, recovery") without checking it.
- **Evidence**: The literal quoted above versus the `inputs.scene_color_layout` argument on the other two call sites.
- **Impact**: None today. If FSR mode ever consumes a `GENERAL` input (the refactor #4538 guards against), this branch would record the #4592 VUID (`oldLayout-01197`) with no assert firing.
- **Suggested Fix**: Pass `inputs.scene_color_layout` here too. Extend the #4592 source pin to every `record_native_blit(` call site rather than `positions[2]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
