**Severity**: MEDIUM · **Dimension**: Streaming & Cells · **Status**: NEW
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F6)

## Description
The Starfield CDB is parsed once and held behind `Arc<ComponentDatabaseFile>` (`byroredux/src/asset_provider.rs:680,733`), an O(1) `is_some()` check per material. Its FO4 analog is not cached: `spawn_precombined_meshes` calls `open_geometry_csg(plugin_path)` on every cell with precombines (`byroredux/src/cell_loader/precombined.rs:82` — the comment even reads "open the shared-geometry CSG once per cell load"), reached per cell from `cell_loader/exterior.rs:342` (exterior) and `cell_loader/load.rs:236` (interior). Each call re-opens the file (`CsgArchive::open`, `crates/bsa/src/csg.rs:103-153`), re-reads + parses the chunk table (a ~240 MB vanilla blob has ~3700 chunks ⇒ ~30 KB read + ~3700-entry `Vec<ChunkEntry>`), and constructs a fresh `ChunkCache` — whose whole purpose is to amortise zlib inflate *within* a load. Dropping the archive at function end discards the warm cache, so adjacent tiles sharing PSG regions re-inflate the same 64 KiB chunks.

## Evidence
Verified live: `precombined.rs:82` calls `open_geometry_csg(plugin_path)` unconditionally per `spawn_precombined_meshes` invocation; no `Arc`/provider-level cache exists for the CSG (contrast `asset_provider.rs` `sf_cdb` which is `Arc`-held).

## Impact
FO4 exterior streaming only — precisely the title where precombines are 100% of vanilla architecture. Per-cell main-thread file open + chunk-table parse + loss of all inter-cell zlib-chunk reuse. Compounds F7 (runs inside the unbounded drain).

## Related
F7 (runs inside the unbounded payload drain); #1446 (CSG doc-rot only — unrelated).

## Suggested Fix
Hold `Option<Arc<CsgArchive>>` on `MaterialProvider` (or a small `CsgProvider` keyed by plugin stem, mirroring `sf_cdb`), resolve lazily on first precombine cell, pass the `Arc` into `spawn_precombined_meshes`. `CsgArchive` is already `Send`/`Sync`-friendly (inner `Mutex<File>` + `Mutex<ChunkCache>`).

## Completeness Checks
- [ ] **SIBLING**: Mirror the `sf_cdb` `Arc` caching pattern exactly; confirm no other per-cell archive re-open exists on the precombine path
- [ ] **TESTS**: Pin that the CSG archive is opened once across N precombine cell-loads (cross-cell cache reuse)
