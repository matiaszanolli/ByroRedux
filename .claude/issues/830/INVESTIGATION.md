# Investigation — #830

**Domain**: binary (streaming worker) + nif (parse path)

## Code path

`pre_parse_cell` (`byroredux/src/streaming.rs:239-312`) — note the function name in the issue body is wrong; it's `pre_parse_cell` not `extract_and_parse_cell`. Loop at L276-304 is exactly as described.

## Thread-safety verification

- `TextureProvider` (`byroredux/src/asset_provider.rs:49`) holds `Vec<Archive>`. No interior mutability outside the `Mutex<File>` inside each archive. Auto-derived `Send + Sync`.
- `Archive::Bsa(BsaArchive)` and `Archive::Ba2(Ba2Archive)` — both wrap `Mutex<File>` per the audit pre-flight. Concurrent `extract_mesh` from rayon workers serializes at the file mutex (one File handle = serial I/O by hardware necessity), but parse + import dominate the cost, so the file mutex doesn't kill the win.
- All four import functions (`extract_bsx_flags`, `import_nif_lights`, `import_nif_particle_emitters`, `import_embedded_animations`) take `&NifScene` and return owned data — no globals, no thread-local state.
- `parse_nif` takes `&[u8]` and returns `io::Result<NifScene>` — pure CPU.

## Approach

Collect `HashSet<String>` → `Vec<String>` (rayon's `IntoParallelIterator` is best-supported on Vec), then `into_par_iter().map(closure).collect()`. The closure does extract + parse + 4 import calls, returns `(path, Option<PartialNifImport>)`. Extend `parsed` with the results.

The closure must move `tex_provider: &TextureProvider` and `&str` slices into each task — fine, both are `Sync`.

Logging from rayon workers is fine — `log::warn!`/`log::debug!` are thread-safe.

## Caller context

`pre_parse_cell` is called from `worker_thread_main` (single OS thread). Adding rayon means we spawn rayon's pool from inside that single worker — rayon's global pool is shared across the process, so the worker thread becomes the "submitter" and rayon worker threads do the parse work in parallel. No additional thread setup needed.

## Scope

1 file: `byroredux/src/streaming.rs`. Existing `rayon = { workspace = true }` dep already in `byroredux/Cargo.toml`. No new deps.

## Test strategy

The function takes a `TextureProvider` populated from real BSA/BA2 archives — hard to mock in unit tests without inventing fake archive readers. Existing streaming integration tests in `streaming_tests.rs` (if any) cover correctness; the parallelism is a perf change with no observable behavior difference. Will add a small unit test that exercises the new closure path with a mock provider if a `MeshResolver` trait already exists, otherwise verify by running existing tests.
