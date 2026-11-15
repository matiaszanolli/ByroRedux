# #6980: TD7-2026-11-15-01: the exterior cell edge `4096.0` was re-declared twice in the 11-07/11-08 window, and the 4x4 quad hole-mask layout is re-typed five times beside it

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6980
- **Labels**: low,terrain-exterior,esm-plugin,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:830` (`REGION_CELL_UNITS`, used at `:925-928`); `byroredux/src/cell_loader/lod_coverage.rs:133` (`const CELL: f32 = 4096.0`); mask layout at `lod_coverage.rs:102-105`, `:120-123`, `:136`
- **Age**: `cf3ccc958` 2026-11-07 (`Refs #6725`), `8e7624256` 2026-11-08 (`Fix #6639`)
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW (class history: closed #1112 "4096.0 literal scattered across 6+ files (divergent bug-fix history)", #2436, #4939; this is a new instance, not a regression of those lines)
- **Description**: `byroredux_core::math::coord::EXTERIOR_CELL_UNITS` (`crates/core/src/math/coord.rs:41`) is documented as the sole source of truth after #1112. Its doc comment records that the scattered literal had "at least one divergent bug-fix history (TD3-110 Z-flip sign disagreement)". Two production sites added since then re-type it. `crates/plugin` already depends on core and imports the constant at `esm/cell/index.rs:11` and `esm/cell/worldspace.rs:6`, so the plugin copy has no layering excuse. The only deliberate second copy, `bsa::uvd::EXTERIOR_CELL_UNITS`, is const-asserted against core from the bin crate (#4939) because bsa does not depend on core. Neither new copy has an assert. In `lod_coverage.rs` the same 4x4 quad layout is also spelled as bare `4`, `0..4`, `dy * 4 + dx` and `level == 4` in `find_overlaps`, `quad_resident_mask` and `quad_cell_bit`. `streaming_helpers.rs:438` names the same quantity `FINEST_QUAD_EDGE = 4` locally inside one function.
- **Evidence**:
  ```
  world.rs:830        const REGION_CELL_UNITS: f32 = 4096.0;
  lod_coverage.rs:133 const CELL: f32 = 4096.0;
  lod_coverage.rs:136 ((0..4).contains(&dx) && (0..4).contains(&dy)).then(|| 1u16 << (dy * 4 + dx))
  ```
- **Impact**: No divergence today because the value is invariant. If the cell edge were ever changed (a Starfield or FO76 grid, a test fixture scale), the region-polygon coverage and the LOD hole mask would silently disagree with the cell loader, which is the #1112 failure mode.
- **Related**: #1112, #2436, #4939 (closed, same constant); `crates/core/src/math/coord.rs:30-41`.
- **Suggested Fix**: delete both literals and use `byroredux_core::math::coord::EXTERIOR_CELL_UNITS`. Name the quad edge once (`const QUAD_EDGE_CELLS: i32 = 4` beside `quad_cell_bit`, reused by `streaming_helpers.rs:438`) and derive the `u16` mask width from `QUAD_EDGE_CELLS.pow(2)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
