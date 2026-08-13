# REN-D10-05: ssao.comp cameraPos comment claims absolute space, host feeds camera-relative

## Description
`ssao.comp` declares `cameraPos` as "camera world position" while the host deliberately feeds `ssao_cam_rel = camera_pos − render_origin`; the shader math is correct (all uses are differences), the comment is not — and it is what a future author reads before adding an absolute-space consumer. Both this rebase and #1642's soft-particle `camRel` are **unpinned**, unlike the four siblings that all got static source-check tests.

## Location
`crates/renderer/shaders/ssao.comp`, `crates/renderer/src/vulkan/context/post_passes.rs`, `crates/renderer/shaders/triangle.frag`

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2756

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D10-05).
