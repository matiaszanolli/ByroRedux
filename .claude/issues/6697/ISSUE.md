# RF3-2026-11-05-05: `VulkanContext`: move the 17 skinning and morph fields into one struct

Labels: low,renderer,vulkan,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/context/mod.rs:551` (`VulkanContext`, 107 fields)

## Why
17 fields hold skinning state, with 125 access sites: `skin_compute`, `skin_palette`, `skin_slots`, `morph_slots`, `morph_delta_cache`, `failed_skin_slots`, `failed_skin_blas`, `pending_skin_unload_victims`, `pending_morph_unload_victims`, `last_skin_coverage_frame`, `skin_dispatch_ran`, `skin_state_submitted`, `skin_pending_populated`, `clean_skin_frames`, `bind_inverse_upload_failed`, `bind_inverse_upload_warned`, `bind_inverse_upload_failure_count`. Most sites are in `context/skinned_blas_refit.rs` (52), `context/draw.rs` (13), `context/dispatch_skin_and_cluster.rs` (13) and `context/teardown.rs` (12). This also makes the `record_skinned_blas_refit` split easier, because disjoint field borrows work around that function's long `accel` borrow.

## Change
Add one skinning sub-struct that owns these fields, keeping `skin_compute`'s destroy at the same point in teardown.

## Constraints
`post`, `history` and `scratch` are the precedent for this grouping. Field moves are mechanical, and this is host-side state only. Check the 4 `include_str!` readers of `context/mod.rs`, and do not mix in any barrier or lifetime change.

## Acceptance Criteria
- The listed fields live on one sub-struct, and `VulkanContext` no longer declares them
- `cargo test -p byroredux-renderer` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit
- A `BYRO_VALIDATION=1` run on a cell with animated NPCs matches the baseline's warning profile

## Completeness Checks
- [ ] **DROP**: Teardown order in `crates/renderer/src/vulkan/context/teardown.rs` is unchanged
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit
- [ ] **VALIDATION**: A `BYRO_VALIDATION=1` run matches the baseline's warning profile; no barrier or timer bracket moved

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6697 --json state`)._
