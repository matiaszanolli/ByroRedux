# REN-D5-01: CausticPipeline/TaaPipeline/SvgfPipeline::destroy are non-idempotent

**Severity**: HIGH
**Dimension**: 5 — Memory/Lifecycle
**Location**: `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/svgf.rs` (each `destroy` + `recreate_on_resize`); callers `crates/renderer/src/vulkan/context/resize.rs` and `crates/renderer/src/vulkan/context/mod.rs`. Correct model: `crates/renderer/src/vulkan/presentation.rs`.

## Description

All three `recreate_on_resize` impls self-`destroy()` on partial failure and propagate the error with the field left `Some`. `destroy_allocator_owned_resources` (reached from `Drop`) then destroys the same object a second time. Scalar handles are guarded with `if handle != null`, but none is nulled after destruction — so the guard never arms for the second pass.

## Evidence

`grep -c '= vk::.*::null();'` → caustic.rs 0, taa.rs 0, svgf.rs 0, presentation.rs 8. Six of nine renderer subsystems with this shape are safe; three are not.

## Impact

Double `vkDestroyPipeline`/`vkDestroyPipelineLayout`/`vkDestroyDescriptorPool`/`vkDestroyDescriptorSetLayout`/`vkDestroySampler` — spec violation, driver-side double-free at teardown. Trigger: VRAM/host allocation failure during a swapchain resize.

## Suggested Fix

Null each handle immediately after destroying it in all three `destroy()` bodies, mirroring `PresentationPipeline::destroy`. Host-side only, no barrier risk.

## Related

#2685, #2487, #1211, #2739 (same hygiene class one layer down)

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D5-01).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2741
