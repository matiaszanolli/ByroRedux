# Issue #33 — Renderer: destruction order inconsistent between recreate_swapchain and Drop

**Labels**: enhancement, renderer, low
**State**: OPEN

## R-10

- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context.rs:760-768` (stale path — context.rs was split into `context/` submodules)

Both orders are currently correct but differ, creating maintenance hazard.

**Fix**: Align orders or extract shared helper.
