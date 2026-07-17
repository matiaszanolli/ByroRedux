# TD1-008: cell_loader/spawn.rs — spawn_placed_instances is a 1065-line function (81% of the file)

**GitHub Issue**: #2057
**Labels**: low,import-pipeline,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `byroredux/src/cell_loader/spawn.rs:180-1244` (`spawn_placed_instances`)

## Description
Per-REFR mesh-spawn entry point handling placement-root setup and a per-mesh loop (mesh-handle registration, material/texture resolution, skinning, physics/collision, BLAS registration) in one function. File itself is only 1316 LOC, so it's invisible to the file-count discovery command.

## Evidence
Confirmed live: `byroredux/src/cell_loader/spawn.rs` is 1316 LOC total; `pub(super) fn spawn_placed_instances(` starts at line 180, matching the report's claimed location — 1065/1316 ≈ 81% of the file, matching the report's stated proportion.

## Suggested Fix
Split into `spawn_placement_root(...)` + a per-mesh `spawn_mesh_instance(...)` helper.

**Effort**: medium

## Completeness Checks
- [ ] **SIBLING**: Same "single giant function invisible to file-LOC threshold" shape as TD1-009/TD1-010/TD1-011 in this same report — worth a follow-up sweep for a function-LOC (not just file-LOC) discovery check
- [ ] **TESTS**: A regression test pins that split-out helpers produce identical spawned-entity output for a representative REFR set
