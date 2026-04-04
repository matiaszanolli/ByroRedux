# Issue #29: Renderer: subpass dependency omits LATE_FRAGMENT_TESTS stage

- **State**: OPEN
- **Labels**: bug, renderer, low, vulkan
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context.rs:985-1000`

`dst_stage_mask` has `EARLY_FRAGMENT_TESTS` but not `LATE_FRAGMENT_TESTS`. Fragment shader uses `discard`, which may defer depth writes to late stage. Benign on desktop, spec gap on strict implementations.

**Fix**: Add `LATE_FRAGMENT_TESTS` to both src and dst stage masks.
