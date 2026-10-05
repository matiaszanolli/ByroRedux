# #5321: TD4-2026-10-05-01: This skill still says `gpu_material_size_claims` "does NOT cover `GpuCamera` / `GpuInstance`" — #5203 widened it to every size-pinned `Gpu*` struct

Labels: low,tech-debt,documentation,doc-rot
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD4-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:147-151`
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - #5203 (`d61d06da0`, 10-04) made `pinned_sizes()` cover `GpuMaterial`, `GpuLight`, `GpuTerrainTile`,
    `GpuInstance` and `GpuCamera` (`crates/renderer/src/vulkan/material_tests.rs:1409-1431`).
  - `audit-renderer/SKILL.md:85` already states the widened scope.
  - This skill was re-synced today (`a2c24b16e`) and still sends Dim 3 to hand-check GpuCamera/GpuInstance
    prose that a test now polices.
  - What the guard genuinely cannot see remains `Vertex::SIZE`, a size written without the type name, and
    `GpuWaterParams`-class structs outside `pinned_sizes()`.
- **Suggested Fix**: restate the bullet to cover all five pinned structs, and keep the hand-check instruction for
  the uncovered cases.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
