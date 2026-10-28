# TD1-2026-10-27-01: `crates/renderer/src/mesh.rs` grew 1687 -> 1864 prod LOC (+177) in four geometry-pool commits over two days

Labels: low, renderer, tech-debt, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD1-2026-10-27-01: `crates/renderer/src/mesh.rs` grew 1687 -> 1864 prod LOC (+177) in four geometry-pool commits over two days

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD1-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/renderer/src/mesh.rs` (pool-limit block ~:20-280, procedural primitives ~:1528-1851); `crates/renderer/src/mesh/` already holds `geometry_ssbo.rs`, `geometry_residency.rs`, `geometry_sharing.rs`.

## Description
Commits 3929a406c, ab9d46d18 (10-26), bb97be77c, 22d6e1a80 (10-27) raised the pool caps 5M -> 7M, added reserve headroom and added three admission tiers. The file is now ~136 lines from the 2000 prod-LOC threshold, and the growth sits in two blocks unrelated to `MeshRegistry`'s upload/residency job:
- **Pool-limit and admission-tier policy**: `VERTEX_POOL_*`/`INDEX_POOL_*`, `DEFAULT_GROUND/ACTOR_RESERVE_PERCENT`, `GeometryAdmissionClass`, `PoolReserve`, `PoolLimits`, `check_pool_growth`, `geometry_rebuild_needs_idle`.
- **Procedural primitive generators**: `cube_vertices`, `triangle_vertices`, `quad_vertices`, `box_vertices_colored`, `uv_sphere`, `fullscreen_quad_ui_vertices` - pure CPU data, ~320 lines.

## Evidence
`wc -l crates/renderer/src/mesh.rs` = 2821 total (1864 prod LOC per the tech-debt scanner; 1687 at the 10-25 baseline). `cube_vertices` at :1528, `uv_sphere` at :1803, `GeometryAdmissionClass` at :55, `PoolLimits` at :81 - both blocks still live in `mesh.rs`.

## Impact
Next geometry-pool feature crosses the 2000-LOC line (this file already tripped it once, closed #3451, and was split into `mesh/`). Maintainability only.

## Related
#3451 (earlier mesh.rs split), SAFE-01 / REN-D2-01 / SPT-D4-01 (live fixes touching the same pool paths - land the split after them to avoid rebasing a move over three live fixes).

## Suggested Fix
Move the pool-limit block to `mesh/pool_limits.rs` and the primitive generators to `mesh/primitives.rs`, re-exported from `mesh.rs`. That leaves `MeshRegistry` at ~1250 LOC. Move the matching `pool_growth_cap_tests` / `primitive_winding_tests` modules with their code.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

