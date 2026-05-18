**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads
**Severity**: LOW

## Observation

`crates/debug-server/src/listener.rs:194-238`:

```rust
loop {
    if shutdown.load(Ordering::Acquire) {
        return;
    }
    match listener.accept() {
        Ok((stream, addr)) => {
            // Don't accept new clients after shutdown was signalled
            if shutdown.load(Ordering::Acquire) {
                drop(stream);
                return;
            }
            // ... [stream registry push at lines 218-221] ...
            thread::Builder::new()
                .name(format!("byro-debug-client-{}", addr))
                .spawn(move || handle_client(stream_arc, q, s))
                .ok();
        }
        ...
    }
}
```

Two shutdown checks (lines 195, 204). The second check, after `accept()` returns, drops the stream if shutdown was signalled mid-accept. **However**, between the registry push (lines 218-221) and the spawn (lines 224-227), there is a window where a per-client thread is spawned for a stream that would otherwise have been dropped.

## Why it's a bug

Mostly benign — the per-client thread self-terminates on its `recv` (shutdown check at line 276), and #1009's socket-side shutdown (`active_streams shutdown(Both)`) unblocks idle reads promptly. Cost is one transient thread spawn during teardown. Not a deadlock or use-after-free.

## Trigger Conditions

A client `connect()` syscall completes between the `if shutdown.load(...)` check at line 195 and `WouldBlock` sleep — and shutdown is signalled between the registry push and thread spawn.

## Fix

Trivial reorder. Move the `shutdown.load` check at line 204 to AFTER `active_streams.lock()` and BEFORE the spawn, so a shutdown signal observed between accept and spawn correctly drops the stream without spawning:

```rust
let stream_arc = Arc::new(stream);
{
    let mut active = active_streams.lock().unwrap();
    if shutdown.load(Ordering::Acquire) {
        drop(stream_arc);  // also closes the socket
        return;
    }
    active.retain(|w| w.upgrade().is_some());
    active.push(Arc::downgrade(&stream_arc));
}
let q = queue.clone();
let s = Arc::clone(&shutdown);
thread::Builder::new()
    .name(format!("byro-debug-client-{}", addr))
    .spawn(move || handle_client(stream_arc, q, s))
    .ok();
```

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: any other accept-then-register-then-spawn pattern in the codebase (probably none)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: confirm `active_streams` lock is dropped before spawn (no lock-held thread::spawn)
- [ ] **FFI**: N/A
- [ ] **TESTS**: existing `dropping_handle_joins_listener_thread` test should still pass; add a stress test that hammers connect + shutdown

## Related

- #1009 — active_streams registry (already mitigates the practical impact)
