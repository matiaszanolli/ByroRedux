# #6314: PERF-D3-01: Terrain height maps decode the whole source DDS on a cache hit, and the resident maps have no memory-budget row

**Labels**: low,performance,terrain-exterior,memory,bug

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-21.md · **Severity**: LOW · **Dimension**: GPU Memory Pressure / Streaming · **Location**: `byroredux/src/asset_provider/terrain_height.rs (run_job, cached_or_generate)`

- **Location**: `byroredux/src/asset_provider/terrain_height.rs:113-134` (`run_job`: `extract_first` → `Rgba8::decode_dds` before `cached_or_generate`), `:156-171` (the cache lookup needs only `bytes`, not `image`); `crates/terrain-height/src/lib.rs:30-34` ("a 1024² BC4 map with mips is 0.67 MiB, and a resident exterior carries a few dozen distinct layers"); `docs/engine/memory-budget.md` (no row).
- **Status**: NEW (`5017a64c2`, `2975d2906`, 2026-10-20).
- **Description**: `cache_key` hashes the raw source bytes, so a warm-cache job could stop at the key and the metadata read. Instead every first request per session also fully decodes the source texture (up to 2048² BC → RGBA8) on the two-thread `byro-terrain-height` pool. The decode is off the main thread; it only delays the height lane (bound later by `apply_ready_heights`). Separately, a few dozen 0.67 MiB BC4 maps (~20–30 MiB) are a new resident texture class, but memory-budget.md has no row for them.
- **Evidence**: code read; size from the crate's own constant.
- **Suggested Fix**: compute the key from the extracted bytes and return the cached entry before `decode_dds`. Add a memory-budget.md row for the per-layer height maps.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
