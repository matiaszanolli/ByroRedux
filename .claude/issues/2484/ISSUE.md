# REN-D4-2026-08-07-02: copy_depth_to_history's pre-copy barrier omits DEPTH_STENCIL_ATTACHMENT_WRITE from its source access scope

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2484
**Finding ID**: REN-D4-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 4 — Sync/Barriers
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs::VulkanContext::copy_depth_to_history` (the `depth_to_src` `vk::ImageMemoryBarrier`)
**Status**: NEW (prior audits recorded this function as "outside any pass with paired barriers" but did not audit the access masks)

## Description
The barrier that moves `depth_image` from `DEPTH_STENCIL_READ_ONLY_OPTIMAL` to `TRANSFER_SRC_OPTIMAL` before the history copy declares `src_access_mask = DEPTH_STENCIL_ATTACHMENT_READ | SHADER_READ` — no `DEPTH_STENCIL_ATTACHMENT_WRITE`. The data being copied *is* the render pass's depth write. A barrier whose first access scope contains only reads performs no availability operation for that write.

## Evidence
```rust
let depth_to_src = vk::ImageMemoryBarrier::default()
    .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::SHADER_READ)
    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
    .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
```
emitted with `src_stage = LATE_FRAGMENT_TESTS | FRAGMENT_SHADER`, `dst_stage = TRANSFER`.

## Impact
Almost certainly **legal today** via dependency chaining: `helpers.rs`'s `dependency_out` has `dst_stage_mask = FRAGMENT_SHADER | COMPUTE_SHADER` / `dst_access_mask = SHADER_READ`, and this barrier's first scope contains `FRAGMENT_SHADER` + `SHADER_READ`, so the two scopes intersect and the render pass's `DEPTH_STENCIL_ATTACHMENT_WRITE` availability propagates through the chain. The exposure is that the correctness of a depth read now depends on an incidental overlap with a dependency declared for an unrelated consumer (SSAO/SVGF/composite). Narrowing `dependency_out` — a plausible future optimisation — would silently break this copy. Symptom would be stale/garbage soft-particle depth fade, invisible to `cargo test`.

## Related
`helpers.rs::create_render_pass` `dependency_out`; #947 (the last change to that dependency's stage masks).

## Suggested Fix
**Needs sync-validation verification** to confirm whether the layer accepts the chain. If a change is made at all, the minimal one is adding `DEPTH_STENCIL_ATTACHMENT_WRITE` to `depth_to_src.src_access_mask` and `EARLY_FRAGMENT_TESTS` to its `src_stage` so the barrier is self-sufficient rather than chain-dependent — a strict widening, no behavioural narrowing.

## Completeness Checks
- [ ] **TESTS**: Needs `BYRO_VALIDATION=1` sync-validation capture before any change
