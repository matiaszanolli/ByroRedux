# REN-D8-02: stage-1b "simulated, not drawn" text and "bindings 12..=15" survive in code and docs after stage 1c

Labels: low,renderer,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6750

**Severity**: LOW · **Dimension**: Volumetrics

## Location
Module doc of `crates/renderer/src/vulkan/volumetrics/local_grid.rs`; header of `crates/renderer/shaders/volumetrics_local_sim.comp`; field doc `VolumetricsPipeline::local_grid`; `docs/engine/memory-budget.md` heading "(#6533 stage 1b)"; docs of `local_fire_views` and the errors in `crates/renderer/src/vulkan/context/resize.rs` / `context/init.rs::local_fire_views_required`; the first paragraph of `LocalBox::overlaps` (describes `contains_source`)

## Description
`d3e3acc7b` (stage 1c) made composite draw the grid, but four places still say "Nothing reads the result yet" / "simulated alongside the froxel field, not yet drawn". Composite's local-fire bindings are 12..=17 (`LocalFireViews` documents 12..=17), yet `local_fire_views`, the resize error and the init error say 12..=15. The first paragraph of `LocalBox::overlaps`'s doc ("The source sphere sits inside the box with the inner margin ...") describes `contains_source`.

## Evidence
`grep -n "not yet drawn\|Nothing reads the result\|12..=15"` over `crates/renderer`. The reflect-validated layout has bindings 12..=17 (the phase table at 17).

## Impact
Misleads the next reader about whether the grid is live. No runtime effect.

## Suggested Fix
Update the doc comments and the two error strings; move the misplaced paragraph to `contains_source`.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
