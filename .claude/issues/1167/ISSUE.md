**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads
**Severity**: MEDIUM

## Observation

`WorldStreamingState` has no `Drop` impl (verified: `grep "impl Drop for WorldStreamingState" byroredux/src/streaming.rs` returns no matches). The worker join handshake (`state.shutdown(Duration::from_secs(1))`) is wired ONLY into `WindowEvent::CloseRequested` at `byroredux/src/main.rs:1074`.

Eight other `event_loop.exit()` call sites bypass the shutdown handshake:
- `main.rs:938` — window-create failure
- `main.rs:948` — raw-handle failure
- `main.rs:1011` — Vulkan ctx init failure
- `main.rs:1085` — resize failure
- `main.rs:1267`, `:1275` — redraw failure
- `main.rs:1594` — `--bench-frames` natural-exit (see #CONC-D6-NEW-02)

On any of these paths, `App` is dropped without `state.shutdown(...)` running. Field drop order on `App::Drop` closes `request_tx` and then drops `worker: Option<JoinHandle<()>>` — the latter is a plain `JoinHandle` drop which **detaches** the thread.

## Why it's a bug

Re-introduces the exact behaviour #856 closed. Worker is detached, may be mid-`BsaArchive::extract()`. Worker holds an `Arc<TextureProvider>` keeping BSA file handles open until its current loop iteration completes. Process exit may hang briefly on Windows where pending I/O blocks process termination.

## Trigger Conditions

Process exit via any path other than `WindowEvent::CloseRequested`. The `--bench-frames` path is the most-trafficked (every CI / nightly bench run on the FNV WastelandNV path).

## Fix

Prefer (a):

**(a)** Implement `impl Drop for WorldStreamingState`:

```rust
impl Drop for WorldStreamingState {
    fn drop(&mut self) {
        // Take the handle so it doesn't also get dropped at end of scope.
        if let Some(worker) = self.worker.take() {
            // Close the request channel to signal the worker.
            drop(self.request_tx.take());
            let _ = join_with_timeout(worker, std::time::Duration::from_secs(1));
        }
    }
}
```

Detach becomes impossible regardless of exit path. Field drop order in `App::Drop` will run this automatically.

**(b)** Factor `App` teardown into `fn shutdown(&mut self)` and call from every `event_loop.exit()` site. More invasive and easier to forget when adding a new exit path.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: check whether `UiManager` or any other long-lived background resource has the same "shutdown only on CloseRequested" pattern
- [ ] **DROP**: confirm `WorldStreamingState::Drop` runs before `App::Drop`'s other Vulkan teardown — channel close + join must complete before BSA file handles are dropped
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test that creates a `WorldStreamingState`, drops it (without calling `shutdown`), and asserts the worker thread joins within timeout

## Related

- #856 — original streaming worker join fix (only wired CloseRequested)
- CONC-D6-NEW-02 — `--bench-frames` sub-case specifically affects the CI hot path
