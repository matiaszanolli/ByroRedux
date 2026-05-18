**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads (CI-perf)
**Severity**: LOW

## Observation

`crates/debug-server/src/listener.rs:229-231`:

```rust
Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
    // No pending connection — sleep briefly to avoid busy-spin.
    thread::sleep(Duration::from_millis(50));
}
```

Combined with `set_nonblocking(true)` at line 191 — there's no kernel-level way to interrupt the sleep. The shutdown path is well-engineered (#855 / #1009), but the unconditional 50 ms sleep means even an idle listener takes up to 50 ms to honour shutdown.

## Why it's a bug

Mean shutdown latency is 25 ms, worst-case 50 ms. For the test `dropping_handle_joins_listener_thread` (line 348) this is fine (asserted under 2 s). For production teardown stacked with `streaming.shutdown`'s 1 s timeout, cumulative latency to clean process exit can hit ~1.05 s.

Not a correctness bug. Adds up under CI test parallelism if every test spins up a listener.

## Trigger Conditions

`DebugServerHandle::shutdown_and_join` called when the listener is sleeping in the `WouldBlock` branch.

## Fix

Three options, in order of effort:

**(a)** Lowest-effort: reduce sleep to 5 ms. 10× lower shutdown latency, still negligible CPU.

**(b)** Replace bare `thread::sleep` with `(Mutex<()>, Condvar)` pair — `notify_all` on shutdown wakes the listener immediately. Requires reshape: `shutdown: Arc<AtomicBool>` becomes `shutdown: Arc<(Mutex<bool>, Condvar)>`.

**(c)** Switch the listener to `mio` for proper async I/O readiness signalling. Overkill for one socket.

Recommend (a) unless there's a CI-time motivation to go further.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: error-sleep at line 235 (`Duration::from_millis(100)`) — keep at 100 ms (error path, low-frequency) or also reduce
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A (if going (b), Condvar must be careful)
- [ ] **FFI**: N/A
- [ ] **TESTS**: `dropping_handle_joins_listener_thread` — assert join completes in under 100 ms now (was 2 s ceiling)

## Related

- #855 / #1009 — listener join + active_streams shutdown
