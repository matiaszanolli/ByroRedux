# #6233: CONC-2026-10-21-01: Terrain-height jobs abort the process on any panic

**Labels**: medium,concurrency,terrain-exterior,bug

**Source**: docs/audits/AUDIT_CONCURRENCY_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Worker Threads & Thread-Safety Bounds (Dim 7) · **Location**: `byroredux/src/terrain_height.rs:53-57`

- **Location**: `byroredux/src/terrain_height.rs:53-57` (pool built with no `panic_handler`), `:91-97` (`self.pool.spawn(move || { let result = run_job(&job); ... })`); `byroredux/src/asset_provider/terrain_height.rs:113-205` (`run_job` → `extract_first` → `Rgba8::decode_dds` → `th::from_*` → `write_atomic`)
- **Status**: NEW. The pool is from #6195 (5017a64c2, 2026-10-20). Related: #6119 (closed, the same class on the pre-parse extractor) and #5665 (open, the `sf-cdb-index` worker has no `catch_unwind`).
- **Description**: `rayon::ThreadPool::spawn` gives a panicking job to the pool's panic handler. Without one, rayon aborts the process (`rayon-core-1.13.0/src/registry.rs:377`: "If there is no handler, or if that handler itself panics, then we abort"). The job does exactly the work the other workers wrap in `catch_unwind`:
  - archive extraction over untrusted BSA/BA2 bytes. #6119 exists because a reader panic is a real class, so `pre_parse.rs` now guards `extract_guarded` (`:393`) and `parse_one_nif` (`:276`).
  - the texture prefetch (`texture_prefetch.rs:227`) wraps `provider.extract` for the same reason.
  - the job also decodes a DDS and runs the FFT integrator on whatever dimensions the decoder returns (capped at 8192², non-zero).

  The workspace deliberately keeps `panic = "unwind"` in release (`Cargo.toml:329-346`, FFI-03 / #1383), so this one worker is the exception that turns a recoverable panic into an abort.
- **Evidence**: `grep -n 'catch_unwind\|panic_handler' byroredux/src/terrain_height.rs byroredux/src/asset_provider/terrain_height.rs` returns nothing. `grep -rn panic_handler byroredux/src crates/*/src` returns nothing.
- **Trigger Conditions**: an exterior cell whose LTEX layer (authored height, normal, or diffuse texture) comes from an archive or loose file that makes the reader, decoder or generator panic. A mod-supplied texture is enough. The request fires at terrain spawn (`acquire_spawn_heights` / `request_heights`), so the abort happens while a cell streams in.
- **Impact**: the whole engine aborts with no unwinding, so no `Drop`, no log flush, and no save ring flush. A guarded path would log one `Failed` layer that renders without height.
- **Related**: #6119, #5665, #6195 (feature issue, open).
- **Suggested Fix**: wrap the job body in `std::panic::catch_unwind(AssertUnwindSafe(|| run_job(&job)))` and push `Err("height job panicked")`, so the slot completes and the state becomes `Failed`. Optionally also set a logging `panic_handler` on the builder. Add a test that spawns a panicking job and asserts `drain` reports it (pure CPU, mirrors `a_permit_moved_into_a_panicking_task_is_released`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
