# #5512: CONC-D7-2026-10-09-02: The worker's skip filters can skip a key the main thread will not have at apply time

**Labels**: bug, concurrency, low, performance

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-09.md` — finding `CONC-D7-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. This is a performance issue only: the main-thread parse fallback (`references/synth_child.rs:633-650`)
  still loads the model.
- **Dimension**: Worker Threads (worker ↔ main handoff)
- **Location**:
  - `byroredux/src/streaming/pre_parse.rs:135-145` (memo scope comment), `:183-202` (`recv_next_batch_request`), `:841`
    (`batch_keys.extend`).
  - Stale drops: `byroredux/src/streaming_helpers.rs:795-806` and `:940-953`.
  - `byroredux/src/cell_loader/nif_import_registry.rs:420-426` (2048-entry LRU) and `:632-635` (`snapshot_keys`).
- **Status**: NEW. It is the residual of #3670's memo (closed). The closed #3670 and #4207 match the area; neither covers a
  memo entry outliving a dropped payload.
- **Description**: The worker skips a model key in two cases:
  1. The key is in the request's `cached_keys` snapshot.
  2. An earlier request in the same "dispatch batch" already emitted it (`batch_keys`, #3670).

  Both assume the key will be in `NifImportRegistry` when this cell applies. Neither holds under the conditions where
  payloads go stale:
  - **The memo is cleared only when the queue empties.** Under a sustained backlog (cold fill plus continued movement) it
    spans several dispatches. The main thread drops a stale payload before any cache mutation, so a key emitted only in a
    dropped payload never reaches the registry. Two later requests then miss it:
    - another cell sharing the model (shared statics are the norm);
    - the same coordinate re-requested at a newer generation.

    Both skip the key as `BatchDuplicate`, and their applies hit a cache miss.
  - **`snapshot_keys` can name a key the LRU has since evicted.** The registry's 2048-entry LRU can evict a snapshot key
    before the request applies, as other payloads' `FinishImports` insert entries. `snapshot_keys`' doc says the snapshot
    "never under-skips"; that is true only while nothing is evicted.

  The comment at `pre_parse.rs:135-140` claims the memo "prevents a later, independent crossing from losing a needed
  payload to an old memo entry". That holds only when the queue drains between dispatches.
- **Impact**: A main-thread NIF parse inside the 4 ms apply budget: a frame hitch on exactly the fast-travel / cold-fill
  frames the worker exists to protect. No correctness loss.
- **Related**: #3670, #862, #4207, CONC-D7-2026-10-09-01.
- **Suggested Fix**: On a stale drop, still `finish_streaming_import` the payload's parsed entries. They are valid imports;
  only the cell spawn is stale. That makes the memo's premise true and also salvages the work. Alternatively, clear the
  memo per request when the request's own generation is superseded. For the LRU half, count a skipped-but-missing key in
  `StreamingTelemetry`, so `mesh.cache` evictions during a fill become visible.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
