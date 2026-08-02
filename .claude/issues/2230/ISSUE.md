# REN-D5-01: memory-budget.md volumetrics VRAM figures are stale by ~2x post-resolution-scaling

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2230

**Dimension**: 5 (Memory)
**Location**: `docs/engine/memory-budget.md` (volumetrics section)
**Status**: NEW

**Description**: `memory-budget.md` describes a fixed 160×90×128 froxel grid; Session 62 made the volumetrics grid resolution-scaled, so the documented figures understate peak 4K VRAM by roughly 2x.

**Impact**: Anyone budgeting VRAM against this doc will under-provision for 4K/high-resolution targets.

**Suggested Fix**: recompute the volumetrics VRAM section against the current resolution-scaled grid formula and update the figures.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
