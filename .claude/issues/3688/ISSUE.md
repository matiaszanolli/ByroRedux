# #3688 — PERF-D7-2026-08-30-03: the LOD-coverage and terrain-seam diagnostics recompute from scratch on every reconcile frame, including two O(n²) scans, for two console commands

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D7-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3688

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming_helpers.rs:128-130` (both calls, unconditional at the tail of `reconcile_lod_rings`), `:152-203` (`update_lod_coverage`), `:228-303` (`update_terrain_seam_stats`), `byroredux/src/cell_loader/lod_coverage.rs:53-111` (`find_overlaps`, `find_full_detail_overlaps`, `find_terrain_full_detail_overlaps`)
- **Status**: NEW
- **Description**: `reconcile_lod_rings` runs every frame while
  `state.lod_reconcile_pending` is set — the entire post-crossing settle
  window. Its last two statements run **unconditionally**, including on frames
  where the budget produced `attempted == 0` and nothing in `state.lod_blocks`
  / `state.object_lod_blocks` / `state.loaded` changed:
  - `update_lod_coverage` allocates four fresh `Vec`s of the resident key sets
    (`terrain_keys`, `object_keys`, `full_cells`, `terrain_keys_with_holes`),
    then runs `find_overlaps` **twice** — an all-pairs `O(n²)` rect scan
    (`lod_coverage.rs:55-62`) — plus two `O(lod_keys × full_cells)` scans, plus
    `resident_vwd_refr_cells`, which is a full `VisibleWhenDistant` query with a
    per-hit `GlobalTransform` lock (that per-entity-lock half is **#3142, OPEN —
    not re-filed here**).
  - `update_terrain_seam_stats` re-runs `check_seam` (33 height comparisons +
    33 normal-byte comparisons, `terrain_seam.rs:124`) over every adjacent
    resident pair — ~2 × `state.loaded.len()` pairs — against `LandscapeData`
    that is immutable for the worldspace's lifetime, so a pair's verdict cannot
    change until the resident set does.
- **Evidence**: `streaming_helpers.rs:128-130`:
  ```rust
  let complete = terrain_complete && object_complete && placement_complete;
  update_lod_coverage(world, state, complete);
  update_terrain_seam_stats(world, state);
  ```
  The only readers of the two resources they write are two `byro-dbg` console
  commands — `byroredux/src/commands/world_info.rs:491` (`LodCoverageStats`) and
  `:512` (`TerrainSeamStats`). Nothing in the render or streaming path consumes
  either; there is no `cfg`, env-var, or `--bench` gate.
  Ring size is set by the band ladder: `fBlockMaximumDistance = 250_000` BU
  (`lod_bands.rs:110`) over `EXTERIOR_CELL_UNITS`, with four levels
  (`LOD_LEVELS = [4, 8, 16, 32]`, `lod_bands.rs:86`) — so `terrain_keys` and
  `object_keys` are each in the low hundreds and `find_overlaps` is tens of
  thousands of rect tests per scheme per frame.
- **Impact**: Pure diagnostic overhead on the exact frames the settle-latency
  benchmark measures, so it inflates `StreamingTelemetry::lod_slices` and the
  *"Exterior LOD settled around (x, y) in N ms"* line it is supposed to be an
  observer of. It is the same shape as #3385 (a per-frame recompute of a value
  that only changes on a residency event) and #3389, both of which were
  accepted as worth fixing.
- **Related**: #3142 (OPEN — the `resident_vwd_refr_cells` per-entity lock,
  one component of this block); #3385 (the LOD-availability memo, same fix
  shape, LANDED); #3389 (`block_hole_mask`'s dead scan, LANDED).
- **Suggested Fix**: Gate both on a residency-change epoch — bump a counter in
  `stream_lod_blocks` / `stream_object_lod_blocks` / `stream_placement_lod_blocks`
  and in the `state.loaded` insert/remove sites, and skip both updaters when the
  epoch is unchanged since the last sample (`LodCoverageStats::settled` still
  needs the per-frame `settled` flag, which is a one-field write). The seam
  stats can go further: their input is `state.loaded`'s key set alone, so they
  only need recomputing on a boundary crossing.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
