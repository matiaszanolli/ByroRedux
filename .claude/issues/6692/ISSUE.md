# RF1-2026-11-05-03: Retire the spawn-parity text guards once both paths share one attach function

Labels: low,import-pipeline,nifal,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 1
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/cell_loader/spawn/mesh_instance.rs:1933` `both_spawn_paths_build_material_texture_handles_through_one_producer`
- `byroredux/src/material_translate.rs:1647` `both_spawn_sites_derive_markers_through_this_boundary`
- `byroredux/src/cell_loader/spawn/mesh_instance.rs:1832` `every_ligh_spawn_site_consumes_the_canonical_falloff_lane`

## Why
These guards scan both files to keep two copies in step. After #6690 there is one copy, so a scan of two files may be checking nothing, and it could still pass.

## Change
After #6690, re-read each guard. Delete it if the shared function makes the divergence impossible, or point it at a behavioural test if a property remains to check.

## Acceptance Criteria
- Each named guard is deleted, retargeted to a behavioural test, or kept with a stated reason
- No surviving guard passes against a deliberately broken shared function, shown by one injected fault

## Completeness Checks
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6692 --json state`)._
