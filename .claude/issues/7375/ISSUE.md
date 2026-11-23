# #7375 — REN-D1-2026-11-23-03: memory-budget.md Mesh Registry table still reads the pool hard caps as the maximum; the #6106 dead-span credit lifts the raw host pools to 1.25x

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D1-03). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (doc rot)
- **Dimension**: AS Correctness (geometry-pool seam, #6106)
- **Location**: `PoolLimits::pool_ceiling` / `DEAD_SPAN_CREDIT_PERCENT` (`mesh/pool_limits.rs`); `MeshRegistry::dead_vertices`/`dead_indices` (`mesh.rs`); `docs/engine/memory-budget.md` "Mesh Registry" table (`VERTEX_POOL_HARD_CAP` row, "same as the admission threshold") and the "Vertex / index pools ... ~840 MB cap" row.
- **Status**: NEW (the behaviour is documented in `exterior-grid-streaming.md`, only the budget page lags)
- **Description**: 11f898383 makes admission test live geometry (`pending.len() - dead`) and lets the raw `pending_vertices` / `pending_indices` run to `hard + hard/100*25` (8.75 M vertices, 35 M indices) until the compaction inside `rebuild_geometry_ssbo`. The uploaded SSBO is always the compacted pool (`plan_geometry_compaction` runs first, gated on `geometry_has_holes`, which every scene-mesh drop sets together with the dead counters), so VRAM stays at the caps. Host RAM does not: worst case about +182 MB vertices +28 MB indices = ~210 MB over the figure the budget page publishes.
- **Evidence**: `MeshRegistry::accumulate_global_geometry`: its first check `pending.len() + n > pool_ceiling()` bails and latches the tier; `release_mesh_ref` adds `mesh.vertex_count` to `dead_vertices` only for `is_scene_mesh`; `plan_geometry_compaction` zeroes both. Tests `a_crossings_unload_frees_room_for_its_loads_before_compaction` and `dead_spans_past_the_credit_refuse_until_compaction` pass.
- **Impact**: documentation only (host RAM ledger understated by up to ~210 MB at the ceiling); no BLAS effect: static BLAS are self-contained and patch BLAS re-derive their addresses each frame (content check below).
- **Related**: #6106, `exterior-grid-streaming.md` ("Admission counts live geometry").
- **Suggested Fix**: add the ceiling row (and the host-RAM peak) to the Mesh Registry table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
