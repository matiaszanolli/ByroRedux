# #6290: CONC-2026-10-21-02: Concurrent same-key height jobs share one temp file

**Labels**: low,concurrency,terrain-exterior,bug

**Source**: docs/audits/AUDIT_CONCURRENCY_2026-10-21.md · **Severity**: LOW · **Dimension**: Worker Threads (Dim 7), the on-disk cache write the skill names as "the shared state to check" · **Location**: `byroredux/src/asset_provider/terrain_height.rs:201-205`

- **Location**: `byroredux/src/asset_provider/terrain_height.rs:201-205` (`write_atomic`: `let tmp = path.with_extension(format!("tmp{}", std::process::id()))`), `:155-197` (`cached_or_generate`); dedup in `byroredux/src/terrain_height.rs:72-98`
- **Status**: NEW
- **Description**: jobs are deduplicated per `LayerHeightSources` (authored / normal / diffuse path triple), but the cache file is keyed by `(GENERATOR_VERSION, tier, green, source bytes)`. Two distinct triples can resolve to the same key. Examples: two LTEX records that share a normal map but differ in diffuse, or byte-identical textures under different archive paths, which vanilla data ships. The pool has two workers (`WORKER_THREADS = 2`), so both jobs can miss the cache and run `write_atomic(&dds_path, …)` at once. The temp name carries only the pid, so both threads write the same temp path:
  1. A writes `tmp` and renames it onto `<key>.dds`.
  2. B had already opened `tmp` (`File::create` truncates the same inode) and is still writing.
  3. B's `rename(tmp, path)` fails with `ENOENT`. B's sources become `HeightState::Failed` for the rest of the session ("no height map for …"), so that layer never gets its map.
  4. Meanwhile A has published `<key>.json` and returned `Ready`. `apply_ready_heights` can `std::fs::read` the `.dds` while B is mid-write after its truncate, so it reads a short file. `enqueue_dds…` then fails and the lane stays 0 for that tile.
- **Evidence**: by reading. The three steps above need only two in-flight jobs with equal keys. No test makes two keys collide.
- **Impact**: a terrain layer intermittently renders flat (no height blend) for the session, depending on thread timing. The cache on disk is whole afterwards (both writers write identical bytes), so the next session is fine.
- **Suggested Fix**: make the temp name unique per write (pid plus a process-wide `AtomicU64` counter, or `tempfile::NamedTempFile::new_in(cache_dir)` then `persist`). Optionally deduplicate in-flight work by cache key as well as by source triple.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
