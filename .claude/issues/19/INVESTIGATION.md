# Investigation: Issue #19

## Root Cause
`create_render_pass()` at context.rs:963 sets the depth attachment's `store_op` to
`DONT_CARE`, which allows the driver to discard depth data after the render pass.
This prevents any future pass from reading the depth buffer (needed for SSAO,
deferred shading, shadow mapping, depth-of-field, etc.).

## Affected Code
- `crates/renderer/src/vulkan/context.rs:963` — depth attachment `store_op`

## Sibling Check
- Lines 954-955: color attachment stencil DONT_CARE — correct (no stencil use)
- Lines 964-965: depth attachment stencil DONT_CARE — correct (no stencil use)
- Only line 963 (depth store_op) needs to change

## Fix
Change `.store_op(vk::AttachmentStoreOp::DONT_CARE)` to `.store_op(vk::AttachmentStoreOp::STORE)`
on the depth attachment.

## Scope
1 file, 1 line changed.
