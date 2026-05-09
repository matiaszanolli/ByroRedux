---
issue: 0
title: REN-AUDIT-CROSS-01: Composite + bloom + volumetric resize chain has 3-way gap (latent UAF)
labels: bug, renderer, high, vulkan, sync
---

**Severity**: HIGH (latent dangling-handle bug today, becomes UAF the moment any partial fix lands)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (consolidated finding from Dim 3, 7, 10)

## Locations

- `crates/renderer/src/vulkan/composite.rs:849-1006` — `recreate_on_resize` writes only bindings 0-5; bindings 6 (volumetric) and 7 (bloom) are skipped.
- `crates/renderer/src/vulkan/bloom.rs:676` — `destroy` exists but no `recreate_on_resize`.
- `crates/renderer/src/vulkan/volumetrics.rs:922` — `destroy` exists but no `recreate_on_resize`.
- `crates/renderer/src/vulkan/context/resize.rs:213-253` — calls SSAO destroy + new on resize but skips bloom + volumetric.

Source per-dim findings: REN-D3-NEW-01 (HIGH), REN-D3-NEW-02, REN-D7-NEW-04 (HIGH), REN-D10-NEW-01 (HIGH), REN-D10-NEW-02 (MEDIUM), REN-D10-NEW-08 (LOW).

## Why it's a bug

- Bloom's mip chain is sized from `screen_extent / 2` at construction. After window resize, composite's binding 7 still points at original-extent mips → bloom additive contribution sampled at wrong resolution. Visibly drifts off bright surfaces during live resize.
- Volumetric is currently gated off in composite (`vol.rgb * 0.0` keep-alive at composite.frag:362), so the binding-6 issue doesn't surface today. The moment volumetric is re-enabled (M-LIGHT future tier), the same resize gap activates immediately.
- The composite descriptor still holds the original-extent image views, so there's no UAF *today*. But once REN-D10-NEW-02 ships (bloom/vol pipelines gain `recreate_on_resize` and destroy the old views), the descriptor becomes a dangling-handle UAF unless REN-D10-NEW-01 ships in the same change.

## Fix sketch (atomic — must land together)

1. Add `BloomPipeline::recreate_on_resize` and `VolumetricsPipeline::recreate_on_resize` mirroring the SSAO pattern (destroy + re-construct with new extent).
2. Wire both into `recreate_swapchain` after the SSAO recreate at `resize.rs:213-253`.
3. Extend `composite.rs::recreate_on_resize` to rewrite all 8 bindings (currently only 0-5).
4. Add a `const_assert!` or runtime check that asserts the binding count matches the layout's `DESCRIPTOR_COUNT` when growing.

## Repro

Live window resize during gameplay (RenderDoc / VK_LAYER_KHRONOS_validation). Per `feedback_speculative_vulkan_fixes.md`, **do NOT ship without RenderDoc verification** — failure modes are invisible to `cargo test` and `cargo run` without live window resize.

## Completeness Checks

- [ ] **UNSAFE**: All 4 fix steps add unsafe Vulkan calls; safety comment must explain the destroy-before-recreate ordering.
- [ ] **SIBLING**: Verify same pattern in SSAO recreate (already correct), TAA recreate, SVGF recreate.
- [ ] **DROP**: Bloom + Volumetric Drop impls already correct; verify the new recreate paths use the same destroy as Drop.
- [ ] **LOCK_ORDER**: No RwLock changes.
- [ ] **FFI**: No cxx changes.
- [ ] **TESTS**: Add unit test that asserts composite descriptor write count == layout binding count. (Live-resize behavior tested manually with RenderDoc.)

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
