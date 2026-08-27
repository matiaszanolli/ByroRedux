# SAFE-2026-08-27-01: `#3298`'s chunked geometry rebuild publishes compacted mesh offsets against the pre-compaction GPU buffer — BLASes built in that window bake wrong geometry

- **Issue**: [#3372](https://github.com/matiaszanolli/ByroRedux/issues/3372)
- **Finding ID**: `SAFE-2026-08-27-01`
- **Source report**: `docs/audits/AUDIT_SAFETY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `critical,renderer,memory,safety,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3372 --json state`.

---

- **Severity**: CRITICAL
- **Dimension**: 3 (leaks / resource lifecycle) + 5 (Vulkan / AS correctness)
- **Location**: `crates/renderer/src/mesh.rs:1112-1178` (`rebuild_geometry_ssbo`), `:1129` (the `compact_pending_geometry()` call), `:964-1004` (`compact_pending_geometry`), `:1349-1350` (deferred `ssbo_*_count` update), `:1487-1500` (`is_geometry_resident`); consumers at `byroredux/src/app_frame.rs:205-235`, `crates/renderer/src/vulkan/context/resources.rs:307-333`
- **Status**: NEW. **Regression introduced by `ae7179a3` (Fix #3298, 2026-08-25)**. Issue #3298 is CLOSED; nothing in `/tmp/audit/issues.json` covers this consequence.
- **Description**:

  `compact_pending_geometry` does two things at once: it squeezes dropped
  meshes' spans out of `pending_vertices` / `pending_indices`, **and it
  rewrites every surviving mesh's `global_vertex_offset` /
  `global_index_offset` in place** (`mesh.rs:990-998`). Those offsets are the
  live values the draw path and the BLAS builder read.

  Before `#3298`, `rebuild_geometry_ssbo` called `compact_pending_geometry()`
  and then built the replacement buffer **synchronously in the same call**, so
  the offsets and the bound buffer were never out of step across a frame
  boundary — the window was zero frames.

  `#3298` kept the compaction at the top of `rebuild_geometry_ssbo`
  (`mesh.rs:1129`) but moved the *upload* into a resumable state machine:
  `advance_geometry_rebuild` copies one bounded chunk per call, and one call
  happens per frame. `global_vertex_buffer` / `global_index_buffer` keep
  serving every draw **unchanged** until swap-in — which is by design and is
  the point of the change — and `ssbo_vertex_count` / `ssbo_index_count` are
  likewise only updated at swap-in (`mesh.rs:1349-1350`).

  The result is a window of **at least two frames** (the vertex phase and the
  index phase never share a call — `mesh.rs:1230-1234`), and up to ~15 frames
  at the FO4 boundary-crossing sizes the change was written for (~600 MiB /
  `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB), during which:

  - mesh offsets describe the **compacted** layout, and
  - the bound global buffer holds the **uncompacted** bytes.

  Nothing suppresses drawing in that window. `is_geometry_resident` is the only
  gate, and it cannot catch this: it compares the new (compacted, therefore
  *smaller*) offsets against the old (uncompacted, therefore *larger*)
  `ssbo_*_count`, so it answers `true` for every mesh
  (`mesh.rs:1497-1499`).

  The precondition is `geometry_has_holes` — i.e. any scene mesh dropped since
  the last compaction (`mesh.rs:944-950`). That is precisely what a cell unload
  does, so the hazard fires on the same boundary crossings `#3298` targets, not
  on an exotic path.
- **Evidence**:

  Pre-`#3298` (`git show ae7179a3^:crates/renderer/src/mesh.rs:1004-1016`) —
  compaction and build in one call:
  ```rust
  pub fn rebuild_geometry_ssbo(&mut self, …) -> Result<()> {
      // If any scene meshes were dropped since the last build, compact
      // the pending buffers and rewrite every live mesh's offsets.
      self.compact_pending_geometry();
      …                       // ← synchronous build, same call
  ```

  Today (`crates/renderer/src/mesh.rs:1120-1165`):
  ```rust
      self.compact_pending_geometry();          // offsets rewritten NOW
      …
      Ok((new_vertex_buffer, new_index_buffer)) => {
          self.geometry_rebuild = Some(GeometryRebuildInProgress { … });
          return self.advance_geometry_rebuild(…);   // ← one chunk, then return
      }
  ```
  and the counts that gate residency only move at swap-in
  (`crates/renderer/src/mesh.rs:1345-1350`):
  ```rust
      self.global_vertex_buffer = Some(job.new_vertex_buffer);
      self.global_index_buffer  = Some(job.new_index_buffer);
      self.geometry_generation  = self.geometry_generation.wrapping_add(1);
      self.ssbo_vertex_count    = job.target_vertex_count;
      self.ssbo_index_count     = job.target_index_count;
  ```

  The BLAS builder pairs the *current* buffer with the *current* offsets, with
  no generation check between them
  (`crates/renderer/src/vulkan/context/resources.rs:317-322`):
  ```rust
      (None, None) => (
          global_vertex_buffer?,                                   // OLD generation
          global_index_buffer?,                                    // OLD generation
          u64::from(mesh.global_vertex_offset) * vertex_stride,    // NEW (compacted)
          u64::from(mesh.global_index_offset)  * index_stride,     // NEW (compacted)
      ),
  ```
  and `byroredux/src/app_frame.rs:235` calls
  `ctx.restore_missing_static_blas_for_draws(&self.draw_commands)`
  **every frame**, unconditionally — including every frame of the window — so a
  cell whose meshes need first-sight BLASes during a boundary crossing will
  build them from the wrong byte ranges.

  Attempts to disprove, all failed:
  - *Existing BLASes are fine* — true, and irrelevant: a BLAS bakes its
    geometry at build time, so entries built before compaction still describe
    the correct triangles. The bug is confined to builds **inside** the window.
  - *Maybe the residency gate filters the draws* — no, see above; it returns
    `true` because it mixes new offsets with old counts.
  - *Maybe `geometry_dirty` suppresses the frame* — no.
    `app_frame.rs:219` uses `is_geometry_dirty()` only to run the residency
    filter, which passes.
  - *Maybe compaction rarely runs* — `#2678` made it run **only** when a scene
    mesh was dropped, which is exactly the streaming case.
  - *Maybe the atomic fallback is what actually runs* —
    `rebuild_geometry_ssbo:1145` tries the chunked path first and only falls
    back when the second full-size allocation fails.
- **Impact**:
  - **Acceleration structures built with wrong geometry** (severity table:
    CRITICAL) for any mesh whose first-sight BLAS build lands inside the
    window — shadows, reflections and GI trace against triangles that belong to
    a different mesh.
  - Raster draws in the window fetch vertices/indices from the wrong offsets:
    visibly scrambled geometry for every live mesh past the first hole, for 2
    to ~15 frames at every exterior boundary crossing that unloaded a cell.
  - Blast radius is every game and every streaming path; it does not depend on
    plugin data.
  - Not GPU-out-of-range (compacted offsets are ≤ the old counts), so it is
    corruption rather than a device fault — which is precisely why it can hide.
- **Related**: #3298 (the change that introduced it), #2678 (the
  `geometry_has_holes` gate that makes compaction conditional), #2743
  (`geometry_generation`, the existing mechanism a fix could reuse), #2374 (the
  atomic fallback path, which is unaffected).
- **Suggested Fix**: Keep the two halves of compaction apart from the copy.
  Simplest correct option: have `is_geometry_resident` return `false` for every
  scene mesh while `geometry_rebuild_in_progress()` **and** the rebuild's
  snapshot was taken after a compaction — the meshes then stay out of raster
  and TLAS for the window instead of rendering wrong (a pop, not corruption).
  Cleaner option: snapshot the pre-compaction offsets into
  `GeometryRebuildInProgress` and only publish the compacted offsets at
  swap-in, alongside the `geometry_generation` bump. Either way, add a
  regression test pinning that compacted offsets are never observable while
  the old buffer is bound.

---
## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_SAFETY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `SAFE-2026-08-27-01`._
