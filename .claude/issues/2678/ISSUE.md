# PERF-D3-02: compact_pending_geometry dirty gate permanently true after first mesh drop

**Issue**: #2678
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: MEDIUM
- **Dimension**: 3 — GPU Memory Pressure
- **Location**: [mesh.rs](crates/renderer/src/mesh.rs) — `MeshRegistry::compact_pending_geometry`, called unconditionally from `MeshRegistry::rebuild_geometry_ssbo`
- **Status**: NEW (no issue in the 400-entry baseline mentions `compact_pending_geometry`; the symbol appears in no other `.rs` or `.md` file in the tree)
- **Description**: The compaction fast-path tests `self.meshes.iter().any(|slot| slot.is_none())`.
  But `meshes` is a **grow-only** `Vec<Option<GpuMesh>>` whose dropped slots hold `None`
  *forever* by deliberate design — `release_mesh_ref` does `slot.take()` and the upload
  paths always `push` a fresh slot (handle stability, #372). So the moment the first
  mesh of any kind is dropped — the first cell unload, the first LOD swap — `any_dead`
  latches true for the rest of the process, and the "fast path: no holes → nothing to
  compact" branch is dead code from then on. Every subsequent `rebuild_geometry_ssbo`
  runs a full compaction pass even when nothing has been dropped since the previous one,
  in which case the pools already have no holes and the pass reproduces a byte-identical
  layout. The function's own doc comment states the opposite contract: *"Pure appends
  (no drops) skip this pass."*
- **Evidence**:
  ```rust
  fn compact_pending_geometry(&mut self) {
      // Fast path: no holes → nothing to compact.
      let any_dead = self.meshes.iter().any(|slot| slot.is_none());
      if !any_dead { return; }
      let mut new_vertices: Vec<Vertex> = Vec::with_capacity(self.pending_vertices.len());
      let mut new_indices:  Vec<u32>    = Vec::with_capacity(self.pending_indices.len());
      … per-live-mesh extend_from_slice over the whole pool …
      self.pending_vertices = new_vertices;
      self.pending_indices  = new_indices;
  }
  ```
  and the slot-never-reused invariant it collides with:
  ```rust
  // MeshRegistry::drop_mesh doc
  /// Handles stay stable: the dropped slot holds `None` forever.
  ```
- **Impact**: CPU-side, per global-SSBO rebuild — i.e. per cell load once streaming is
  running. Two fresh allocations sized to the whole live pool plus a scattered
  per-mesh copy of it, then a free of the old pair. `memory-budget.md` puts the typical
  resident vertex/index pools at **~208 MB** (soft cap 4 M vertices ≈ 416 MB at the
  104 B stride); that is a transient ~2× host-RAM spike and a multi-hundred-MB
  scattered copy on a single thread at each boundary crossing, on a machine where a
  CPU bottleneck is a bug by policy. No GPU correctness impact — the compaction result
  is correct, just redundant. No quantitative guard exists for this site (dhat covers
  the NIF parse path only, per the skill's alloc-bound note), so the cost is reasoned
  from the pool ceilings rather than measured.
- **Related**: `check_pool_growth` soft/hard caps (verified intact, below); the
  batched-teardown work in `app_step.rs::step_streaming` that already de-duplicated
  the *global* `shrink_storages`/`shrink_blas_scratch_to_fit` passes per boundary —
  this is the same class of "global pass repeated when its precondition no longer
  holds" that batching closed elsewhere.
- **Suggested Fix**: Replace the `any_dead` scan with a `geometry_has_holes: bool` flag
  set in `release_mesh_ref` when `was_scene_mesh` is true and cleared at the end of
  `compact_pending_geometry`. `geometry_dirty` cannot be reused — appends set it too.

---


---
*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `PERF-D3-02`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or a bench delta vs the checked-in baseline) pins this fix
