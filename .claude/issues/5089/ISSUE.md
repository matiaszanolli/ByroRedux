# TD1-2026-09-29-02: `groundcover.rs` re-crossed 2000 (2025) (regression of #4568)

**Labels**: low,renderer,terrain-exterior,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

**Regression of #4568** (#4568 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #4568 · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/groundcover.rs`
- **Age**: #4568 extracted `groundcover_stats.rs`, leaving 1982. Then `aabd99a05` (#4413 model tier,
  09-24), `21430f45e` (#4729) and `c14f5361a` (#4056) pushed it back over.
- **Suggested Fix**: split construct from record.
  - Construct: `new`, `create_buffers`, `create_layouts`, `set_layout_contracts`, `create_descriptors`,
    `build_*pipelines` (≈421–1200).
  - Per-frame: `prepare`, `harvest`, `write_descriptor_sets`, `record_scatter`, `record_interaction`,
    `record_draw` (≈1205–1940).
  - This is the axis that worked for `context/` and volumetrics.

**Validated at HEAD 9fcfdc3fc**: `prod_loc crates/renderer/src/vulkan/groundcover.rs` = 2025.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
