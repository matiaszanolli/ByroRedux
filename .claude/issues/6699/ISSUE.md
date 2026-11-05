# RF3-2026-11-05-07: `VulkanContext`: group the ray-probe and image-health diagnostics

Labels: low,renderer,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/context/mod.rs:551` (`VulkanContext`, 107 fields)

## Why
Seven diagnostic fields sit beside frame state, with 19 access sites in 8 files: `pending_selected_ray_probe`, `selected_ray_probe_result`, `selected_ray_probe_entity_maps`, `next_selected_ray_probe_generation`, `image_health_buffers`, `image_health_last`, `image_health_total`.

## Change
Add one diagnostics sub-struct. `image_health_buffers` owns GPU buffers, so its destroy stays at the same point in teardown.

## Constraints
`post`, `history` and `scratch` are the precedent for this grouping. Field moves are mechanical, and this is host-side state only. Check the 4 `include_str!` readers of `context/mod.rs`, and do not mix in any barrier or lifetime change.

## Acceptance Criteria
- The listed fields live on one sub-struct, and `VulkanContext` no longer declares them
- `cargo test -p byroredux-renderer` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **DROP**: Teardown order in `crates/renderer/src/vulkan/context/teardown.rs` is unchanged
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6699 --json state`)._
