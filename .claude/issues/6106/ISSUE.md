# REN-SD-2026-10-19-01: Space freed by a crossing's own unload is unavailable to that crossing's loads, and a full-detail cell refused at the geometry pool is never retried or counted

Labels: medium,renderer,memory,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6106

**Severity**: MEDIUM (a cell loses statics or its terrain mesh; the loss is silent and permanent for that residency) · **Dimension**: Memory/Lifecycle

## Location
- `crates/renderer/src/mesh.rs` `accumulate_global_geometry` (admission tested against `pending_vertices.len()` / `pending_indices.len()` via `check_pool_growth`; closed latch checked first)
- `crates/renderer/src/mesh/geometry_ssbo.rs` `plan_geometry_compaction` (the only place holes are squeezed out and the only place `scene_geometry_admission_closed = false`)
- `byroredux/src/app_frame.rs` (rebuild gate at the `defer_geometry_rebuild` / `rebuild_geometry_ssbo` block) and `byroredux/src/streaming/mod.rs` `WorldStreamingState::geometry_batch_in_progress`
- `byroredux/src/app_step.rs` `step_streaming` (sets `lod_reconcile_pending = true` at the crossing; comment "Unload first to free GPU resources before kicking new loads")
- Refusal sinks: `byroredux/src/cell_loader/spawn/mesh_instance.rs` `prepare_mesh_upload_range` ("skipping remaining uncached geometry for this cell") and `byroredux/src/cell_loader/terrain.rs` (`Failed to upload terrain mesh` -> `return None`)

## Description
`drop_meshes` marks `geometry_has_holes` and `geometry_dirty` but leaves the dead spans inside `pending_vertices` / `pending_indices`. Admission counts those spans.

Compaction runs only inside `rebuild_geometry_ssbo`, which `app_frame.rs` starts only when `!geometry_batch_in_progress()`. That predicate is true whenever `pending` is non-empty, an apply job is active, or `lod_reconcile_pending` is set. The crossing that unloads sets `lod_reconcile_pending` itself (`step_streaming`) and queues the loads in the same call.

So the dead spans of the cells unloaded at a crossing stay counted until every load and the LOD reconcile have drained. A load that crosses the cap in the meantime closes admission (`scene_geometry_admission_closed = true`), and every later fresh mesh of that and following cells is refused.

The refusal is not retried. `prepare_mesh_upload_range` returns `PreparedMeshUpload::Failed`, `spawn_mesh_instance` returns `false`, and the cell is inserted into `loaded` like any other. A refused terrain mesh returns `None` from the terrain spawn and the cell has no ground. After the batch settles, compaction reopens admission, but nothing re-runs the refused cell. The only signal is a one-line `log::warn!` per placement batch; no telemetry counter exposes full-detail refusals (`rejected_for_capacity` diffs `scene_geometry_admission_rejections()` around the LOD rings only).

## Evidence
- `accumulate_global_geometry`: `let new_v_len = self.pending_vertices.len() + vertices.len();` then `check_pool_growth(..., VERTEX_POOL_HARD_CAP, ...)` -> `scene_geometry_admission_closed = true` on `Err`.
- `app_frame.rs`: `rebuild_in_progress || (ctx.mesh_registry.is_geometry_dirty() && !defer_geometry_rebuild)`.
- `geometry_batch_in_progress`: `!self.pending.is_empty() || self.active_apply.is_some() || self.persistent_apply.is_some() || self.lod_reconcile_pending`.
- `plan_geometry_compaction` has no production caller besides `rebuild_geometry_ssbo`.
- Disproof attempted: no other compaction path mid-batch; nothing in `streaming/` or `cell_loader/` re-queues a loaded cell; the interior door path is not affected (latch cleared by a rendered frame before the interior apply).

## Impact
The pool's effective capacity at a crossing is `cap - (volume unloaded at that crossing)`, not `cap`; for a radius-12 grid that is one or two rows, about 4-8 % of the resident set. Whenever the live set sits inside that band (EXT-D6-2026-10-19-01 reports it reachable at the default radius on FNV and FO4), newly entered cells lose their uncached meshes or their ground until unloaded, invisibly to every smoke gate. Not measured (no engine launch).

## Related
EXT-D6-2026-10-19-01 (LOD-ring analogue; different fix site - the LOD providers' known-miss caching - filed separately), #6054 (closed, `Starved` verdict), #2678 (hole flag), `docs/engine/exterior-grid-streaming.md` "Geometry admission".

## Suggested Fix
Smallest change: before queueing loads at a crossing that unloaded cells, run the compaction (`plan_geometry_compaction` only mutates the CPU pools; the GPU copy can stay resumable) so the freed space is counted; alternatively count dead spans out of the admission test. Independently: record full-detail refusals (a `MeshRegistry` counter diffed per cell apply) and put a refused cell on a retry list re-armed by the admission closed -> open edge. Neither touches a barrier or layout.

Source: docs/audits/AUDIT_RENDERER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
