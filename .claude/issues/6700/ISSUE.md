# RF3-2026-11-05-08: `VulkanContext`: group the ground-cover and terrain-tile state

Labels: low,renderer,terrain-exterior,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/context/mod.rs:551` (`VulkanContext`, 107 fields)

## Why
Seven exterior fields have 51 access sites in 8 files: `groundcover`, `groundcover_models`, `groundcover_bench`, `groundcover_bench_cells`, `terrain_tiles`, `terrain_tile_free_list`, `terrain_tiles_dirty`.

## Change
Add one exterior sub-struct, or two if ground cover and terrain tiles read better apart. Keep teardown order for the fields that own Vulkan objects.

## Constraints
`post`, `history` and `scratch` are the precedent for this grouping. Field moves are mechanical, and this is host-side state only. Check the 4 `include_str!` readers of `context/mod.rs`, and do not mix in any barrier or lifetime change.

## Acceptance Criteria
- The listed fields live on one sub-struct, and `VulkanContext` no longer declares them
- `cargo test -p byroredux-renderer` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit
- An exterior run with ground cover matches the baseline's `bench:` non-timing tokens

## Completeness Checks
- [ ] **DROP**: Teardown order in `crates/renderer/src/vulkan/context/teardown.rs` is unchanged
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6700 --json state`)._
