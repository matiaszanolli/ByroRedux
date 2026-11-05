# RF3-2026-11-05-06: `VulkanContext`: group the exposure settings and pass-failure state

Labels: low,renderer,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/context/mod.rs:551` (`VulkanContext`, 107 fields)

## Why
14 fields describe post-pass settings and degraded-pass recovery, with about 45 access sites. Settings: `exposure_auto`, `exposure_fixed`, `exposure_compensation_stops`, `exposure_adaptation_seconds`, `tonemap`. Failure and recovery: `taa_failed`, `svgf_failed`, `caustic_failed`, `exposure_meter_failed`, `svgf_recovery_frames`, `caustic_cleared_on_skip`, `volumetrics_cleared_on_skip`, `pending_caustic_cleared_on_skip`, `pending_volumetrics_cleared_on_skip`.

## Change
Fold these into the existing `post` sub-struct or a sibling, whichever matches what `post` already holds.

## Constraints
`post`, `history` and `scratch` are the precedent for this grouping. Field moves are mechanical, and this is host-side state only. Check the 4 `include_str!` readers of `context/mod.rs`, and do not mix in any barrier or lifetime change.

## Acceptance Criteria
- The listed fields live on one sub-struct, and `VulkanContext` no longer declares them
- `cargo test -p byroredux-renderer` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6698 --json state`)._
