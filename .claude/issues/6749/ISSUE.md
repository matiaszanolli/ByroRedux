# REN-D8-01: the memory-budget ledger understates the local-grid irradiance field by 2x and omits the brick buffers

Labels: low,renderer,memory,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6749

**Severity**: LOW · **Dimension**: Volumetrics

## Location
`docs/engine/memory-budget.md` ("World-space combustion local grid" paragraph); `irradiance_extent` / `irradiance_resident_bytes` in `crates/renderer/src/vulkan/volumetrics/local_grid.rs`

## Description
The ledger says "160x90x64 x 8 B x 2 FIF ~ 14 MiB at 1280x720". The image is created from `irradiance_extent(froxel_extent)`, whose width is doubled (radiance and mean light direction side by side, per `local_irradiance.glsl`): about 28 MiB at 720p and about 112 MiB at a 2560x1440 render (default FSR Quality at 4K). The per-slot 8 KiB `BRICK_BUFFER_BYTES` buffers and the 8 KiB `combustion_phase_table` image are not listed either.

## Evidence
`irradiance_extent` is `width: froxel.width * 2`. `irradiance_resident_bytes` (which applies the doubling) is called only from the creation `log::info!`. `memory_budget_doc_states_the_resident_cost` greps only `**48 MiB resident**`, so these rows cannot drift into a test failure.

## Impact
The VRAM ledger (RT minimum 6 GB, ~4 GB budget) is off by up to ~56 MiB at 1440p render. No functional effect.

## Suggested Fix
Correct the paragraph (twice the froxel width; add the brick and phase-table rows) and extend the doc test to pin `irradiance_resident_bytes` for a stated render extent, as `volumetrics_host_buffer_ledger_matches_the_code` does for host buffers.

Related: #6531 / #6533 (local grid tracking).

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
