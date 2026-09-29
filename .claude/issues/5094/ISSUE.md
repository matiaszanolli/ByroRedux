# TD1-2026-09-29-07: `volumetrics.rs` is back over the line at 2391 (+490) (regression of #2256)

**Labels**: low,renderer,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

**Regression of #2256** (#2256 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #2256 · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`
- **Age**: `a5dfd472d` (#4781, +93), `7859a1b04` (#4775), `29e62cf2f` (#4776), `4ec1e48c5`
  (#4959–#4970). #2256 had moved construction into `volumetrics/init.rs`.
- **Suggested Fix**:
  - Move fog-volume clustering (`GpuFogVolume`, cluster build, portal sweep, grid filter; ≈202–1060,
    about 850 LOC) to `volumetrics/fog_clusters.rs`.
  - Move combustion light moments (≈1190–1370 plus `append_combustion_surface_lights`) to
    `volumetrics/combustion.rs`.
  - Repoint the `include_str!("volumetrics.rs")` scans in `caustic.rs`, `svgf.rs` and `context/draw.rs`.

**Validated at HEAD 9fcfdc3fc**: `prod_loc crates/renderer/src/vulkan/volumetrics.rs` = 2391.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
