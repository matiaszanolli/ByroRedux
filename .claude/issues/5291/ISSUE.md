# #5291: PERF-D3-2026-10-05-01: The Starfield CDB `MaterialIndex` is built lazily on the main thread inside the material merge (~2 s, ~470 MB peak per CDB), unbudgeted and missing from `memory-budget.md`

**Labels**: medium,performance,memory,game:starfield,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5291

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-05.md` — `PERF-D3-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure (CPU-side material memory) / Streaming
- **Location**: `byroredux/src/asset_provider/material/cdb.rs:102-167` (`cdb_material_index`, `lookup_cdb_material`); callers `asset_provider/material/merge.rs:482,589,700,1372`; `docs/engine/memory-budget.md:9-19`
- **Status**: NEW. It arrived with `224a19372` (#3398 Phase 2) and `18fce7e43`/`978d25c19`. #5210 covers the CDB Phase-2 *spec* rot in `nifal.md`, not cost or memory. #3398 is the feature tracker.
- **Description**: on the first `.mat` lookup that reaches a given CDB, `cdb_material_index` does three things:
  1. re-opens the archive (`Archive::open(source)`);
  2. extracts the 105 MB CDB;
  3. runs `MaterialIndex::build`. `224a19372` measured this at about 2 s and about 470 MB peak.
  
  Every caller holds `&mut MaterialProvider`, so the build runs on the main thread:
  - `merge_external_material` ← `nif_import_registry::merge_external_materials` (in `finish_partial_import`, the streamed apply);
  - `spawn/mesh_instance.rs`;
  - `scene/nif_loader.rs`.
  
  `lookup_cdb_material` walks sources `.rev()` (DLC/Creation CDBs first) and builds each index lazily. A base-only material, or a path in no CDB, first seen during a streamed apply therefore builds the next CDB inside the apply slice. `FrameTimeBudget` cannot preempt that. With the SFBGS007 near-copy (500,385 keys), two indices can end up resident. Their resident size is unmeasured: the index is std `HashMap`s of per-object `Vec`/`String` (`crates/sfmaterial/src/index.rs:120-139`). `memory-budget.md` still says production "does not retain inflated CDB blobs" and documents only the `probe_header` cache.
- **Evidence**: `cdb.rs:111-129` (build outside any worker); `cdb.rs:155-167` (lazy per-source walk).
- **Impact**:
  - A multi-second main-thread stall on the first Starfield `.mat` merge. At boot it lands on top of the ~7.7 s ESM parse, which it could overlap.
  - A further stall at whichever later streamed apply first reaches an un-built CDB.
  - Resident CPU memory missing from the budget the RT VRAM/RAM plan is checked against.
- **Related**: #3398, #5210, the `870ea1d07` concurrent-build race fix (now only a test-harness concern, since production is main-thread only), #2705.
- **Suggested Fix**: start each discovered CDB's index build on a worker at discovery time (the stream pool or a rayon task, overlapping the ESM parse) and publish it through the existing cache. Measure peak and resident bytes with the real-data gate and add a row to `memory-budget.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
