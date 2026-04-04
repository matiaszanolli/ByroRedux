# Investigation: Issue #29

## Root Cause
`create_render_pass()` at context.rs:985-995 specifies `EARLY_FRAGMENT_TESTS` in both
src and dst stage masks of the subpass dependency, but omits `LATE_FRAGMENT_TESTS`.

The fragment shader uses `discard` (alpha test), which per Vulkan spec can defer
depth/stencil writes to the late fragment test stage. Without `LATE_FRAGMENT_TESTS`
in the dependency, the synchronization doesn't cover this deferred write path.

## Affected Code
- `crates/renderer/src/vulkan/context.rs:985-995` — single `SubpassDependency`
- No sibling render pass functions (single `create_render_pass`)

## Fix
Add `LATE_FRAGMENT_TESTS` to both `src_stage_mask` and `dst_stage_mask`.
Also add `DEPTH_STENCIL_ATTACHMENT_WRITE` to `dst_access_mask` since the late
stage performs the actual depth write.

## Scope
1 file, ~4 lines changed.
