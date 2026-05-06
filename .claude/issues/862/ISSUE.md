## FNV-D3-NEW-03: streaming worker bypasses NifImportRegistry — re-parses cached NIFs every cell

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 3 MEDIUM

## Severity / Dimension
MEDIUM / Cell loading × M40 streaming — perf cliff for Phase 2 doorwalking

## Location
`byroredux/src/streaming.rs:288-361` (`pre_parse_cell`)
`byroredux/src/cell_loader.rs:1550-1614` (`finish_partial_import` overwrites cache)

## Description
The cell-stream worker has no access to `World` and therefore cannot read `NifImportRegistry`. It blindly extracts every unique model path in the cell from the BSA archives, parses every one through `parse_nif`, runs `import_nif_lights` / `import_nif_particle_emitters` / `import_embedded_animations` for every one, and ships the entire `LoadCellPayload` back to the main thread. The drain (`finish_partial_import`) then `reg.insert(cache_key, Some(cached))` — **overwriting** any pre-existing cache entry produced by an earlier cell's REFR walk or by an earlier streaming payload.

```rust
// streaming.rs:317-360 — runs into_par_iter over the full unique-path
// set with no membership check. There is no Arc<Mutex<HashMap<…>>> or
// atomic snapshot of the registry available to the worker.
let results: Vec<...> = unique_paths.into_par_iter().map(|path| {
    // parse + light-extract + emitter-extract + clip-extract every path
    ...
}).collect();
```

## Impact
When the player walks across a 7×7 grid in WastelandNV, the worker pays the full BSA-extract + NIF-parse cost for every cell crossing, **even though >95% of the static furniture (rocks, roadway, junkpiles) is already in `NifImportRegistry`**. The `mesh.cache` debug counters will under-report the lifetime hit rate because the streaming side never increments `reg.hits` — it only inserts.

On a 7950X this still completes in ~40-50 ms per cell (per #830), but the rayon workers are doing redundant parses, and the post-finish `Arc` replacement compounds with the `AnimationClipRegistry` leak (see FNV-D3-NEW-04).

**M40 Phase 2 hot-reload (scripted teleport) gets punished worst** — every "scripted teleport back to where you were" pays the full re-parse cost despite all NIFs being cached.

## Suggested Fix
Pass a snapshot `Arc<HashSet<String>>` of currently-cached keys into the worker request alongside `wctx`, and skip extraction for paths that are already present. Or, more safely, snapshot through a parking-lot `Arc<RwLock<HashSet<String>>>` co-owned by `WorldStreamingState` and refreshed on the main thread after each `finish_partial_import` insert.

## Related
- `FNV-D3-NEW-04` — `AnimationClipRegistry` leak compounds with this (each worker re-parse leaks an unused clip handle)
- `FNV-D3-NEW-05` — `finish_partial_import` re-runs `convert_nif_clip` (downstream of this)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Initial-radius streaming path (`scene.rs::stream_initial_radius`) also dispatches to the same worker — verify it picks up the same membership check
- [ ] **DROP**: N/A — Arc snapshot lifecycle is straightforward
- [ ] **LOCK_ORDER**: New shared `RwLock<HashSet>` — pick TypeId-stable name; verify no system holds it across other ECS locks
- [ ] **FFI**: N/A
- [ ] **TESTS**: Streaming integration test — load cell A, walk to cell B (sharing 90% of statics), assert `NifImportRegistry.misses` increases by ≤ unique-new-models-in-B count (not by total-unique-models-in-B)
