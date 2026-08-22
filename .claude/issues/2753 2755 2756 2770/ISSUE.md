# Batch fix: #2753, #2755, #2756, #2770

All renderer/doc-tech-debt issues from AUDIT_RENDERER_2026-08-12b.md.
Domain: **renderer** → `byroredux-renderer`.

## #2753 — REN-D10-03: GpuCamera doc stale consumer credit + ambiguous position frame
`gpu_types.rs` (`GpuCamera::render_origin`/`::position`) doc credits
`triangle.vert` with absolute reconstruction; moved to `triangle.frag` since
#1496 (now the busiest consumer: absolute reconstruction, camRel soft-particle
rebase, renderOrigin.w FSR-reset view). `position` documented as "xyz = world
position" ambiguously — should say ABSOLUTE explicitly like the rest of the
block.

## #2755 — REN-D10-04: shader-pipeline.md stale cell_loader/references.rs path
Doc cites `cell_loader/references.rs` (a file) for `RT_ABSOLUTE_PRECISION_CEILING`;
it's actually a directory `byroredux/src/cell_loader/references/` — constant +
predicate in `mod.rs`, firing `debug_assert!` in `complete.rs`.

## #2756 — REN-D10-05: ssao.comp cameraPos comment wrong, unpinned rebase
`ssao.comp` declares `cameraPos` as "camera world position" but host
(`post_passes.rs`) feeds `ssao_cam_rel = camera_pos - render_origin` (camera-
relative). Shader math is correct (all uses are differences) but comment is
misleading. This rebase + #1642's soft-particle camRel (triangle.frag) are
unpinned by tests, unlike 4 sibling rebases that already have static
source-check tests.

## #2770 — REN-D1-03: Magic material kind 11 (MultiLayerParallax) hand-copied
Value `11` hand-copied as a literal at 4 sites (predicates.rs, draw.rs,
acceleration/tests.rs, triangle.frag) instead of using the shared
MATERIAL_KIND_* table (like MATERIAL_KIND_GLASS beside it). Test declares a
4th independent copy, so it can't detect `is_refractive_glass`-style drift.
