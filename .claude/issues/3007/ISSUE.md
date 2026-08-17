# RT-2026-08-16-08: debug server announces listening regardless of whether the bind succeeded

**Issue**: #3007
**Severity**: MEDIUM
**Dimension**: Debug server
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md`.

**Location**: `crates/debug-server/src/lib.rs`:27-39 · `crates/debug-server/src/listener.rs`
**Status note**: NEW — the **code-level residual of #1619**, which was closed by editing skill text rather than the code.

## Description

`start()` logs `"Debug server listening on 127.0.0.1:{port}"` **unconditionally**, before and independent of whether `listener_loop`'s `TcpListener::bind` succeeded. `bench-hold:` then advertises the port on the same unconditional basis.

## Evidence

```rust
// crates/debug-server/src/lib.rs:27-39
pub fn start(scheduler: &mut Scheduler, port: u16) -> DebugServerHandle {
    let (mut drain_system, handle) = listener::spawn(port);
    registration::register_all(drain_system.registry_mut());
    scheduler.add_exclusive(Stage::Late, drain_system);
    // Hostname here mirrors `listener_loop`'s `TcpListener::bind`
    // hardcoded `127.0.0.1` — both must move in lockstep if a
    // future host arg lands. See #857.
    log::info!("Debug server listening on 127.0.0.1:{}", port);
    handle
}
```

The bind happens inside the spawned listener thread; `start` never observes its result. Re-verified 2026-08-17.

## Impact

When the port is already in use — the common case when a second engine instance is launched, which the [No Parallel Engine Launch] rule exists because of — the engine still prints "listening on 127.0.0.1:9876" and `--bench-hold` still advertises it. Anyone attaching `byro-dbg` gets a connection failure that the engine's own log contradicts.

Wasted-debugging cost rather than a correctness bug, which is why it is MEDIUM. Notable that #1619 was closed by editing skill text — the misleading log line itself was never changed.

## Suggested Fix

Have `listener::spawn` report bind success back to `start` (a `oneshot`/`mpsc` or a shared `AtomicBool` suffices) and log either the success or the bind error. `bench-hold:` should advertise the port only on confirmed success.

## Related

- #1619 (closed by editing skill text; this is the untouched code half)
- #857 (the hostname lockstep comment in the same function)

## Completeness Checks
- [ ] **BIND-TRUTH**: The log reflects the actual bind result, not the intent
- [ ] **BENCH-HOLD**: `--bench-hold`'s advertisement is gated on the same signal
- [ ] **SIBLING**: Any other "listening on" / readiness log in the workspace checked for the same unconditional shape
- [ ] **TESTS**: A regression test binds the port first and asserts the engine reports failure

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3007 --json state` when live state is needed.*
