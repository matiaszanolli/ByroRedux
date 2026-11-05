# RF1-2026-11-05-02: The dismemberment-cap filter exists on the NIF spawn path but not the cell path

Labels: low,import-pipeline,game:fo3,game:fnv,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 1
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/scene/nif_loader.rs:1068` (the gate)
- `byroredux/src/cell_loader/spawn/mesh_instance.rs:1041` (no equivalent)
- `byroredux/src/npc_spawn/seam_blend.rs:221` (a second consumer of `is_dismemberment_cap`)

## Why
`spawn_nif_mesh` skips FO3 and FNV cap sub-meshes so they do not render as gore at the joints, because there is no dismemberment system yet. `spawn_mesh_instance` has no such check. This may be harmless if placed references never carry caps, but nothing in the code says so. No cap has been observed rendering through the cell path.

## Change
Find out whether any model reachable from a placed FO3 or FNV reference has a cap sub-mesh. If one does, the filter belongs in the shared step from #6690. If none does, record that where the difference sits and pin it with a test.

## Acceptance Criteria
- The census result is recorded on this issue, naming the data scanned and the counts
- The two paths either share the filter, or the difference has a comment and a test stating why it is safe

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6691 --json state`)._
