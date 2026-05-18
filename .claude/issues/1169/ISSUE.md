**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads
**Severity**: LOW

## Observation

`byroredux/src/streaming.rs:272-308` — `join_with_timeout` spawns a watcher thread that owns the joined handle. On `recv_timeout::Timeout`, the watcher is intentionally detached:

```rust
let watcher = std::thread::Builder::new()
    .name("byro-join-watcher".into())
    .spawn(move || {
        let _ = handle.join();
        let _ = done_tx.send(());
    });
match watcher {
    Ok(_watcher_handle) => match done_rx.recv_timeout(timeout) {
        Ok(()) => Ok(()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(JoinTimeout),  // watcher continues to wait, indefinitely
        ...
    },
    ...
}
```

The watcher continues to wait for the worker, holding the worker's `JoinHandle` indefinitely. The watcher thread itself is detached (`_watcher_handle` is discarded).

## Why it's a bug

Today's only call site is `WorldStreamingState::shutdown` (one per shutdown). On process teardown the leak is reaped by the OS — zero impact in practice. The function is `pub` and could be called from non-shutdown paths in the future; any such caller leaks one thread + one `Arc`-held resource graph per timeout.

## Trigger Conditions

Streaming worker is mid-`BsaArchive::extract()` (slow disk / network FS) when `shutdown(timeout)` fires and the timeout expires.

## Fix

Two options:

**(a)** Make the policy explicit by renaming to `join_with_timeout_terminal` (callers know it's shutdown-only and a leak is expected on timeout).

**(b)** Convert the watcher into a `try_join` loop using `Thread::is_finished` (stabilised in Rust 1.61):

```rust
pub fn join_with_timeout(
    handle: JoinHandle<()>,
    timeout: std::time::Duration,
) -> Result<(), JoinTimeout> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if handle.is_finished() {
            let _ = handle.join();
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(JoinTimeout)  // handle is dropped → detaches the worker, same as today
}
```

No watcher thread. Caller still leaks the underlying worker on timeout (because `handle` is consumed and dropped), but no auxiliary thread is leaked.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: any other "timeout-or-detach" patterns in the codebase? `grep -rn "is_finished\|join_with_timeout"`
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: existing `join_with_timeout_*` tests in `streaming.rs` should still pass; add one for the timeout path that asserts no extra threads

## Related

- CONC-D6-NEW-01 — only current caller
