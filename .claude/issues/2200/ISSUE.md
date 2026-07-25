# TD2-NEW-01: frame_upscaler.rs hand-rolls the same 4-image barrier shape instead of a local helper

**Labels**: low, renderer, vulkan, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD2-NEW-01)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2200

## Severity
LOW

## Dimension
2 (Logic Duplication)

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:592-640` (`record_fsr_barriers_before`)

## Description
Four of the six barriers built in `record_fsr_barriers_before` are byte-identical except `.image(...)`, applied to `inputs.scene_color`, `inputs.motion_vectors`, `inputs.reactive`, `inputs.transparency` (confirmed live at lines 604-634). New occurrence of the duplication class Dim 2 already fixed once this window (#2071/TD2-112, a different barrier shape).

## Evidence
`crates/renderer/src/vulkan/frame_upscaler.rs:604-634` — 4 near-identical `vk::ImageMemoryBarrier::default()...` blocks differing only in `.image(...)`.

## Impact
Cosmetic/maintainability only — all 4 are semantically correct today.

## Related
#2071/TD2-112 (closed) fixed a different barrier shape in `descriptors.rs`.

## Suggested Fix
Add a small local closure/free function `fn shader_read_barrier(image: vk::Image, range: vk::ImageSubresourceRange) -> vk::ImageMemoryBarrier` and call it 4×.

## Completeness Checks
- [ ] **UNSAFE**: `record_fsr_barriers_before` is already `unsafe fn`; the extraction adds no new unsafe surface — confirm the existing safety comment still covers the helper's call sites after refactor
- [ ] **SIBLING**: Check `descriptors.rs` and other barrier-recording sites (`gbuffer.rs`, `svgf.rs`) for the same same-layout/access-pair shape before generalizing the helper
- [ ] **TESTS**: N/A — pure refactor, no behavior change
