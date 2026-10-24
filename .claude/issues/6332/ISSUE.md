# PERF-D3-2026-10-23-02: LOD starvation re-arm reopens on every pool compaction

- **GitHub**: #6332
- **Severity**: MEDIUM
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-23.md

Found by `/audit-performance` 2026-10-23 (`docs/audits/AUDIT_PERFORMANCE_2026-10-23.md`, PERF-D3-02). **Confidence: medium** — derived from a code trace, not reproduced.

## Location
`byroredux/src/app_step.rs` (re-arm edge, ~137-147); `byroredux/src/cell_loader/lod_support.rs` (`lod_should_rearm_after_starvation` = `starved && admission_open`); `byroredux/src/cell_loader/{object_lod,placement_lod}.rs` (partial upload, then `unload_*_lod_block`); `crates/renderer/src/mesh/geometry_ssbo.rs` (`scene_geometry_admission_closed = false` at the end of `plan_geometry_compaction`); `crates/renderer/src/mesh.rs` (`drop_mesh` sets `geometry_has_holes`).

## Description
Since #6092 (`635330d08`), a multi-sub-mesh LOD quad (object `.bto` quad, placement cell) whose k-th sub-mesh trips the hard geometry-pool cap is abandoned whole: `unload_*_lod_block` drops the k-1 sub-meshes that already uploaded. Those drops set `geometry_has_holes`, so the next geometry rebuild runs `plan_geometry_compaction`, which reopens `scene_geometry_admission_closed` **unconditionally**. The re-arm test only checks the closed→open edge and never that the pool gained headroom. After compaction the pool is back to its pre-attempt layout and the closest-first order is deterministic, so the same quad is the first candidate again and fails at the same sub-mesh.

The existing tests (`a_capacity_refusal_is_held_back_not_cached_as_a_miss`, `a_held_back_quad_is_retried_after_admission_reopens`, `starvation_rearms_only_once_admission_reopens`) exercise pure classifiers with a fake admission flag; none models "unwinding my own partial upload reopens admission".

## Impact
While the pool is saturated and the nearest unresident quad is multi-sub-mesh, each cycle costs an archive read + decode, partial GPU-pool appends, a chunked O(pool) geometry rebuild (typical ~208 MB, cap 4M×104 B + 16M×4 B = 480 MB), and possibly a `geometry_rebuild_needs_idle` device-idle wait at low VRAM headroom. Bounded per cycle by `LodWorkBudget`, but can recur every few frames. This is the re-decode/re-reject regression #6054 was written to prevent, moved onto a compaction-driven path. Terrain LOD is single-mesh and unaffected.

**Runtime tell**: a repeating `Geometry pool admission reopened — retrying the held-back LOD quads` log line with `lod_starved` non-zero on the bench line and no cell unload between repeats.

Related: #6054, #6092, #6093, #6106.

## Suggested Fix
Add re-arm hysteresis (record live vertex/index counts when the Starved arm fires; re-arm only once usage dropped by at least the held-back quad's footprint), or pre-check the whole quad's totals against remaining pool headroom before uploading any sub-mesh so a refusal never leaves a partial to unwind. Add a test driving a fake registry through partial upload → unload → compact and assert no re-arm.

## Completeness Checks
- [ ] **DROP**: Pool/compaction teardown ordering unchanged
- [ ] **SIBLING**: Same pattern checked in related files/systems
- [ ] **TESTS**: A regression test pins this specific fix
