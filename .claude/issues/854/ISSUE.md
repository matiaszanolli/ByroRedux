## Source Audit
`docs/audits/AUDIT_CONCURRENCY_2026-05-05.md`

## Severity / Dimension
MEDIUM / Worker Threads (Streaming, Debug)

## Location
`byroredux/src/streaming.rs:210-230` (worker loop), `byroredux/src/streaming.rs:285-314` (rayon parallel parse), `byroredux/src/main.rs:545-559` (request dispatch)

## Description
**Trigger Conditions**: A NIF in a freshly-loaded cell hits a code path in `byroredux_nif::parse_nif`, `extract_bsx_flags`, `import_nif_lights`, `import_nif_particle_emitters`, or `import_embedded_animations` that panics. The panic propagates up through the rayon `into_par_iter().map(...).collect()` (rayon-style: panics in worker tasks resurface in `collect()`), then up through `pre_parse_cell` and `cell_pre_parse_worker`. The `JoinHandle<()>` is held in `WorldStreamingState.worker` but is never observed — it's `#[allow(dead_code)]` at line 157 with a comment "nothing currently calls `.take().join()`."

When the worker panics, `request_rx` (the receiver) is dropped. The next `step_streaming` tick on the main thread tries `state.request_tx.send(req)` (`main.rs:552`). The send returns `Err` and the main thread logs `"Streaming worker channel closed; cell ({},{}) cannot be loaded"`, removes the pending entry, and continues — but no new worker is spawned, no fatal error is propagated, and every subsequent cell-crossing produces the same warning while the world streaming silently stops working. Looking at the code there is no catch_unwind around `pre_parse_cell`, no panic-hook installed for the worker, and no health check on the `JoinHandle::is_finished()` state.

## Evidence
```rust
// streaming.rs:210-230 — no catch_unwind, no panic recovery
fn cell_pre_parse_worker(
    request_rx: mpsc::Receiver<LoadCellRequest>,
    payload_tx: mpsc::Sender<LoadCellPayload>,
) {
    while let Ok(req) = request_rx.recv() {
        let payload = pre_parse_cell(...);  // ← panic here = thread death
        if payload_tx.send(payload).is_err() { break; }
    }
}

// main.rs:552-559 — main thread observes Err and logs, but doesn't re-spawn
if state.request_tx.send(req).is_err() {
    log::error!(
        "Streaming worker channel closed; cell ({},{}) cannot be loaded",
        gx, gy
    );
    state.pending.remove(&(gx, gy));  // ← world keeps running, streaming dead
}
```

## Impact
One bad NIF (or one bug in NIF parser code that triggers on vanilla content for a specific game) silently kills exterior streaming for the whole session. The player can keep playing — but no new cells load, and existing cells don't unload (because unload happens off the same diff loop that's now no-oping). Particularly painful in CI where a single panic in a parser regression test could be misattributed to "streaming worked, just nothing rendered" instead of "the worker died on cell 3". In production a transient `unwrap()` regression in any of ~30 NIF block parsers immediately bricks all subsequent cell loads.

## Suggested Fix
Wrap `pre_parse_cell` in `std::panic::catch_unwind(AssertUnwindSafe(|| pre_parse_cell(...)))` and emit an empty `LoadCellPayload` with a logged warning when the closure panics. Alternatively, use `JoinHandle::is_finished()` in `step_streaming` to detect worker death and re-spawn (more invasive but catches resource-exhaustion panics too). The catch_unwind path is the lightweight option and matches the per-NIF error contract already in place.

## Related
The NIF parser already does graceful error recovery for malformed blocks (the `parsed: HashMap<String, Option<...>>` shape carries per-NIF None failure markers). The panic case is the unhandled path.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
