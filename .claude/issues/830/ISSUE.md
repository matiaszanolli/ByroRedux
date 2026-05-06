# NIF-PERF-06: streaming pre-parse worker is single-threaded — saturates 1 of 32 cores

## Description

`extract_and_parse_cell` (the streaming worker's per-cell entry point) iterates `model_paths` with a plain `for path in model_paths` loop, calling `byroredux_nif::parse_nif`, `import_nif_lights`, `import_nif_particle_emitters`, and `import_embedded_animations` for each model serially. The worker is one OS thread, so the entire pre-parse phase saturates exactly **1 of 32 hardware threads** on the dev machine (Ryzen 7950X) — the other 31 cores idle while the player waits for an exterior cell to stream in.

Parsing of distinct NIF files is **embarrassingly parallel**: each call takes a `&[u8]` slice (extracted bytes) and produces an owned `NifScene` with no shared mutable state, no I/O after the byte extraction, and no global state mutation. The workspace already depends on `rayon = "1"`.

## Location

`byroredux/src/streaming.rs:276-304`

## Evidence

```rust
for path in model_paths {
    let Some(bytes) = tex_provider.extract_mesh(&path) else { ... };
    let scene = match byroredux_nif::parse_nif(&bytes) { ... };           // CPU-bound
    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    let lights = byroredux_nif::import::import_nif_lights(&scene);
    let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
    let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
    parsed.insert(path, Some(PartialNifImport { scene, bsx, lights, particle_emitters, embedded_clip }));
}
```

## Pre-flight: thread-safety verified

- `TextureProvider::extract_mesh` is `&self` (`asset_provider.rs:84`)
- `BsaArchive` (`crates/bsa/src/archive.rs:119`) wraps its `File` in `Mutex<File>` → `Send + Sync`
- `Ba2Archive` (`crates/bsa/src/ba2.rs:78`) same pattern → `Send + Sync`

Concurrent `extract_mesh` calls will serialize on the file mutex (one File handle = serial I/O by hardware necessity), but the much-larger parse + import work parallelizes fully across the rayon pool.

## Impact

For an exterior cell with 100 unique models averaging 3 ms each, serial parse takes ~300 ms on one core. Parallelizing across 8 worker threads (rayon's default) drops it to ~40-50 ms — a **6-7× speedup** on cell-streaming latency. Especially impactful for FNV/SE exterior radius=3 / =5 grids where 30+ cells stream as the player runs.

## Suggested Fix

Replace the for-loop with rayon parallel iter:

```rust
use rayon::prelude::*;
let results: Vec<(String, Option<PartialNifImport>)> = model_paths
    .into_par_iter()
    .map(|path| {
        let entry = (|| -> Option<PartialNifImport> {
            let bytes = tex_provider.extract_mesh(&path)?;
            let scene = byroredux_nif::parse_nif(&bytes).ok()?;
            let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
            let lights = byroredux_nif::import::import_nif_lights(&scene);
            let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
            let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
            Some(PartialNifImport { scene, bsx, lights, particle_emitters, embedded_clip })
        })();
        (path, entry)
    })
    .collect();
parsed.extend(results);
```

For peak throughput (V2 follow-up): lift `extract_mesh` into a serial pre-pass that gathers all bytes, then run the par_iter on the (path, bytes) pairs — this hides I/O latency behind the previous cell's parse work and avoids holding the BSA mutex on parse-bound rayon workers.

## Telemetry follow-up

NIF parse has no equivalent of `ScratchTelemetry` today. After parallelizing, add a `NifParseTelemetry { cells_streamed, models_parsed, parallel_efficiency }` resource so worker-thread saturation is observable.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `cell_loader.rs` and `cell_loader_terrain.rs` for any other serial NIF-parse loops on a worker thread
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: BSA/BA2 file mutex is per-archive — concurrent calls on the same archive serialize, on different archives don't. Verify the texture provider doesn't cycle a global lock that would defeat parallelism.
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a streaming-throughput regression test (mock provider with 50 NIFs, assert wall-clock < N × single-NIF-time)

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` — NIF-PERF-06