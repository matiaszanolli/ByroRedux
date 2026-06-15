**Severity**: HIGH · **Dimension**: Draw & Instancing · **Status**: NEW
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F1)

## Description
Cull mode and extended-dynamic depth state are emitted once for the leader `batches[i]` at the top of each outer iteration; the `use_indirect` branch then merges all consecutive batches sharing `batch_state = (pipeline_key, render_layer)` into a single `cmd_draw_indexed_indirect` and advances `i = end`. The merge predicate (`crates/renderer/src/vulkan/context/draw.rs:2681-2685`) excludes **only** `Blended && two_sided`. It does NOT exclude:
- (a) opaque `two_sided` batches — `two_sided` is dynamic `cmd_set_cull_mode`, not a `pipeline_key` axis (#930), with `default_cull = NONE` only when `two_sided` (`:2587`), computed once from the leader; or
- (b) batches with differing `z_test`/`z_write`/`z_function` — these are real batch-split keys (`:1882-1884`) but not part of `batch_state`.

## Evidence
The opaque sort key is `(rt_only, 0, render_layer, two_sided, 0, 0, pack_depth_state, mesh, sort_depth, entity)` (`byroredux/src/render/mod.rs:206-216`). `two_sided` (slot 3) and `pack_depth_state` (slot 6) sort *before* mesh, so within one `(pipeline_key, render_layer)` run the `two_sided=false`→`true` batches and the differing-depth-state batches are **adjacent** and get merged. `two_sided` is set on opaque static meshes from the `TwoSided` marker (`byroredux/src/render/static_meshes.rs:189,557`); depth state from the material (`static_meshes.rs:336-337,660-662`). Verified live: the merge loop at `draw.rs:2681` only breaks on `Blended && two_sided`.

## Impact
Visible rendering defect: two-sided opaque cutout geometry (fences, grates, foliage cards, railings) loses its back faces when grouped behind a single-sided leader (`CULL_BACK` applied where `CULL_NONE` is required); opaque batches authored `z_write=0` (glow halos / sky-like) z-fight or write depth wrongly when grouped with a `z_write=1` leader, or vice-versa. Invisible to `cargo test` (needs Vulkan + real cell content).

## Related
#1258 (post-merge batch telemetry), #930 (dynamic two-sided cull), #398 (dynamic depth state).

## Suggested Fix
Extend the merge stop-condition at `draw.rs:2681` to also break on a change in `two_sided`, `z_test`, `z_write`, or `z_function`. Cheapest: compare a `group_state(b) = (b.pipeline_key, b.render_layer, b.two_sided, b.z_test, b.z_write, b.z_function)` instead of `batch_state`. The sort already clusters identical state, so this only fragments groups at genuine state boundaries — no instancing loss within a state-homogeneous run.

## Completeness Checks
- [ ] **SIBLING**: Same merge predicate checked in the direct-draw fallback path and any other batch-grouping site
- [ ] **TESTS**: A regression test pins `group_state` so a leader's cull/depth state cannot bleed across a `two_sided`/depth-state boundary
